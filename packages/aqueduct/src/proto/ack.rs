//! Proto logic and utils for acking / nacking.

use super::{
    TokioUnboundedSender,
    ReceiverCtrlTaskMsg,
};
use crate::{
    frame::write,
    util::abort_on_drop::AbortOnDrop,
};
use std::{
    cell::Cell,
    collections::BTreeMap,
    time::Duration,
    mem::swap,
    iter,
};
use tokio::time::sleep;
use anyhow::*;


// throughout this module we use the following approach to overflow-prevention: we explicitly error
// if we encounter a message num equal to u64::MAX. a well-behaved remote should be assigning
// message nums in serial, and it is implauisble that it would send u64::MAX messages. thereafter,
// we are free throughout this module to compute `n + 1` wherein n was a received message number
// without the fear of overflow. we design our arithmetic expressions around that.


/// Collection of integer ranges stored as a not-necessarily sorted vector.
#[derive(Default, Debug)]
struct VecAckRanges(Vec<(u64, u64)>);

impl VecAckRanges {
    /// Add a single integer as a range. Does not detect overlaps.
    ///
    /// Unspecified behavior occurs if `n` equals `u64::MAX`.
    ///
    /// O(1).
    fn add_point(&mut self, n: u64) {
        debug_assert!(n < u64::MAX);

        if let Some(&mut (_, ref mut end)) = self.0
            .iter_mut().rev().next()
            .filter(|&&mut (_, end)| end + 1 == n)
        {
            // this "expand range" case is just a memory optimization
            *end = n;
        } else {
            self.0.push((n, n));
        }
    }

    /// Sort ranges and produce a draining iterator over them which merges contiguous ranges,
    /// detects overlaps as it scans, and eventually leaves `self` in a default state.
    ///
    /// O(n log n).
    fn sort_merge_drain<'a>(&'a mut self) -> impl Iterator<Item=Result<(u64, u64)>> + 'a {
        // this is expected to probably be mostly sorted, wherein unstable-sort would be slower
        self.0.sort_by_key(|&(start, _)| start);
        let mut sorted = self.0.drain(..).peekable();
        iter::from_fn(move || -> Option<Result<(u64, u64)>> {
            let (a, mut b) = sorted.next()?;
            debug_assert!(b >= a);
            debug_assert!(a < u64::MAX);
            debug_assert!(b < u64::MAX);
            while let Some((a2, b2)) = sorted.next_if(|&(a2, _)| a2 <= b + 1) {
                debug_assert!(b2 >= a2);
                debug_assert!(a2 < u64::MAX);
                debug_assert!(b2 < u64::MAX);
                if a2 <= b {
                    return Some(Err(anyhow!("message nums received in duplicate")));
                }
                debug_assert!(a2 == b + 1);
                b = b2;
            }
            Some(Ok((a, b)))
        })
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}


/// Collection of integer ranges stored as a b-tree.
#[derive(Default, Debug)]
struct TreeAckRanges(BTreeMap<Cell<u64>, u64>);

