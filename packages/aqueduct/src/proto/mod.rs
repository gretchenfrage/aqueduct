mod per_chan_id_bit;
mod range_set;

use self::{
    per_chan_id_bit::{PerCreatorSide, PerIsOneshot, PerSenderSide},
    range_set::RangeSetU64,
};
use crate::frame::{common::*, read, write};
use dashmap::DashMap;
use multibytes::MultiBytes;
use quinn;
use std::{
    sync::{
        Arc, RwLock,
        atomic::{AtomicU64, Ordering::Relaxed},
    },
    time::Duration,
};

struct Connection {
    // whether the local side of the connection is the client or the server side
    side: Side,
    // the underlying QUIC connection
    quic_connection: quinn::Connection,
    // the next channel ID indexes for channel IDs the local side creates
    next_chan_id_idxs: PerSenderSide<PerIsOneshot<AtomicU64>>,
    // senders that currently exist for which the local side is the sender side
    senders: DashMap<ChanId, SenderState>,
    // receivers that currently exist for which the local side is the receiver side
    receivers: DashMap<ChanId, ReceiverState>,
    // the idx part of all chan IDs that have been removed from self.senders or self.receivers and
    // for which the remote side is the creator side
    removed_channels: PerSenderSide<PerIsOneshot<RwLock<RangeSetU64>>>,
}

#[derive(Default)]
struct SenderState {
    received_creating_message: bool,
}

#[derive(Default)]
struct ReceiverState {
    // whether the local side has received the message that this channel was attached to, or true
    // this is the entrypoint channel
    received_creating_message: bool,
    // buffer of all messages we have received on this channel
    received_messages: Vec<ReceivedMessage>,

    // set of reliable message numbers the local side has received and sent back acks for
    reliable_received_acked: RangeSetU64,
    // set of reliable message numbers the local side has received but not sent acks for
    reliable_received_unacked: RangeSetU64,

    // set of unreliable message numbers the local side has ack-nacked (exclusive upper bound)
    unreliable_ack_nacked_lt: u64,
    // set of unreliable message numbers the local side has received and not acked
    unreliable_received_unacked: RangeSetU64,
    // sum of count values of all sent_unreliable frames local side has received
    sent_unreliable_count_total: u64,

    // if the local side has received a finish_sender frame, that frame's sent_reliable number
    finished_sent_reliable: Option<u64>,

    // the receiver-to-sender feedback control stream for this channel. we utilize tokio Mutex's
    // strict FIFO guarantees to form a queue of control data to asynchronously write without
    // holding a receiver state lock across await boundaries.
    ctrl_stream: Arc<tokio::sync::Mutex<Option<quinn::SendStream>>>,
}

impl ReceiverState {
    fn reliable_ack_task_should_exist(&self) -> bool {
        !self.reliable_received_unacked.is_empty()
    }

    fn unreliable_ack_nack_task_should_exist(&self) -> bool {
        !self.unreliable_received_unacked.is_empty()
            || self.sent_unreliable_count_total > self.unreliable_ack_nacked_lt
    }

    fn should_be_closed(&self) -> bool {
        self.finished_sent_reliable
            .is_some_and(|n| n == self.reliable_received_acked.min_absent())
            && !self.reliable_ack_task_should_exist()
            && !self.unreliable_ack_nack_task_should_exist()
    }
}

struct ReceivedMessage {
    message_headers: Vec<(MultiBytes, MultiBytes)>,
    attached_channels: Vec<ReceivedMessageAttachedChannel>,
    payload: MultiBytes,
}

struct ReceivedMessageAttachedChannel {
    chan_id: ChanId,
    channel_headers: Vec<(MultiBytes, MultiBytes)>,
}

