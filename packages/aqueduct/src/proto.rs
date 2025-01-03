
use crate::{
    codec::{
        common::*,
        read,
        write,
    },
    zero_copy::MultiBytes,
    *,
};
use std::sync::{
    atomic::{
        AtomicU64,
        AtomicBool,
        Ordering::Relaxed,
    },
    Arc,
    Mutex,
};
use bytes::Bytes;
use dashmap::{
    mapref::entry::Entry,
    DashMap,
};
use anyhow::*;


// shared state for side of an aqueduct connection
struct Conn {
    // the underlying QUIC connection
    quic_conn: quinn::Connection,
    // whether we need to precede outgoing frames with a Version frame
    send_version: AtomicBool,
    // what side of the connection are we
    side: Side,
    // next locally minted chan ids
    next_chan_ids: [[AtomicU64; 2]; 2],
    // mpsc channels to receiver tasks
    receivers: DashMap<ChanId, ReceiverTaskMpsc>,
}

impl Conn {
    // handle an incoming QUIC unidirectional stream or datagram
    async fn handle_frames(self: &Arc<Self>, mut rframes: read::Frames) -> Result<()> {
        while let Some(r) = rframes.frame().await? {
            match r {
                read::Frame::Version(r) => {
                    rframes = r.validate().await?;
                }
                read::Frame::ConnectionControl(_) => {
                    bail!("ConnectionControl frame past handshake");
                }
                read::Frame::ChannelControl(_) => {
                    bail!("ChannelControl frame not on bidi stream");
                }
                read::Frame::Message(r) => {
                    rframes = self.handle_message_frame(r).await?;
                }
                read::Frame::SentUnreliable(_) => {
                    bail!("SentUnreliable frame not on chan ctrl stream");
                }
                read::Frame::AckReliable(_) => {
                    bail!("AckReliable frame not on chan ctrl stream");
                }
                read::Frame::AckNackUnreliable(_) => {
                    bail!("AckNackUnreliable frame not on chan ctrl stream");
                }
                read::Frame::FinishSender(_) => {
                    bail!("FinishSender frame not on chan ctrl stream");
                }
                read::Frame::CloseReceiver(_) => {
                    bail!("CloseReceiver frame not on chan ctrl stream");
                }
                read::Frame::ClosedChannelLost(r) => {
                    let (r, chan_id) = r.chan_id().await?;
                    rframes = r;
                    todo!()
                }
            }
        }
        Ok(())
    }

    // handle an incoming message frame
    async fn handle_message_frame(self: &Arc<Self>, r: read::Message) -> Result<read::Frames> {
        // fully read and decode, validate as we go
        let (r, sent_on) = r.sent_on().await?;
        ensure!(sent_on.dir().side_to() == self.side, "message chan id wrong dir");
        let (r, message_num) = r.message_num().await?;
        let (mut r, _attachments_len) = r.attachments_len().await?;
        let mut attachments = Vec::new();
        while let Some(attachment) = r.next_attachment().await? {
            ensure!(
                attachment.minted_by() != self.side,
                "attachment chan id minted by wrong side",
            );
            attachments.push(Some(if attachment.dir().side_to() == self.side {
                let mpsc_entry = self.receivers
                    .get(&attachment)
                    .unwrap_or_else(|| self.receivers
                        .entry(attachment)
                        .or_default()
                        .downgrade());
                let mut recv_task_msg_lock = mpsc_entry.recv_task_msg.lock().unwrap();
                let recv_task_msg = recv_task_msg_lock.take()
                    .ok_or_else(|| anyhow!("detected receiver attached twice by remote"))?;
                DecodedAttachment::Receiver(attachment, recv_task_msg)
            } else {
                DecodedAttachment::Sender(attachment)
            }));
        }
        let r = r.done();
        let (r, _payload_len) = r.payload_len().await?;
        let (r, payload) = r.payload().await?;
        let msg_frame = DecodedMessageFrame {
            reliable: todo!(),
            sent_on,
            message_num,
            attachments,
            payload,
        };

        // route to receiver, possiblye create receiver
        let task_msg = ReceiverTaskMsg::DecodedMessageFrame(msg_frame);
        if sent_on.minted_by() == self.side {
            if let Some(receiver) = self.receivers.get(&sent_on) {
                receiver.send_task_msg.send(task_msg).unwrap();
            }
        } else {
            // TODO: optimize locking?
            self.receivers.entry(sent_on).or_default().send_task_msg.send(task_msg).unwrap();
        }

        Ok(r)
    }