impl TreeAckRanges {
    /// Add an integer range. Error on overlap.
    ///
    /// Unspecific behavior occurs if the range overlaps `u64::MAX` or if the range has a
    /// non-positive length.
    ///
    /// O(log n).
    fn add_range(&mut self, a: u64, b: u64) -> Result<()> {
        // note: using Cell to modify the b-tree key should be fine so long as we don't change its
        //       place relative to its neighbors.
        debug_assert!(b >= a);
        debug_assert!(b < u64::MAX);
        let mut range = self.0.range_mut(..=Cell::new(b + 1));
        if let Some((a2, b2)) = range.next_back() {
            debug_assert!(*b2 >= a2.get());
            debug_assert!(a2.get() < u64::MAX);
            debug_assert!(*b2 < u64::MAX);
            if a2.get() == b + 1 {
                // [a, b] merges with (at least) existing [a2, b2] on its right
                if let Some((a3, b3)) = range.next_back() {
                    debug_assert!(*b3 >= a3.get());
                    debug_assert!(a3.get() < u64::MAX);
                    debug_assert!(*b3 < u64::MAX);
                    debug_assert!(*b3 + 1 < a2.get());
                    if *b3 >= a {
                        // [a, b] intersects with existing [a3, b3]
                        return Err(anyhow!("message nums received in duplicate"));
                    } else if *b3 + 1 == a {
                        // [a, b] merges with existing [a3, b3] on its left, in addition to [a2, b2]
                        // 1. modify tree entry [a3, b3] -> [a3, b2]
                        // 2. delete tree entry [a2, b2]
                        *b3 = *b2;
                        let a2 = a2.clone();
                        self.0.remove(&a2);
                    } else {
                        // [a, b] only merges with [a2, b2]
                        // - modify tree entry [a2, b2] -> [a, b2]
                        a2.set(a);
                    }
                } else {
                    // [a, b] only merges with [a2, b2]
                    // - modify tree entry [a2, b2] -> [a, b2]
                    a2.set(a);
                }
            } else {
                if *b2 >= a {
                    // [a, b] intersects with existing [a2, b2]
                    return Err(anyhow!("message nums received in duplicate"));
                } else if *b2 + 1 == a {
                    // [a, b] merges with (only) existing [a2, b2] on its left
                    // - modify tree entry [a2, b2] -> [a2, b]
                    *b2 = b;
                } else {
                    // [a, b] does not merge with anything
                    // - insert tree entry [a, b]
                    self.0.insert(Cell::new(a), b);
                }
            }
        } else {
            // [a, b] does not merge with anything
            // - insert tree entry [a, b]
            self.0.insert(Cell::new(a), b);
        }
        Ok(())
    }

    /// Get the first integer that is not within a range. 
    ///
    /// O(1).
    fn first_unacked_point(&self) -> u64 {
        self.0.iter()
            .next()
            .filter(|&(ref a, _)| a.get() == 0)
            .map(|(_, &b)| b + 1)
            .unwrap_or_default()
    }
}


/// Receiver-side manager for acking and nacking unreliable messages.
#[derive(Default, Debug)]
pub struct ReceiverUnreliableAckManager {
    // invariants:
    // - before the send stream is Some, we never have the timer set.
    // - once the send stream is Some, we always have the timer set iff
    //   `highest_seen_plus_1 > ack_nacked_below`.

    /// We have acked or nacked all unreliable message nums below this.
    ack_nacked_below: u64,
    /// The highest unreliable message num we are aware of the remote having sent plus 1, or zero
    /// if we are not aware of the remote ever having sent a message unreliably.
    highest_seen_plus_1: u64,
    /// If we currently have the timer set, a number n such that upon receiving the timer event we
    /// will ack or nack all unreliable message nums below n.
    set_timer_ack_nack_below: Option<(u64, AbortOnDrop)>,
    /// - If we currently have the timer set, unreliable message nums we have received and not yet
    ///   acked which are less than the timer's n value.
    /// - If we do not currently have the timer set, all unreliable message nums we have received
    ///   and not yet acked.
    unacked_below_set_timer: VecAckRanges,
    /// - If we currently have the timer set, unreliable message nums we have received and not yet
    ///   acked which are greater than or equal to the timer's n value.
    /// - If we do not currently have the timer set, this should be empty.
    unacked_not_below_set_timer: VecAckRanges,
}

impl ReceiverUnreliableAckManager {
    /// Mark an unreliable message number as being received, so that it will be acked in the
    /// future, or return `Ok(true)` if the message has already been nacked and thus must be
    /// ignored.
    ///
    /// (Technically, this can also return `Ok(true)` if the message has already been acked rather
    /// than nacked, but that's an "on your head then be it" situation w.r.t. the remote.)
    pub fn on_receive(
        &mut self,
        n: u64,
        send_ctrl_task_msg: &TokioUnboundedSender<ReceiverCtrlTaskMsg>,
        send_stream_is_some: bool,
    ) -> Result<bool> {
        if n < self.ack_nacked_below {
            return Ok(true);
        }
        let n_plus_1 = n.checked_add(1).ok_or_else(|| anyhow!("remote message num too high"))?;
        self.highest_seen_plus_1 = self.highest_seen_plus_1.max(n_plus_1);
        if let Some(&(stanb, _)) = self.set_timer_ack_nack_below.as_ref() {
            if n < stanb {
                self.unacked_below_set_timer.add_point(n);
            } else {
                self.unacked_not_below_set_timer.add_point(n);
            }
        } else {
            self.unacked_below_set_timer.add_point(n);
            if send_stream_is_some {
                self.start_timer(send_ctrl_task_msg);
            }
        }
        Ok(false)
    }

