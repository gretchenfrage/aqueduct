
mod ack;
mod frame_category;


use self::{
    frame_category::{
        NonBidiFrame,
        CtrlFrameToReceiver,
    },
    ack::{
        AckTimer,
        ReceiverReliableAck,
        ReceiverUnreliableAck,
    },
};
use crate::{
    codec::{
        common::*,
        read::{self, ReadError},
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
use tokio::{pin, select};
use dashmap::DashMap;
use anyhow::{Error, anyhow, ensure, bail};


// shared state for side of an aqueduct connection
struct Conn {
    // what side of the connection are we
    side: Side,
    // the underlying QUIC connection
    quic_conn: quinn::Connection,
    // whether we need to precede outgoing frames with a Version frame
    send_version: AtomicBool,
    // next locally minted chan ids
    next_chan_ids: [[AtomicU64; 2]; 2],
    // mpsc channels to receiver tasks
    receivers: DashMap<ChanId, ReceiverTaskMpsc>,
}

impl Conn {
    // shut down the connection due to an unrecoverable error.
    async fn shutdown(&self, error: Option<Error>) {
        if let Some(error) = error {
            error!("shutting down connection: {}", error);
        } else {
            trace!("shutting down connection (no error)");
        }
        self.quic_conn.close(0.into(), b"");
    }

    // handle an incoming QUIC unidirectional stream or datagram.
    async fn handle_uni_frames(self: &Arc<Self>, rframes: read::Frames) {
        self.handle_frames_inner(rframes).await.or_else(|e| match e {
            // unidirectional streams ignored when reset
            ReadError::Reset(_) => Ok(()),
            ReadError::Other(e) => self.shutdown(Some(e)),
        })
    }

    // handle an incoming QUIC unidirectional stream or datagram, or error.
    async fn try_handle_uni_frames(
        self: &Arc<Self>,
        mut rframes: read::Frames,
    ) -> Result<(), ReadError> {
        while let Some(r) = rframes.frame().await? {
            let r = NonBidiFrame::try_from(r)?;
            rframes = match r {
                NonBidiFrame::Version(r) => r.validate().await?,
                NonBidiFrame::Message(r) => self.handle_message_frame(r).await?,
                NonBidiFrame::ClosedChannelLost(r) => {
                    let (r, chan_id) = r.chan_id().await?;
                    rframes = r;
                    todo!()
                }
            };
        }
        Ok(())
    }

    // handle an incoming message frame
    async fn handle_message_frame(
        self: &Arc<Self>,
        r: read::Message,
    ) -> Result<read::Frames, ReadError> {
        // fully read and decode, validate as we go
        let reliable = r.reliable();
        let (r, sent_on) = r.sent_on().await?;
        read::ensure!(sent_on.dir().side_to() == self.side, "message chan id wrong dir");
        let (r, message_num) = r.message_num().await?;
        let (mut r, _attachments_len) = r.attachments_len().await?;

        // decode attachments, and create sender / receiver tasks
        let mut attachments = Vec::new();
        while let Some(attachment) = r.next_attachment().await? {
            read::ensure!(
                attachment.minted_by() != self.side,
                "attachment chan id minted by wrong side",
            );
            attachments.push(Some(if attachment.dir().side_to() == self.side {
                // receiver was attached
                let mpsc_entry = self.receivers
                    .get(&attachment)
                    .unwrap_or_else(|| self.receivers
                        .entry(attachment)
                        .or_default()
                        .downgrade());
                let recv_task_msg = mpsc_entry
                    .recv_task_msg.lock().unwrap()
                    .take()
                    .ok_or_else(|| anyhow!("detected receiver attached twice by remote"))?;
                DecodedAttachment::Receiver {
                    chan_id: attachment,
                    send_task_msg: mpsc_entry.send_task_msg.clone(),
                    recv_task_msg,
                }
            } else {
                // sender was attached
                DecodedAttachment::Sender(attachment)
            }));
        }

        let r = r.done();
        let (r, _payload_len) = r.payload_len().await?;
        let (r, payload) = r.payload().await?;
        let msg_frame = DecodedMessageFrame {
            reliable,
            sent_on,
            message_num,
            attachments,
            payload,
        };

        // route to receiver, possibly create receiver
        let task_msg = ReceiverTaskMsg::DecodedMessageFrame(msg_frame);
        if let Some(receiver) = self.receivers.get(&sent_on) {
            // try to route to existing receiver with only a read-lock
            receiver.send_task_msg.send(task_msg).unwrap();
        } else if sent_on.minted_by() != self.side {

        }

        if sent_on.minted_by() == self.side {
                receiver.send_task_msg.send(task_msg).unwrap();
            }
            // ignore messages for which we already closed the receiver
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
    ) -> Result<(), Error> {
        let r = rframes.first_frame().await.map_err(ReadError::cannot_reset)?;
        let read::Frame::ChannelControl(r) = r else { bail!("expected ChannelControl frame") };
        let (r, chan_id) = r.chan_id().await.map_err(ReadError::cannot_reset)?;
        rframes = r;

        ensure!(chan_id.minted_by() == self.side, "ChannelControl frame minted by wrong side");
        if chan_id.dir().side_to() == self.side {
            // receiver
            if let Some(task_mpsc) = self.receivers.get(&chan_id) {
                task_mpsc.send_task_msg
                    .send(ReceiverTaskMsg::ChanCtrl(stream_send))
                    .unwrap();
            } else {
                stream_send.reset((ResetCode::Lost as u32).into()).unwrap();
            }
        } else {
            // sender
        }

        Ok(())
    }

    // read frames from a channel control stream and forward them to the receiver task.
    async fn relay_ctrl_to_receiver(
        self: Arc<Self>,
        mut rframes: read::Frames,
        send_task_msg: tokio::sync::mpsc::UnboundedSender<ReceiverTaskMsg>,
    ) -> Result<(), Error> {
        match self.relay_ctrl_to_receiver_inner(rframes, &send_task_msg).await {
            Ok(()) => (),
            // if the inner task fails due to stream reset, catch and handle gracefully
            Err(ReadError::Reset(code)) => {
                let _ = send_task_msg.send(ReceiverTaskMsg::ChanCtrlReset(code));
            }
            Err(ReadError::Other(e)) => return Err(e),
        }
        Ok(())
    }

    async fn relay_ctrl_to_receiver_inner(
        self: &Arc<Self>,
        mut rframes: read::Frames,
        send_task_msg: &tokio::sync::mpsc::UnboundedSender<ReceiverTaskMsg>,
    ) -> Result<(), ReadError> {
        while let Some(r) = rframes.frame().await? {
            let r = CtrlFrameToReceiver::try_from(r)?;
            match r {
                CtrlFrameToReceiver::SentUnreliable(r) => {
                    let (r, delta) = r.delta().await?;
                    rframes = r;
                    let _ = send_task_msg.send(ReceiverTaskMsg::SentUnreliable(delta));
                }
                CtrlFrameToReceiver::FinishSender(r) => {
                    let (r, reliable_count) = r.reliable_count().await?;
                    rframes = r;
                    let _ = send_task_msg.send(ReceiverTaskMsg::FinishSender(reliable_count));
                }
            }
        }
        Ok(())
    }

    // task for a networked receiver
    async fn receiver_task<M, D: DecoderDetacher<M>>(
        self: Arc<Self>,
        chan_id: ChanId,
        send_task_msg: tokio::sync::mpsc::UnboundedSender<ReceiverTaskMsg>,
        mut recv_task_msg: tokio::sync::mpsc::UnboundedReceiver<ReceiverTaskMsg>,
        app_decoder: D,
        gateway: IntoSender<M>,
    ) -> Result<(), Error> {
        debug_assert!(chan_id.dir().side_to() == self.side);
        let gateway = gateway.into_ordered_unbounded();

        // create the channel control stream, if it's our side's responsibility
        let mut chan_ctrl = None;
        if chan_id.minted_by() != self.side {
            // open stream
            let (mut stream_send, stream_recv) = self.quic_conn.open_bi().await?;
            let rframes = read::Frames::from_bidi_stream(stream_recv);

            // write the frames that triggers the remote side to initialize it
            let mut wframes = write::Frames::default();
            debug_assert!(!self.send_version.load(Relaxed));
            wframes.channel_control(chan_id);
            wframes.send_stream(&mut stream_send).await?;

            // locally install it
            chan_ctrl = Some(stream_send);
            tokio::spawn(Arc::clone(&self).relay_ctrl_to_receiver(rframes, send_task_msg.clone()));
        }

        // acking state
        let reliable_ack_timer = AckTimer::<()>::new();
        pin!(reliable_ack_timer);

        // ==== unreliable ack-nacking state ====
        let mut reliable_ack = ReceiverReliableAck::default();
        let reliable_ack_timer = AckTimer::new();
        pin!(reliable_ack_timer);

        let mut unreliable_ack = ReceiverUnreliableAck::default();
        let unreliable_ack_timer = AckTimer::new();
        pin!(unreliable_ack_timer);

        // enter the receiver task loop
        loop {
            // get task msg, or process ack timer then continue
            let task_msg = select! {
                () = &mut reliable_ack_timer => {
                    reliable_ack.on_timer_zero(chan_ctrl.as_mut().unwrap()).await?;
                    continue
                }
                timer_val = &mut unreliable_ack_timer => {
                    unreliable_ack
                        .on_timer_zero(
                            timer_val,
                            &mut unreliable_ack_timer,
                            chan_ctrl.as_mut().unwrap(),
                        ).await?;
                    continue
                }
                opt_task_msg = recv_task_msg.recv() => {
                    opt_task_msg.unwrap() // TODO?
                }
            };

            // process ack message
            match task_msg {
                ReceiverTaskMsg::DecodedMessageFrame(mut msg_frame) => {
                    // received message on channel

                    // update ack-nacking state
                    if msg_frame.reliable {
                        reliable_ack.on_message(msg_frame.message_num, &mut reliable_ack_timer);
                        reliable_ack_timer.start(());
                    } else {
                        if unreliable_ack.on_message(msg_frame.message_num).is_err() {
                            continue;
                        }
                    }

                    // decode and detach and give to the application
                    let detach_target = DetachTarget {
                        conn: &self,
                        attachments: &mut msg_frame.attachments,
                    };
                    // TODO: catch both this and panic
                    let app_msg = app_decoder.decode(msg_frame.payload, detach_target).unwrap();
                    // TODO: catch this
                    gateway.send(app_msg).ok().unwrap();
                }
                ReceiverTaskMsg::ChanCtrl(stream_send) => {
                    debug_assert!(chan_ctrl.is_none());
                    chan_ctrl = Some(stream_send);
                }
                ReceiverTaskMsg::SentUnreliable(delta) => {
                    unreliable_ack.on_sent_unreliable(delta, &mut unreliable_ack_timer)?;
                }
                ReceiverTaskMsg::FinishSender(reliable_count) => {

                }
                ReceiverTaskMsg::ChanCtrlReset(code) => {

                }
            }
        }
    }

    // task for a networked sender
    async fn sender_task<M, E: EncoderAttacher<M>>(
        self: Arc<Self>,
        chan_id: ChanId,
        app_encoder: E,
        gateway: IntoReceiver<M>,
    ) -> Result<(), Error> {
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
    async fn client_handshake(self: Arc<Self>) -> Result<(), Error> {
        let (mut stream_send, stream_recv) = self.quic_conn.open_bi().await?;
        let mut rframes = read::Frames::from_bidi_stream(stream_recv);

        let mut wframes = write::Frames::default();
        wframes.version();
        wframes.connection_control();
        wframes.send_stream(&mut stream_send).await;
        stream_send.finish().unwrap();

        let r = rframes.first_frame().await.map_err(ReadError::cannot_reset)?;
        let read::Frame::Version(r) = r else { bail!("expected Version frame") };
        rframes = r.validate().await.map_err(ReadError::cannot_reset)?;
        self.send_version.store(false, Relaxed);

        let r = rframes.first_frame().await.map_err(ReadError::cannot_reset)?;
        let read::Frame::ConnectionControl(r) = r else { bail!("expected ConnectionControl frame") };
        rframes = r.skip_headers().await.map_err(ReadError::cannot_reset)?;

        ensure!(
            rframes.frame().await.map_err(ReadError::cannot_reset)?.is_none(),
            "expected end of stream",
        );

        Ok(())
    }
}

// mpsc sender to receiver task
struct ReceiverTaskMpsc {
    send_task_msg: tokio::sync::mpsc::UnboundedSender<ReceiverTaskMsg>,
    recv_task_msg: Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<ReceiverTaskMsg>>>,
    chan_ctrl_frame_found: AtomicBool,
}

impl Default for ReceiverTaskMpsc {
    fn default() -> Self {
        let (send_task_msg, recv_task_msg) = tokio::sync::mpsc::unbounded_channel();
        ReceiverTaskMpsc {
            send_task_msg,
            recv_task_msg: Mutex::new(Some(recv_task_msg)),
            chan_ctrl_frame_found: false.into(),
        }
    }
}

// message sent through mpsc channel to receiver task
enum ReceiverTaskMsg {
    // a message frame was received and decoded and routed to the receiver
    DecodedMessageFrame(DecodedMessageFrame),
    // the channel control stream was found
    ChanCtrl(quinn::SendStream),
    // a SentUnreliable frame was received from the channel control stream
    SentUnreliable(u64),
    // the FinishSender frame was received from the channel control stream
    FinishSender(u64),
    // the channel control stream was reset
    ChanCtrlReset(ResetCode),
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
    // when an attached received is decoded, its receiver task mpsc is preemptively created / the
    // recv_task_msg taken. this frontruns double-attachment errors before application decoder is
    // invoked.
    Receiver {
        chan_id: ChanId,
        send_task_msg: tokio::sync::mpsc::UnboundedSender<ReceiverTaskMsg>,
        recv_task_msg: tokio::sync::mpsc::UnboundedReceiver<ReceiverTaskMsg>,
    },
}

/// Logic for encoding a message and attaching its attachments in some format.
pub trait EncoderAttacher<M>: Send + Sync + 'static {
    fn encode(&self, msg: M, attach: AttachTarget) -> Result<MultiBytes, Error>;
}

/// Logic for decoding a message and detaching its attachments in some format.
pub trait DecoderDetacher<M>: Send + Sync + 'static {
    fn decode(&self, encoded: MultiBytes, detach: DetachTarget) -> Result<M, Error>;
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
            send_task_msg: send_task_msg.clone(),
            recv_task_msg: Mutex::new(None),
            chan_ctrl_frame_found: false.into(),
        };
        let prev_val = self.conn.receivers.insert(chan_id, task_mpsc);
        // the way in which we locally mint it should make collision impossible
        debug_assert!(prev_val.is_none());
        tokio::spawn(
            Arc::clone(&self.conn)
                .receiver_task(chan_id, send_task_msg, recv_task_msg, decoder, sender)
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
        if slot.as_ref()
            .is_some_and(|attachment| !matches!(attachment, DecodedAttachment::Receiver { .. }))
        {
            return Err(DetachError::WrongAttachmentType);
        }
        let attachment = slot.take().ok_or(DetachError::AlreadyDetached)?;
        let DecodedAttachment::Receiver { chan_id, send_task_msg, recv_task_msg } =
            attachment
            else { unreachable!() };
        let (gateway, receiver) = channel();
        tokio::spawn(
            Arc::clone(&self.conn).receiver_task(chan_id, send_task_msg, recv_task_msg, decoder, gateway)
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
async fn server_handshake(conn: &quinn::Connection) -> Result<(), Error> {
    let (mut stream_send, stream_recv) = conn.accept_bi().await?;
    let mut rframes = read::Frames::from_bidi_stream(stream_recv);

    let r = rframes.first_frame().await.map_err(ReadError::cannot_reset)?;
    let read::Frame::Version(r) = r else { bail!("expected Version frame") };
    rframes = r.validate().await.map_err(ReadError::cannot_reset)?;

    let r = rframes.first_frame().await.map_err(ReadError::cannot_reset)?;
    let read::Frame::ConnectionControl(r) = r else { bail!("expected ConnectionControl frame") };
    rframes = r.skip_headers().await.map_err(ReadError::cannot_reset)?;

    let mut wframes = write::Frames::default();
    wframes.version();
    wframes.connection_control();
    wframes.send_stream(&mut stream_send).await?;
    stream_send.finish().unwrap();

    ensure!(
        rframes.frame().await.map_err(ReadError::cannot_reset)?.is_none(),
        "expected end of stream",
    );

    Ok(())
}