    // handle an incoming QUIC bidirectional stream
    async fn handle_bidi_stream(
        self: &Arc<Self>,
        mut rframes: read::Frames,
        mut stream_send: quinn::SendStream,
    ) -> Result<()> {
        let r = rframes.frame().await?.unwrap();
        let read::Frame::ChannelControl(r) = r else { bail!("expected ChannelControl frame") };
        let (r, chan_id) = r.chan_id().await?;
        rframes = r;

        ensure!(chan_id.minted_by() == self.side, "ChannelControl frame minted by wrong side");
        if chan_id.dir().side_to() == self.side {
            // receiver
            if let Some(task_mpsc) = self.receivers.get(&chan_id) {
                task_mpsc.send_task_msg
                    .send(ReceiverTaskMsg::ChanCtrl(rframes, stream_send))
                    .unwrap();
            } else {
                stream_send.reset((ResetErrorCode::Lost as u32).into()).unwrap();
            }
        } else {
            // sender
        }

        Ok(())
    }

    // task for a networked receiver
    async fn receiver_task<M, D: DecoderDetacher<M>>(
        self: Arc<Self>,
        chan_id: ChanId,
        mut recv_task_msg: tokio::sync::mpsc::UnboundedReceiver<ReceiverTaskMsg>,
        app_decoder: D,
        gateway: IntoSender<M>,
    ) -> Result<()> {
        debug_assert!(chan_id.dir().side_to() == self.side);
        let gateway = gateway.into_ordered_unbounded();
        let mut chan_ctrl = None;
        if chan_id.minted_by() != self.side {
            let (mut stream_send, stream_recv) = self.quic_conn.open_bi().await?;
            let mut rframes = read::Frames::from_bidi_stream(stream_recv);

            let mut wframes = write::Frames::default();
            debug_assert!(!self.send_version.load(Relaxed));
            wframes.channel_control(chan_id);
            wframes.send_stream(&mut stream_send).await?;

            chan_ctrl = Some((rframes, stream_send));
        }
        while let Some(task_msg) = recv_task_msg.recv().await {
            match task_msg {
                ReceiverTaskMsg::DecodedMessageFrame(mut msg_frame) => {
                    let detach_target = DetachTarget {
                        conn: &self,
                        attachments: &mut msg_frame.attachments,
                    };
                    // TODO: catch both this and panic
                    let app_msg = app_decoder.decode(msg_frame.payload, detach_target).unwrap();
                    // TODO: catch this
                    gateway.send(app_msg).ok().unwrap();
                }
                ReceiverTaskMsg::ChanCtrl(rframes, mut stream_send) => {
                    if chan_ctrl.is_some() {
                        stream_send.reset((ResetErrorCode::Lost as u32).into()).unwrap();
                    } else {
                        chan_ctrl = Some((rframes, stream_send));
                    }
                }
            }
        }
        todo!()
    }

    // task for a networked sender
    async fn sender_task<M, E: EncoderAttacher<M>>(
        self: Arc<Self>,
        chan_id: ChanId,
        app_encoder: E,
        gateway: IntoReceiver<M>,
    ) -> Result<()> {
        debug_assert!(chan_id.dir().side_to() != self.side);
        let gateway = gateway.into_receiver();
        let mut delivery_guarantees = DeliveryGuarantees::Unconverted;
        let mut single_stream = None;
        for message_num in 0.. {
            // TODO: catch both unwraps
            let app_msg = gateway.recv().await.unwrap().unwrap();
            
            if delivery_guarantees == DeliveryGuarantees::Unconverted {
                delivery_guarantees = gateway.delivery_guarantees();
            }
            debug_assert!(delivery_guarantees != DeliveryGuarantees::Unconverted);

            let mut attachments = write::Attachments::default();
            let attach_target = AttachTarget {
                conn: &self,
                attachments: &mut attachments,
                next_attachment_idx: 0,
            };
            // TODO: catch both this and panic
            let payload = app_encoder.encode(app_msg, attach_target).unwrap();

            // encode frames
            let mut frames = write::Frames::default();
            if self.send_version.load(Relaxed) {
                frames.version();
            }
            frames.message(chan_id, message_num, attachments, payload);

            // send frames
            match delivery_guarantees {
                DeliveryGuarantees::Ordered => {
                    if single_stream.is_none() {
                        // TODO catch
                        single_stream = Some(self.quic_conn.open_uni().await.unwrap());
                    }
                    // TODO catch
                    frames.send_stream(single_stream.as_mut().unwrap()).await.unwrap();
                }
                DeliveryGuarantees::Unordered => {
                    // TODO: catch
                    frames.send_new_stream(&self.quic_conn).await.unwrap();
                }
                DeliveryGuarantees::Unreliable => {
                    // TODO: catch
                    frames.send_datagram(&self.quic_conn).await.unwrap();
                }
                DeliveryGuarantees::Unconverted => unreachable!(),
            }
        }
        Ok(())
    }

