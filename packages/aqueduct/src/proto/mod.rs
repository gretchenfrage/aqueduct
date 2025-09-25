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
    received_creating_message: bool,
    received_messages: Vec<ReceivedMessage>,
    reliable_received_acked: RangeSetU64,
    reliable_received_unacked: RangeSetU64,
    reliable_ack_task_exists: bool,
    ctrl_stream: Option<quinn::SendStream>,
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
                read::Frame::Message(r) => todo!(),
                read::Frame::SentUnreliable(r) => todo!(),
                read::Frame::FinishSender(r) => todo!(),
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
                read::Frame::RouteTo(r) => todo!(),
                read::Frame::Message(r) => {
                    // read message
                    let read::Message {
                        message_num,
                        message_headers: mut r,
                    } = r;
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
                                attached_channel_headers.push((
                                    attached_channel_header_key,
                                    attached_channel_header_val,
                                ));
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

                    // process message
                    read::ensure!(
                        chan_id.sender() != self.side,
                        "received MESSAGE frame for channel ID for which local side is the sender"
                    );
                    for attached_channel in &received_message.attached_channels {
                        read::ensure!(
                            attached_channel.chan_id.creator() != self.side,
                            "received MESSAGE frame with attached channel ID for which local side
                            is the creator"
                        );
                        if attached_channel.chan_id.sender() == self.side {
                            let mut sender = self.sender_state(attached_channel.chan_id)?;
                            read::ensure!(
                                !sender.received_creating_message,
                                "received channel ID attached to multiple messages"
                            );
                            sender.received_creating_message = true;
                        } else {
                            let mut receiver = self.receiver_state(attached_channel.chan_id)?;
                            read::ensure!(
                                !receiver.received_creating_message,
                                "received channel ID attached to multiple messages"
                            );
                            receiver.received_creating_message = true;
                        }
                    }
                    let mut receiver = self.receiver_state(chan_id)?;
                    receiver.received_messages.push(received_message);
                    if reliable {
                        read::ensure!(
                            !receiver
                                .reliable_received_unacked
                                .insert(message_num, message_num),
                            "received reliable message num in duplicate"
                        );
                        if !receiver.reliable_ack_task_exists {
                            receiver.reliable_ack_task_exists = true;
                            let this = self.clone();
                            self.spawn(async move {
                                tokio::time::sleep(Duration::from_secs(1)).await;
                                this.reliable_ack(chan_id).await;
                                Ok(())
                            });
                        }
                    } else {
                    }
                    drop(receiver);

                    r
                }
                read::Frame::SentUnreliable(r) => todo!(),
                read::Frame::FinishSender(r) => todo!(),
                read::Frame::CancelSender(r) => todo!(),
                read::Frame::AckReliable(r) => todo!(),
                read::Frame::AckNackUnreliable(r) => todo!(),
                read::Frame::CloseReceiver(r) => todo!(),
                read::Frame::ForgetChannel(r) => todo!(),
            };
        }
        Ok(())
    }

    // get the SenderState for the channel ID if it currently exists. if it used to exist but
    // doesn't any more, return a Reset error. if it never existed, create it with defaults the
    // channel ID creator side is the remote side, or fatally error if it is the local side.
    fn sender_state(
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
    // doesn't any more, return a Reset error. if it never existed, create it with defaults the
    // channel ID creator side is the remote side, or fatally error if it is the local side.
    fn receiver_state(
        &self,
        chan_id: ChanId,
    ) -> read::Result<dashmap::mapref::one::RefMut<'_, ChanId, ReceiverState>> {
        debug_assert!(chan_id.sender() != self.side);
        self.receivers.entry(chan_id).or_try_insert_with(|| {
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
            Ok(ReceiverState::default())
        })
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

    async fn reliable_ack(&self, chan_id: ChanId) -> read::Result<()> {
        let Some(mut receiver_guard) = self.receivers.get_mut(&chan_id) else {
            trace!("reliable ack timer elapsed for non-existent receiver, ignoring");
            return Ok(());
        };
        let receiver = &mut *receiver_guard;

        debug_assert!(receiver.reliable_ack_task_exists);
        receiver.reliable_ack_task_exists = false;
        debug_assert!(!receiver.reliable_received_unacked.is_empty());

        let first_unacked = receiver
            .reliable_received_acked
            .iter()
            .next()
            .filter(|&(s2, _)| s2 == 0)
            .map(|(_, e2)| e2 + 1)
            .unwrap_or(0);

        for (s, e) in receiver.reliable_received_unacked.iter() {
            read::ensure!(
                !receiver.reliable_received_acked.insert(s, e),
                "received reliable message num in duplicate",
            );
        }

        let mut deltas = write::Deltas::default();
        let mut acking = receiver.reliable_received_unacked.iter().peekable();
        let first_unacked = acking
            .next_if(|&(s1, _)| s1 == first_unacked)
            .map(|(s, e)| {
                let new_first_unacked = e + 1;
                deltas.delta(new_first_unacked - s);
                new_first_unacked
            })
            .unwrap_or_else(|| {
                deltas.delta(0);
                first_unacked
            });
        let mut base = Some(first_unacked);
        for (s, e) in acking {
            deltas.delta(s - base.unwrap());
            deltas.delta(e - s);
            base = e.checked_add(1);
        }

        receiver.reliable_received_unacked.clear();

        drop(receiver_guard);

        let mut w = write::Frames::new_with_version(todo!());

        Ok(())
    }
}