impl Connection {
    fn new(side: Side, quic_connection: quinn::Connection) -> Arc<Self> {
        let this = Arc::new(Connection {
            side,
            quic_connection,
            next_chan_id_idxs: Default::default(),
            senders: Default::default(),
            receivers: Default::default(),
            removed_channels: Default::default(),
        });

        // initialize entrypoint channel
        if side == Side::CLIENT {
            this.next_chan_id_idxs[ChanId::ENTRYPOINT][ChanId::ENTRYPOINT].store(1, Relaxed);
            this.senders.insert(
                ChanId::ENTRYPOINT,
                SenderState {
                    received_creating_message: true,
                    ..Default::default()
                },
            );
        } else {
            this.receivers.insert(
                ChanId::ENTRYPOINT,
                ReceiverState {
                    received_creating_message: true,
                    ..Default::default()
                },
            );
        }

        // handle streams
        this.spawn({
            let this = this.clone();
            async move {
                loop {
                    let stream = this.quic_connection.accept_uni().await?;
                    let r = read::Frames::from_stream(stream);
                    this.spawn({
                        let this = this.clone();
                        async move { this.handle_frames(r, true).await }
                    });
                }
            }
        });

        // handle datagrams
        this.spawn({
            let this = this.clone();
            async move {
                loop {
                    let datagram = this.quic_connection.read_datagram().await?;
                    let r = read::Frames::from_datagram(datagram);
                    this.spawn({
                        let this = this.clone();
                        async move { this.handle_frames(r, false).await }
                    });
                }
            }
        });

        this
    }

    fn spawn(&self, f: impl Future<Output = read::Result<()>> + Send + 'static) {
        tokio::task::spawn(async move {
            match f.await {
                Ok(()) => (),
                Err(read::Error::Reset) => trace!("received stream reset"),
                Err(read::Error::Other(e)) => {
                    error!(%e, "unrecoverable error in connection task");
                    todo!()
                }
            }
        });
    }

    async fn handle_frames(self: &Arc<Self>, r: read::Frames, reliable: bool) -> read::Result<()> {
        while let Some(r) = r.frame().await? {
            match r {
                read::Frame::Version(r) => todo!(),
                read::Frame::AckVersion(r) => todo!(),
                read::Frame::ConnectionHeaders(r) => todo!(),
                read::Frame::RouteTo(r) => return self.handle_routed_frames(r, reliable).await,
                read::Frame::Message(_) => read::bail!("received un-routed MESSAGE frame"),
                read::Frame::SentUnreliable(_) => {
                    read::bail!("received un-routed SENT_UNRELIABLE frame")
                }
                read::Frame::FinishSender(_) => {
                    read::bail!("received un-routed FINISH_SENDER frame")
                }
                read::Frame::CancelSender(r) => todo!(),
                read::Frame::AckReliable(r) => todo!(),
                read::Frame::AckNackUnreliable(r) => todo!(),
                read::Frame::CloseReceiver(r) => todo!(),
                read::Frame::ForgetChannel(r) => todo!(),
            }
        }
        Ok(())
    }

    async fn handle_routed_frames(
        self: &Arc<Self>,
        r: read::RouteTo,
        reliable: bool,
    ) -> read::Result<()> {
        let read::RouteTo { chan_id, mut next } = r;
        while let Some(r) = next.frame().await? {
            next = match r {
                read::Frame::Version(r) => todo!(),
                read::Frame::AckVersion(r) => todo!(),
                read::Frame::ConnectionHeaders(r) => todo!(),
                read::Frame::RouteTo(_) => read::bail!("received routed ROUTE_TO frame"),
                read::Frame::Message(r) => self.handle_message_frame(chan_id, reliable, r).await?,
                read::Frame::SentUnreliable(r) => self.handle_sent_unreliable_frame(chan_id, r)?,
                read::Frame::FinishSender(r) => self.handle_finish_sender_frame(chan_id, r)?,
                read::Frame::CancelSender(r) => todo!(),
                read::Frame::AckReliable(r) => todo!(),
                read::Frame::AckNackUnreliable(r) => todo!(),
                read::Frame::CloseReceiver(r) => todo!(),
                read::Frame::ForgetChannel(r) => todo!(),
            };
        }
        Ok(())
    }