    // locally mint a new channel id
    fn mint_chan_id(&self, dir: Dir, oneshot: bool) -> ChanId {
        let idx = self.next_chan_ids[dir as u8 as usize][oneshot as usize].fetch_add(1, Relaxed);
        assert!(idx != u64::MAX, "chan id mint overflow. wow, you overflowed a 64-bit counter!");
        ChanId::new(dir, self.side, oneshot, idx)
    }

    // client-side Aqueduct handshake
    async fn client_handshake(self: Arc<Self>) -> Result<()> {
        let (mut stream_send, stream_recv) = self.quic_conn.open_bi().await?;
        let mut rframes = read::Frames::from_bidi_stream(stream_recv);

        let mut wframes = write::Frames::default();
        wframes.version();
        wframes.connection_control();
        wframes.send_stream(&mut stream_send).await;
        stream_send.finish().unwrap();

        let r = rframes.frame().await?.unwrap();
        let read::Frame::Version(r) = r else { bail!("expected Version frame") };
        rframes = r.validate().await?;
        self.send_version.store(false, Relaxed);

        let r = rframes.frame().await?.unwrap();
        let read::Frame::ConnectionControl(r) = r else { bail!("expected ConnectionControl frame") };
        rframes = r.skip_headers().await?;

        ensure!(rframes.frame().await?.is_none(), "expected end of stream");

        Ok(())
    }
}

// mpsc sender to receiver task
struct ReceiverTaskMpsc {
    send_task_msg: tokio::sync::mpsc::UnboundedSender<ReceiverTaskMsg>,
    recv_task_msg: Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<ReceiverTaskMsg>>>,
}

impl Default for ReceiverTaskMpsc {
    fn default() -> Self {
        let (send_task_msg, recv_task_msg) = tokio::sync::mpsc::unbounded_channel();
        ReceiverTaskMpsc {
            send_task_msg,
            recv_task_msg: Mutex::new(Some(recv_task_msg)),
        }
    }
}

// message sent through mpsc channel to receiver task
enum ReceiverTaskMsg {
    // a message frame was received and decoded and routed to the receiver
    DecodedMessageFrame(DecodedMessageFrame),
    // a stream was identified as the channel control stream
    ChanCtrl(read::Frames, quinn::SendStream),
}

// fully decoded message frame
struct DecodedMessageFrame {
    // whether this was received on a stream as opposed to a datagram
    reliable: bool,
    // actual fields
    sent_on: ChanId,
    message_num: u64,
    // some additional processing of attachments may be done in the decoding stage
    attachments: Vec<Option<DecodedAttachment>>,
    payload: MultiBytes,
}

// stored in DecodedMessageFrame
enum DecodedAttachment {
    Sender(ChanId),
    // when an attached received is decoded, its receiver task mpsc is preemtively created / the
    // recv_task_msg taken. this frontruns double-attachment errors before application decoder is
    // invoked.
    Receiver(ChanId, tokio::sync::mpsc::UnboundedReceiver<ReceiverTaskMsg>),
}

/// Logic for encoding a message and attaching its attachments in some format.
pub trait EncoderAttacher<M>: Send + Sync + 'static {
    fn encode(&self, msg: M, attach: AttachTarget) -> Result<MultiBytes>;
}

/// Logic for decoding a message and detaching its attachments in some format.
pub trait DecoderDetacher<M>: Send + Sync + 'static {
    fn decode(&self, encoded: MultiBytes, detach: DetachTarget) -> Result<M>;
}

/// Passed to an [`EncoderAttacher`] to attach attachments to.
pub struct AttachTarget<'a> {
    conn: &'a Arc<Conn>,
    attachments: &'a mut write::Attachments,
    next_attachment_idx: usize,
}

impl<'a> AttachTarget<'a> {
    fn attach_inner(&mut self, dir: Dir, oneshot: bool) -> (usize, ChanId) {
        let attachment_idx = self.next_attachment_idx;
        self.next_attachment_idx += 1;
        let chan_id = self.conn.mint_chan_id(dir, oneshot);
        self.attachments.attachment(chan_id);
        (attachment_idx, chan_id)
    }

