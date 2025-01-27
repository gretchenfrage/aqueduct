
use crate::{
    misc::remove_first,
    codec::write,
};
use std::{
    time::Duration,
    task::{Context, Poll},
    future::Future,
    pin::Pin,
    collections::BTreeMap,
    cell::Cell,
};
use tokio::time::{Sleep, sleep};
use anyhow::{Error, ensure, anyhow};


const ACK_DELAY: Duration = Duration::from_secs(1);

// timer for letting acks aggregate before sending.
//
// begins in a "stopped" state, where it pends forever. calling start starts the timer unless the
// timer is already running. after a delay, it resolves to the value it was started with, and thus
// transitions back to a stopped state, whereupon it may be started again later.
pub(crate) struct AckTimer<T>(Option<(Sleep, T)>);

impl<T: Copy> Future for AckTimer<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<T> {
        unsafe {
            let this = self.get_unchecked_mut();
            if let &mut Some((ref mut sleep, val)) = &mut this.0 {
                match Pin::new_unchecked(sleep).poll(cx) {
                    Poll::Pending => Poll::Pending,
                    Poll::Ready(()) => {
                        this.0 = None;
                        Poll::Ready(val)
                    }
                }
            } else {
                Poll::Pending
            }
        }
    }
}

impl<T> AckTimer<T> {
    pub(crate) fn new() -> Self {
        AckTimer(None)
    }

    // start the timer if it's not currently running.
    pub(crate) fn start(self: &mut Pin<&mut Self>, val: T) {
        unsafe {
            let this = self.as_mut().get_unchecked_mut();
            if this.0.is_none() {
                this.0 = Some((sleep(ACK_DELAY), val));
            }
        }
    }
}

#[derive(Default)]
struct UnsortedRanges(Vec<(u64, u64)>);

impl UnsortedRanges {
    fn add(&mut self, n: u64) {
        if let Some(&mut (_, ref mut end)) = self.0
            .iter_mut().rev().next()
            .filter(|&&mut (_, end)| end + 1 == n)
        {
            // this "expand range" case is just an optimization
            *end = n;
        } else {
            self.0.push((n, n));
        }
    }

    fn sort(&mut self) {
        // this is expected to probably be mostly sorted, therefore unstable-sort could be slower.
        self.0.sort_by_key(|&(start, _)| start);
    }
}

// receiver-side state for acking and nacking reliable messages, other than the timer.
#[derive(Default)]
pub(crate) struct ReceiverReliableAck {
    // inclusive ranges of reliable message nums that have been acked, represented as map from
    // start to end. invariant: continuous ranges are merged.
    acked: BTreeMap<Cell<u64>, u64>,
    // not-necessarily-sorted list of inclusive ranges of reliable message nums that have been
    // received and not yet acked
    not_acked: UnsortedRanges,
}

impl ReceiverReliableAck {
    pub(crate) async fn on_timer_zero(
        &mut self,
        chan_ctrl: &mut quinn::SendStream,
    ) -> Result<(), Error> {
        // all reliable message nums less than this have been acked
        let mut fully_acked = self.acked
            .first_key_value()
            .filter(|&(start, _)| start.get() == 0)
            .map(|(_, &end)| end + 1)
            .unwrap_or(0);

        // sort the ranges. if there are overlaps, that will be detected as a protocol error later
        self.not_acked.sort();

        // loop through the ranges and:
        // - encode acks
        // - update self.acked
        // - detect duplicate message nums

        let mut acks = write::PosNegRanges::default();
        let mut prev_end = None;

        for (start, end) in self.not_acked.0.drain(..) {
            // validate not acked range
            debug_assert!(end >= start, "negative length range");
            ensure!(
                prev_end.is_none_or(|prev_end| prev_end < start),
                "reliable message number received in duplicate"
            );
            prev_end = Some(end);

            // encode new ack
            acks.neg_delta(start - fully_acked);
            acks.pos_delta(end + 1 - start);
            fully_acked = end + 1;

            // merge into tree
            let mut btree_range = self.acked.range_mut(..=Cell::new(end + 1));

            if let Some((start2, end2)) = btree_range.next_back() {
                debug_assert!(start2.get() >= *end2, "negative length range");
                if start2.get() == end + 1 {
                    // new range merges with existing range on its right

                    // modifying the key is ok because its maintains its order relative to other
                    // present keys.
                    start2.set(start);

                    if let Some((start3, end3)) = btree_range.next_back() {
                        debug_assert!(start3.get() >= *end3, "negative length range");
                        debug_assert!(*end3 < start2.get() - 1, "non-merged ranges");
                        ensure!(*end3 < start, "reliable message number received in duplicate");
                        if *end3 == start - 1 {
                            // new range also merges with existing range on its left
                            *end3 = *end2;

                            let start2 = start2.clone();
                            self.acked.remove(&start2);
                        }
                    }
                } else {
                    ensure!(*end2 < start, "reliable message number received in duplicate");
                    if *end2 == start - 1 {
                        // new range merges with existing range on its left
                        *end2 = end;
                    }
                }
            } else {
                // new range does not merge with any existing range
                self.acked.insert(Cell::new(start), end);
            }
        }

        // encode and send the ack frame
        let mut wframes = write::Frames::default();
        wframes.ack_reliable(acks);
        wframes.send_stream(chan_ctrl).await?;

        Ok(())
    }