    /// Mark an unreliable message number as being declared sent by the remote side, so that it
    /// will be nacked within a reasonable amount of time if not acked (or already nacked).
    pub fn on_declared(
        &mut self,
        n: u64,
        send_ctrl_task_msg: &TokioUnboundedSender<ReceiverCtrlTaskMsg>,
        send_stream_is_some: bool,
    ) -> Result<()> {
        let n_plus_1 = n.checked_add(1).ok_or_else(|| anyhow!("remote message num too high"))?;
        self.highest_seen_plus_1 = self.highest_seen_plus_1.max(n_plus_1);
        
        // maybe trigger the timer start because of this
        if self.set_timer_ack_nack_below.is_none()
            && self.highest_seen_plus_1 > self.ack_nacked_below
            && send_stream_is_some
        {
            debug_assert!(self.unacked_below_set_timer.is_empty());
            debug_assert!(self.unacked_not_below_set_timer.is_empty());            
            self.start_timer(send_ctrl_task_msg);
        }

        Ok(())
    }

    /// Call upon receiving an `UnreliableAckTimer` task message (self may send it to itself).
    pub async fn on_timer_event(
        &mut self,
        send_ctrl_task_msg: &TokioUnboundedSender<ReceiverCtrlTaskMsg>,
        send_stream: &mut Option<quinn::SendStream>,
    ) -> Result<()> {
        // unwrap safety: we only spawn a task to send an UnreliableAckTimer task message back to
        //                self when transitioning set_timer_ack_nack_below to Some, and only
        //                transition it to None once that task message is received (here).
        let (stanb, _) = self.set_timer_ack_nack_below.take().unwrap();
        // unwrap safety: we only start the ack timer when send_stream is Some.
        let send_stream = send_stream.as_mut().unwrap();

        // encode and send acks and nacks and update internal state
        let mut w = write::Frames::default();
        let mut pnr = write::PosNegRanges::default();
        for r in self.unacked_below_set_timer.sort_merge_drain() {
            let (a, b) = r?;
            debug_assert!(b >= a);
            debug_assert!(a >= self.ack_nacked_below);
            pnr.neg_delta(a - self.ack_nacked_below);
            pnr.pos_delta(b - a + 1);
            self.ack_nacked_below = b + 1;
        }
        debug_assert!(self.ack_nacked_below <= stanb);
        pnr.neg_delta(stanb - self.ack_nacked_below);
        self.ack_nacked_below = stanb;
        w.ack_nack_unreliable(pnr);
        w.send_on_stream(send_stream).await?;

        // possibly immediately restart timer
        debug_assert!(self.unacked_below_set_timer.is_empty());
        if self.highest_seen_plus_1 > self.ack_nacked_below {
            swap(&mut self.unacked_below_set_timer, &mut self.unacked_not_below_set_timer);
            self.start_timer(send_ctrl_task_msg);
        } else {
            debug_assert!(self.unacked_not_below_set_timer.is_empty());
        }

        Ok(())
    }

    /// Call upon `send_stream` transitioning from `None` to `Some`, if it was previously `None`.
    pub fn on_ctrl_stream_opened(
        &mut self,
        send_ctrl_task_msg: &TokioUnboundedSender<ReceiverCtrlTaskMsg>,
    ) {
        debug_assert!(self.set_timer_ack_nack_below.is_none());
        if self.highest_seen_plus_1 > 0 {
            self.start_timer(send_ctrl_task_msg);
        }
    }

    fn start_timer(&mut self, send_ctrl_task_msg: &TokioUnboundedSender<ReceiverCtrlTaskMsg>) {
        debug_assert!(self.set_timer_ack_nack_below.is_none());
        self.set_timer_ack_nack_below = Some((
            self.highest_seen_plus_1,
            AbortOnDrop::spawn({
                let send_ctrl_task_msg = send_ctrl_task_msg.clone();
                async move {
                    sleep(Duration::from_secs(1)).await;
                    let _ = send_ctrl_task_msg.send(ReceiverCtrlTaskMsg::UnreliableAckTimer);
                }
            }),
        ));
    }
}