    async fn handle_message_frame(
        self: &Arc<Self>,
        chan_id: ChanId,
        reliable: bool,
        r: read::Message,
    ) -> read::Result<read::Frames> {
        // read message, do basic stateless validation
        let read::Message {
            message_num,
            message_headers: mut r,
        } = r;
        read::ensure!(
            message_num < u64::MAX,
            "received MESSAGE frame with message num u64::MAX"
        );
        read::ensure!(
            chan_id.sender() != self.side,
            "received MESSAGE frame for channel ID for which local side is the sender"
        );
        let mut message_headers = Vec::new();
        while r.remaining_bytes() > 0 {
            let (message_header_key, message_header_val) = r.header().await?;
            message_headers.push((message_header_key, message_header_val));
        }
        let mut r = r.done().await?;
        let mut attached_channels = Vec::new();
        while r.remaining_bytes() > 0 {
            r = {
                let r = r.attachment().await?;
                let read::MessageAttachment {
                    channel: attached_chan_id,
                    channel_headers: mut r,
                } = r;
                let mut attached_channel_headers = Vec::new();
                while r.remaining_bytes() > 0 {
                    let (attached_channel_header_key, attached_channel_header_val) =
                        r.header().await?;
                    attached_channel_headers
                        .push((attached_channel_header_key, attached_channel_header_val));
                }
                attached_channels.push(ReceivedMessageAttachedChannel {
                    chan_id: attached_chan_id,
                    channel_headers: attached_channel_headers,
                });
                r.done()
            };
        }
        let r = r.done().await?;
        let (payload, r) = r.payload().await?;
        let received_message = ReceivedMessage {
            message_headers,
            attached_channels,
            payload,
        };

        // lookup receiver / implicitly create receiver / short-circuit due to receiver not
        // existing (either with reset error or fatal error)
        let mut receiver_entry = self.setup_receiver_state(chan_id)?;
        let receiver = receiver_entry.get_mut();

        if reliable {
            // deal with reliable acking
            read::ensure!(
                receiver
                    .finished_sent_reliable
                    .is_none_or(|n| message_num < n),
                "received reliable message num beyond declared finish"
            );
            let task_exists = receiver.reliable_ack_task_should_exist();
            read::ensure!(
                !receiver
                    .reliable_received_unacked
                    .insert(message_num, message_num),
                "received reliable message num in duplicate"
            );
            debug_assert!(receiver.reliable_ack_task_should_exist());
            if !task_exists {
                let this = self.clone();
                self.spawn(async move {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    this.reliable_ack(chan_id).await
                });
            }
        } else {
            // deal with unreliable ack-nacking / reset if previously nacked
            read::ensure!(
                receiver.finished_sent_reliable.is_none()
                    || message_num < receiver.sent_unreliable_count_total,
                "received unreliable message num beyond declared finish",
            );
            if message_num < receiver.unreliable_ack_nacked_lt {
                trace!(
                    "received unreliable message num that has already been nacked (or acked),
                    ignoring"
                );
                return Err(read::Error::Reset);
            }
            let task_exists = receiver.unreliable_ack_nack_task_should_exist();
            read::ensure!(
                !receiver
                    .unreliable_received_unacked
                    .insert(message_num, message_num),
                "received unreliable message num in duplicate"
            );
            if !task_exists {
                self.maybe_spawn_unreliable_ack_nack_task(chan_id, receiver)
            }
        }

        // create attached channels / mark them as having received creating message
        for attached_channel in &received_message.attached_channels {
            read::ensure!(
                attached_channel.chan_id.creator() != self.side,
                "received MESSAGE frame with attached channel ID for which local side is the
                creator"
            );
            if attached_channel.chan_id.sender() == self.side {
                let mut attached_sender = self.setup_sender_state(attached_channel.chan_id)?;
                read::ensure!(
                    !attached_sender.received_creating_message,
                    "received channel ID attached to multiple messages"
                );
                attached_sender.received_creating_message = true;
            } else {
                let mut attached_receiver_entry =
                    self.setup_receiver_state(attached_channel.chan_id)?;
                let attached_receiver = attached_receiver_entry.get_mut();
                read::ensure!(
                    !attached_receiver.received_creating_message,
                    "received channel ID attached to multiple messages"
                );
                attached_receiver.received_creating_message = true;
            }
        }

        // buffer message
        receiver.received_messages.push(received_message);

        Ok(r)
    }