    pub fn attach_sender<M, D>(&mut self, sender: IntoSender<M>, decoder: D) -> usize
    where
        M: Send + 'static,
        D: DecoderDetacher<M>,
    {
        let (
            attachment_idx,
            chan_id,
        ) = self.attach_inner(self.conn.side.opposite().dir_to(), false);
        let (send_task_msg, recv_task_msg) = tokio::sync::mpsc::unbounded_channel();
        let task_mpsc = ReceiverTaskMpsc {
            send_task_msg,
            recv_task_msg: Mutex::new(None),
        };
        let prev_val = self.conn.receivers.insert(chan_id, task_mpsc);
        // the way in which we locally mint it should make collision impossible
        debug_assert!(prev_val.is_none());
        tokio::spawn(
            Arc::clone(&self.conn).receiver_task(chan_id, recv_task_msg, decoder, sender)
        );
        attachment_idx
    }

    pub fn attach_receiver<M, E>(&mut self, receiver: IntoReceiver<M>, encoder: E) -> usize
    where
        M: Send + 'static,
        E: EncoderAttacher<M>,
    {
        let (
            attachment_idx,
            chan_id,
        ) = self.attach_inner(self.conn.side.opposite().dir_to(), false);
        tokio::spawn(Arc::clone(&self.conn).sender_task(chan_id, encoder, receiver));
        attachment_idx
    }
    /*
        pub fn attach_oneshot_sender<M, D>(&mut self, sender: OneshotSender<M>, decoder: D) -> u32
        where
            D: DecoderDetacher<M>,
        {
            todo!()
        }

        pub fn attach_oneshot_receiver<M, E>(&mut self, receiver: OneshotReceiver<M>, encoder: E) -> u32
        where
            E: EncoderAttacher<M>,
        {
            todo!()
        }
    */
}

/// Passed to an [`DecoderDettacher`] to detach attachments from.
pub struct DetachTarget<'a> {
    conn: &'a Arc<Conn>,
    attachments: &'a mut [Option<DecodedAttachment>],
}

impl<'a> DetachTarget<'a> {
    pub fn detach_sender<M, E: EncoderAttacher<M>>(
        &mut self,
        attachment_idx: usize,
        encoder: E,
    ) -> Result<IntoSender<M>, DetachError> {
        todo!()
    }

    pub fn detach_receiver<M: Send + 'static, D: DecoderDetacher<M>>(
        &mut self,
        attachment_idx: usize,
        decoder: D,
    ) -> Result<IntoReceiver<M>, DetachError> {
        let slot = self.attachments.get_mut(attachment_idx)
            .ok_or(DetachError::IndexOutOfBounds)?;
        if slot.as_ref().is_some_and(|attachment|
            !matches!(attachment, DecodedAttachment::Receiver { .. })
        ) {
            return Err(DetachError::WrongAttachmentType);
        }
        let attachment = slot.take().ok_or(DetachError::AlreadyDetached)?;
        let DecodedAttachment::Receiver(
            chan_id,
            recv_task_msg,
        ) = attachment else { unreachable!() };
        let (gateway, receiver) = channel();
        tokio::spawn(
            Arc::clone(&self.conn).receiver_task(chan_id, recv_task_msg, decoder, gateway)
        );
        // TODO: ugly
        std::result::Result::Ok(receiver)
    }
    /*
        pub fn detach_oneshot_sender<M, E: EncoderAttacher<M>>(
            &mut self,
            attachment_idx: u32,
            encoder: E,
        ) -> Result<OneshotSender<M>, MissingAttachment> {
            todo!()
        }

        pub fn detach_oneshot_receiver<M, D: DecoderDetacher<M>>(
            &mut self,
            attachment_idx: u32,
            decoder: D,
        ) -> Result<OneshotReceiver<M>, MissingAttachment> {
            todo!()
        }
        
        pub fn remaining_attachments(&self) -> usize {
            self.remaining_attachments
        }
    */
}

pub enum DetachError {
    IndexOutOfBounds,
    WrongAttachmentType,
    AlreadyDetached,
}

// server-side Aqueduct connection handshake
async fn server_handshake(conn: &quinn::Connection) -> Result<()> {
    let (mut stream_send, stream_recv) = conn.accept_bi().await?;
    let mut rframes = read::Frames::from_bidi_stream(stream_recv);

    let r = rframes.frame().await?.unwrap();
    let read::Frame::Version(r) = r else { bail!("expected Version frame") };
    rframes = r.validate().await?;

    let r = rframes.frame().await?.unwrap();
    let read::Frame::ConnectionControl(r) = r else { bail!("expected ConnectionControl frame") };
    rframes = r.skip_headers().await?;

    let mut wframes = write::Frames::default();
    wframes.version();
    wframes.connection_control();
    wframes.send_stream(&mut stream_send).await?;
    stream_send.finish().unwrap();

    ensure!(rframes.frame().await?.is_none(), "expected end of stream");

    Ok(())
}