    pub(crate) fn on_message(&mut self, message_num: u64, timer: &mut Pin<&mut AckTimer<()>>) {
        self.not_acked.add(message_num);
        timer.start(());
    }
}

// receiver-side state for acking and nacking unreliable messages, other than the timer.
#[derive(Default)]
pub(crate) struct ReceiverUnreliableAck {
    // all unreliable message nums below this have been acked or nacked (and none above have)
    ack_nacked: u64,
    // the sender has declared having sent all unreliable message nums below this
    declared_sent: u64,
    // not-necessarily-sorted list of inclusive ranges of unreliable message nums that have been
    // received and not yet acked
    not_acked: UnsortedRanges,
}

impl ReceiverUnreliableAck {
    // call whenever the unreliable ack timer finishes, with the timer value.
    pub(crate) async fn on_timer_zero(
        &mut self,
        timer_val: u64,
        timer: &mut Pin<&mut AckTimer<u64>>,
        chan_ctrl: &mut quinn::SendStream,
    ) -> Result<(), Error> {
        // the timer holds the value of declared_sent at the point when it was started, and
        // finishes after the loss detection duration. thus, when it finishes, all unreliable
        // message nums below that must be acked or nacked.
        let must_ack_nack = timer_val;

        // assertion safety:
        // - must_ack_nack equals what declared_sent was when the timer was started
        // - the timer is only started when declared_sent is greater than ack_nacked
        // - ack_nacked only changes when the timer stops, and only to a value less than or equal
        //   to declared_sent
        debug_assert!(must_ack_nack > self.ack_nacked);

        // sort the ranges. if there are overlaps, that will be detected as a protocol error later
        self.not_acked.0.sort_by_key(|&(start, _)| start);

        // encode acks and nacks, increase ack_nacked as we go
        let mut ack_nacks = write::PosNegRanges::default();

        let mut prev_end = None;
        let mut num_ranges_fully_ack = 0;

        for (i, &mut (ref mut start, end)) in self.not_acked.0.iter_mut().enumerate() {
            // assertion safety: this would've already been caught as a protocol error
            debug_assert!(*start >= self.ack_nacked);
            debug_assert!(end >= *start); // sanity check

            // detect overlapping ranges
            ensure!(
                prev_end.is_none_or(|prev_end| prev_end < *start),
                "unreliable message number received in duplicate"
            );
            prev_end = Some(end);

            // decide how much of this range was declared long enough ago to ack
            debug_assert_eq!(num_ranges_fully_ack, i); // sanity check
            let ack_end = if end < must_ack_nack {
                // ack entire range
                num_ranges_fully_ack += 1;
                end
            } else if *start < must_ack_nack {
                // ack part of range
                *start = must_ack_nack;
                must_ack_nack - 1
            } else {
                // ack none of range
                break;
            };

            // nack gap (the writer handles filtering & merging automatically)
            ack_nacks.neg_delta(*start - self.ack_nacked);
            self.ack_nacked = *start;

            // ack
            ack_nacks.pos_delta(ack_end + 1 - self.ack_nacked);
            self.ack_nacked = ack_end + 1;

            // break if we're only partially acking this range
            if num_ranges_fully_ack == i {
                break;
            }
        }

        // garbage collect fully acked ranges
        remove_first(&mut self.not_acked.0, num_ranges_fully_ack);

        // trailing nack
        ack_nacks.neg_delta(must_ack_nack - self.ack_nacked);
        self.ack_nacked = must_ack_nack;

        // if additional messages were declared to have been sent since the timer was started,
        // immediately start it again
        if self.declared_sent > self.ack_nacked {
            timer.start(self.declared_sent);
        }

        // encode and send the ack-nack frame
        let mut wframes = write::Frames::default();
        wframes.ack_nack_unreliable(ack_nacks);
        wframes.send_stream(chan_ctrl).await?;

        Ok(())
    }

    // call whenever a Message frame is received unreliably.
    pub(crate) fn on_message(&mut self, message_num: u64) -> Result<(), AlreadyNacked> {
        if message_num < self.ack_nacked {
            return Err(AlreadyNacked);
        }

        self.not_acked.add(message_num);
        Ok(())
    }

    // call whenever a SentUnreliable frame is received.
    pub(crate) fn on_sent_unreliable(
        &mut self,
        delta: u64,
        timer: &mut Pin<&mut AckTimer<u64>>,
    ) -> Result<(), Error> {
        self.declared_sent = self.declared_sent
            .checked_add(delta)
            .ok_or_else(|| anyhow!("SentUnreliable message num overflowed"))?;
        timer.start(self.declared_sent);
        Ok(())
    }
}

// error for receiving an unreliable message we have already nacked. it must be ignored.
pub(crate) struct AlreadyNacked;