    fn handle_sent_unreliable_frame(
        self: &Arc<Self>,
        chan_id: ChanId,
        r: read::SentUnreliable,
    ) -> read::Result<read::Frames> {
        let read::SentUnreliable { count, next: r } = r;
        let mut receiver_entry = self.setup_receiver_state(chan_id)?;
        let receiver = receiver_entry.get_mut();
        read::ensure!(
            receiver.finished_sent_reliable.is_none(),
            "received SENT_UNRELIABLE frame after receiving FINISH_SENDER frame for same channel",
        );
        let task_exists = receiver.unreliable_ack_nack_task_should_exist();

        receiver.sent_unreliable_count_total = receiver
            .sent_unreliable_count_total
            .checked_add(count)
            .and_then(|n| n.checked_add(1))
            .filter(|&n| n < u64::MAX)
            .ok_or_else(|| {
                read::error!("received overflowing SENT_UNRELIABLE frame count total")
            })?;
        if !task_exists {
            self.maybe_spawn_unreliable_ack_nack_task(chan_id, receiver);
        }
        Ok(r)
    }

    fn handle_finish_sender_frame(
        self: &Arc<Self>,
        chan_id: ChanId,
        r: read::FinishSender,
    ) -> read::Result<read::Frames> {
        let read::FinishSender {
            sent_reliable,
            next: r,
        } = r;
        let mut receiver_entry = self.setup_receiver_state(chan_id)?;
        let receiver = receiver_entry.get_mut();
        read::ensure!(
            receiver.finished_sent_reliable.is_none(),
            "received FINISH_SENDER frame in duplicate"
        );
        debug_assert!(!receiver.should_be_closed());
        receiver.finished_sent_reliable = Some(sent_reliable);
        self.maybe_close_receiver(chan_id, receiver);
        Ok(r)
    }

    fn maybe_close_receiver(self: &Arc<Self>, chan_id: ChanId, receiver: &mut ReceiverState) {
        if !receiver.should_be_closed() {
            return;
        }
        let ctrl_stream_guard_fut = receiver.ctrl_stream.clone().lock_owned();
        let this = self.clone();
        self.spawn(async move {
            let mut ctrl_stream_guard = ctrl_stream_guard_fut.await;
            let ctrl_stream =
                lazy_init_ctrl_stream(&mut *ctrl_stream_guard, &this.quic_connection, chan_id)
                    .await?;
            let mut w = write::Frames::default();
            w.close_receiver();
            w.send_on_stream(ctrl_stream).await?;
            Ok(())
        });
    }

    // get the SenderState for the channel ID if it currently exists. if it used to exist but
    // doesn't any more, return a Reset error. if it never existed, create it with defaults if the
    // channel ID creator side is the remote side, or fatally error if it is the local side.
    fn setup_sender_state(
        &self,
        chan_id: ChanId,
    ) -> read::Result<dashmap::mapref::one::RefMut<'_, ChanId, SenderState>> {
        debug_assert!(chan_id.sender() == self.side);
        self.senders.entry(chan_id).or_try_insert_with(|| {
            if chan_id.creator() == self.side {
                self.ensure_local_created(chan_id)?;
                trace!("received frame containing chan ID which has been removed, ignoring");
                return Err(read::Error::Reset);
            }
            if self.removed_channels[chan_id][chan_id]
                .read()
                .unwrap()
                .contains(chan_id.idx())
            {
                trace!("received frame containing chan ID which has been removed, ignoring");
                return Err(read::Error::Reset);
            }
            Ok(SenderState::default())
        })
    }

    // get the ReceiverState for the channel ID if it currently exists. if it used to exist but
    // doesn't any more, return a Reset error. if it never existed, create it with defaults if the
    // channel ID creator side is the remote side, or fatally error if it is the local side.
    fn setup_receiver_state(
        &self,
        chan_id: ChanId,
    ) -> read::Result<dashmap::mapref::entry::OccupiedEntry<'_, ChanId, ReceiverState>> {
        debug_assert!(chan_id.sender() != self.side);
        match self.receivers.entry(chan_id) {
            dashmap::mapref::entry::Entry::Occupied(entry) => Ok(entry),
            dashmap::mapref::entry::Entry::Vacant(entry) => {
                if chan_id.creator() == self.side {
                    self.ensure_local_created(chan_id)?;
                    trace!("received frame containing chan ID which has been removed, ignoring");
                    return Err(read::Error::Reset);
                }
                if self.removed_channels[chan_id][chan_id]
                    .read()
                    .unwrap()
                    .contains(chan_id.idx())
                {
                    trace!("received frame containing chan ID which has been removed, ignoring");
                    return Err(read::Error::Reset);
                }

                Ok(entry.insert_entry(ReceiverState::default()))
            }
        }
    }

    // error if the channel ID, for which the creator side is the local side, has never been
    // created by the local side.
    fn ensure_local_created(&self, chan_id: ChanId) -> read::Result<()> {
        debug_assert!(chan_id.creator() == self.side);
        read::ensure!(
            chan_id.idx() < self.next_chan_id_idxs[chan_id][chan_id].load(Relaxed),
            "received frame containing chan ID which local side never created"
        );
        Ok(())
    }

    async fn reliable_ack(self: &Arc<Self>, chan_id: ChanId) -> read::Result<()> {
        // lock receiver state
        let Some(mut receiver_guard) = self.receivers.get_mut(&chan_id) else {
            trace!("reliable ack timer elapsed for non-existent receiver, ignoring");
            return Ok(());
        };
        let receiver = &mut *receiver_guard;

        debug_assert!(receiver.reliable_ack_task_should_exist());
        debug_assert!(!receiver.should_be_closed());

        // save this for later (lowest un-acked reliable message num)
        let acked_lt = receiver.reliable_received_acked.min_absent();

        // mark all received message as acked. front-run duplicate detection short-circuiting to
        // avoid arithmetic overflow/underflow edge cases.
        for (s, e) in receiver.reliable_received_unacked.iter() {
            read::ensure!(
                !receiver.reliable_received_acked.insert(s, e),
                "received reliable message num in duplicate",
            );
        }

        // form deltas
        let mut deltas = write::Deltas::default();
        let mut acking = receiver.reliable_received_unacked.iter().peekable();
        // there is always a single ack delta, representing a range length beyond 0
        let mut base = acking
            .next_if(|&(s1, _)| s1 == acked_lt)
            .map(|(s, e)| {
                let new_first_unacked = e + 1;
                deltas.delta(new_first_unacked - s);
                new_first_unacked
            })
            .unwrap_or_else(|| {
                deltas.delta(0);
                acked_lt
            });
        // it is followed by 0 or more (not-ack, ack) delta pairs, representing range lengths
        // beyond 1
        for (s, e) in acking {
            debug_assert!(e < u64::MAX);
            deltas.delta(s - base);
            deltas.delta(e - s);
            base = e + 1;
        }

        // mark all received messages as not unacked (different from marking them as acked)
        receiver.reliable_received_unacked.clear();

        // transition from locking the receiver state to locking the ctrl stream
        let ctrl_stream_guard_fut = receiver.ctrl_stream.clone().lock_owned();

        // maybe finish
        self.maybe_close_receiver(chan_id, receiver);

        drop(receiver_guard);
        let mut ctrl_stream_guard = ctrl_stream_guard_fut.await;

        // transmit the acks
        let ctrl_stream =
            lazy_init_ctrl_stream(&mut *ctrl_stream_guard, &self.quic_connection, chan_id).await?;
        let mut w = write::Frames::default();
        w.ack_reliable(deltas);
        w.send_on_stream(ctrl_stream).await?;

        Ok(())
    }

    fn maybe_spawn_unreliable_ack_nack_task(
        self: &Arc<Self>,
        chan_id: ChanId,
        receiver: &mut ReceiverState,
    ) {
        if !receiver.unreliable_ack_nack_task_should_exist() {
            return;
        }
        let ack_nack_lt = receiver
            .unreliable_received_unacked
            .min_absent()
            .max(receiver.sent_unreliable_count_total);
        let this = self.clone();
        self.spawn(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            this.unreliable_ack_nack(chan_id, ack_nack_lt).await
        });
    }

    async fn unreliable_ack_nack(
        self: &Arc<Self>,
        chan_id: ChanId,
        ack_nack_lt: u64,
    ) -> read::Result<()> {
        // lock receiver state
        let Some(mut receiver_guard) = self.receivers.get_mut(&chan_id) else {
            trace!("unreliable ack timer elapsed for non-existent receiver, ignoring");
            return Ok(());
        };
        let receiver = &mut *receiver_guard;

        debug_assert!(receiver.unreliable_ack_nack_task_should_exist());
        debug_assert!(!receiver.should_be_closed());

        // form deltas
        let mut deltas = write::Deltas::default();
        // there is always a single ack delta, representing a range length beyond 0
        let first_ack = receiver
            .unreliable_received_unacked
            .first_range_entry()
            .filter(|range| range.start() == receiver.unreliable_ack_nacked_lt)
            .map(|range| range.delete_lt(ack_nack_lt))
            .unwrap_or(0);
        deltas.delta(first_ack);
        receiver.unreliable_ack_nacked_lt += first_ack;
        // it is followed by 0 or more deltas, alternating between nacks and acks, representing
        // range lengths beyond 1
        while receiver.unreliable_ack_nacked_lt < ack_nack_lt {
            if let Some(range) = receiver
                .unreliable_received_unacked
                .first_range_entry()
                .filter(|range| range.start() < ack_nack_lt)
            {
                // (ack, nack) pair
                let curr_nack = range.start() - receiver.unreliable_ack_nacked_lt;
                let curr_ack = range.delete_lt(ack_nack_lt);
                deltas.delta(curr_nack - 1);
                deltas.delta(curr_ack - 1);
                receiver.unreliable_ack_nacked_lt += curr_nack + curr_ack;
            } else {
                // lone nack, the last delta
                let curr_nack = ack_nack_lt - receiver.unreliable_ack_nacked_lt;
                deltas.delta(curr_nack - 1);
                receiver.unreliable_ack_nacked_lt += curr_nack;
                debug_assert!(receiver.unreliable_ack_nacked_lt == ack_nack_lt);
            }
        }
        debug_assert!(receiver.unreliable_ack_nacked_lt == ack_nack_lt);

        // potentially immediately create another unreliable ack-nack task
        self.maybe_spawn_unreliable_ack_nack_task(chan_id, receiver);

        // transition from locking the receiver state to locking the ctrl stream
        let ctrl_stream_guard_fut = receiver.ctrl_stream.clone().lock_owned();

        // maybe finish
        self.maybe_close_receiver(chan_id, receiver);

        drop(receiver_guard);
        let mut ctrl_stream_guard = ctrl_stream_guard_fut.await;

        // transmit the acks
        let ctrl_stream =
            lazy_init_ctrl_stream(&mut *ctrl_stream_guard, &self.quic_connection, chan_id).await?;
        let mut w = write::Frames::default();
        w.ack_nack_unreliable(deltas);
        w.send_on_stream(ctrl_stream).await?;

        Ok(())
    }
}

async fn lazy_init_ctrl_stream<'a>(
    opt_ctrl_stream: &'a mut Option<quinn::SendStream>,
    quic_connection: &quinn::Connection,
    chan_id: ChanId,
) -> Result<&'a mut quinn::SendStream, anyhow::Error> {
    if opt_ctrl_stream.is_none() {
        let mut w = write::Frames::default();
        w.route_to(chan_id);
        let ctrl_stream = w.send_on_new_stream(quic_connection).await?;
        *opt_ctrl_stream = Some(ctrl_stream);
    }
    Ok(opt_ctrl_stream.as_mut().unwrap())
}
