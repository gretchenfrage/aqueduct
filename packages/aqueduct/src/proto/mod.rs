
mod chan_id_mint;
mod ack;

use self::{
    chan_id_mint::ChanIdMint,
    ack::ReceiverUnreliableAckManager,
};
use crate::{
    zero_copy::MultiBytes,
    channel::{
        api::*,
        error::*,
    },
    frame::{
        common::*,
        read::{self, ResetResult},
        write,
    },
};
use std::{
    sync::{
        atomic::{
            AtomicBool,
            Ordering::Relaxed,
        },
        Arc,
    },
    panic::{AssertUnwindSafe, catch_unwind},
    num::NonZero,
};
use dashmap::DashMap;
use tokio::sync::mpsc::{
    UnboundedSender as TokioUnboundedSender,
    UnboundedReceiver as TokioUnboundedReceiver,
    unbounded_channel as tokio_unbounded_channel,
};
use anyhow::{Result, Context, anyhow};


/// Top-level shared state of an Aqueduct connection.
struct Connection {
    /// The underlying QUIC connection.
    quic_conn: quinn::Connection,
    /// Whether we're the client or the server.
    side: Side,
    /// For minting new channel IDs.
    chan_id_mint: ChanIdMint,
    /// Whether we must (still) begin all outgoing QUIC streams/datagrams with a Version frame.
    must_send_version: Arc<AtomicBool>,
    /// Whether we have (as of yet) received an entire valid Version frame from the remote side.
    has_received_version: Arc<AtomicBool>,
    /// Receiver state for networked Aqueduct channels.
    receivers: DashMap<ChanIdLocalReceiver, NetReceiver>,
}

/// Receiver state for a networked Aqueduct channel.
struct NetReceiver {
    /// Message sender to the local side's channel control task.
    send_ctrl_task_msg: TokioUnboundedSender<ReceiverCtrlTaskMsg>,
    /// Whether a ReceiverTaken message may be sent to the channel control task.
    receiver_taken: AtomicBool,
    /// Whether a CtrlStreamOpened message may be sent to the channel control task.
    ctrl_stream_opened: AtomicBool,
}

/// Message to a receiver-side channel control task.
enum ReceiverCtrlTaskMsg {
    /// The local application took the receiver handle. That means that proto code constructed a
    /// gateway Aqueduct channel, gave ownership of the receiver half to the application, and
    /// wrapped the sender half plus application-provided serialization logic into the contained
    /// gateway object.
    ReceiverTaken(Box<dyn FnMut(ReceivedMessage) -> Result<()> + Send>),
    /// A Message frame was received and routed to this receiver.
    ReceivedMessage(ReceivedMessage),
    /// The remote side opened the channel control stream for this receiver.
    CtrlStreamOpened(quinn::SendStream),
    /// A SentUnreliable frame was received on the channel control stream.
    ReceivedSentUnreliable {
        delta: NonZero<u64>,
    },
    /// A FinishSender frame was received on the channel control stream.
    ReceivedFinishSender {
        reliable_count: u64,
    },
    /// The remote side reset the channel control stream.
    CtrlStreamReset(ResetCode),
    /// Used internally by control task.
    UnreliableAckTimer,
    /// Used internally by control task.
    ReliableAckTimer,
}

/// A fully received Message frame.
struct ReceivedMessage {
    message_num: MessageNum,
    payload: MultiBytes,
    /// These are all initially `Some`; they are wrapped in `Option` to simplify `DecoderDetacher`
    /// implementation.
    attachments: Vec<Option<ChanIdRemotelyMinted>>,
}

impl Connection {
    /// Handle a sequence of incoming Message frames.
    async fn handle_messages(&self, r: read::Message) -> ResetResult<()> {
        // route
        let (mut r, sent_on) = r.sent_on().await?;
        let receiver = match sent_on.sort_by_minted_by(self.side) {
            SortByMintedBy::LocallyMinted(_) => {
                if let Some(receiver) = self.receivers.get(&sent_on) {
                    receiver
                } else {
                    return Ok(());
                }
            }
            SortByMintedBy::RemotelyMinted(_) => {
                self.receivers
                    .entry(sent_on)
                    .or_insert_with(|| {
                        let (send_ctrl_task_msg, recv_ctrl_task_msg) = tokio_unbounded_channel();
                        let _ = recv_ctrl_task_msg; // TODO: spawn channel control task
                        NetReceiver {
                            send_ctrl_task_msg,
                            receiver_taken: false.into(),
                            ctrl_stream_opened: true.into(),
                        }
                    })
                    .downgrade()
            }
        };

        // process Message frames
        while let Some(r2) = self.handle_routed_message(&receiver, r).await? {
            r = r2;
        }

        Ok(())
    }

    /// Handle an incoming Message frame that has already been routed to a receiver.
    async fn handle_routed_message(
        &self,
        receiver: &NetReceiver,
        r: read::Message2,
    ) -> ResetResult<Option<read::Message2>> {
        // read the rest of the message
        let (r, message_num) = r.message_num().await?;
        let (mut r, _) = r.attachments_len().await?;
        let mut attachments = Vec::new();
        while let Some(attachment) = r.next_attachment().await? {
            attachments.push(Some(attachment));
        }
        let (r, _) = r.done().payload_len().await?;
        let (r, payload) = r.payload().await?;

        // sent it to the control task
        let task_msg = ReceiverCtrlTaskMsg::ReceivedMessage(ReceivedMessage {
            message_num,
            payload,
            attachments,
        });
        if receiver.send_ctrl_task_msg.send(task_msg).is_err() {
            return Ok(None);
        }

        // read whether there will be another message
        Ok(r.next_message().await?)
    }

    /// Handle an incoming bidirectional QUIC stream.
    async fn handle_bidi(
        self: &Arc<Self>,
        mut send_stream: quinn::SendStream,
        recv_stream: quinn::RecvStream,
    ) -> ResetResult<()> {
        // TODO it's not clear to me why this should be able to reset
        let r = read::bidi_stream(recv_stream, self.side, Arc::clone(&self.has_received_version))
            .await?;
        match r {
            read::BidiFrames::ConnectionControl(r) => {
                r.skip_headers_inner().await?.finish().await?;
                let mut frames = write::Frames::default();
                frames.version();
                frames.connection_control();
                frames.send_on_stream(&mut send_stream).await?;
                send_stream.finish().unwrap();
            }
            read::BidiFrames::ChannelControl(r) => match r.chan_id().await? {
                read::ChanCtrlFrames::Sender(r, chan_id) => {
                    let _ = (r, chan_id);
                    todo!()
                }
                read::ChanCtrlFrames::Receiver(r, chan_id) => {
                    self.handle_receiver_ctrl_frames(chan_id, send_stream, r).await?;
                }
            }
        }
        Ok(())
    }

    /// Handle an incoming control stream for a channel with a local receiver.
    async fn handle_receiver_ctrl_frames(
        self: &Arc<Self>,
        chan_id: ChanIdLocalReceiverLocallyMinted,
        mut send_stream: quinn::SendStream,
        mut next_r: read::ReceiverChanCtrlFrames,
    ) -> ResetResult<()> {
        // get the local net receiver or short-circuit
        let Some(receiver) = self.receivers.get(&ChanIdLocalReceiver::from(chan_id))
        else {
            send_stream.reset((ResetCode::Lost as u32).into()).unwrap();
            return Ok(());
        };

        // claim the right to send a CtrlStreamOpened message to the control task or short-circuit
        if receiver.ctrl_stream_opened.swap(true, Relaxed) {
            send_stream.reset((ResetCode::Lost as u32).into()).unwrap();
            return Ok(());
        }

        // send ownership of the send stream to the channel control task
        // TODO: think harder about how we want to handle race conditions with the control task closing
        // TODO: related to that, we need to relay ctrl stream resets to the control task
        let task_msg = ReceiverCtrlTaskMsg::CtrlStreamOpened(send_stream);
        let _todo = receiver.send_ctrl_task_msg.send(task_msg);

        // read incoming control frames and relay them to the receiver task
        // (loop breaks upon encountering beginning of final frame on stream)
        let r = loop {
            match next_r.next_frame().await? {
                read::ReceiverChanCtrlFrame::SentUnreliable(r) => {
                    let (r, delta) = r.delta().await?;
                    let task_msg = ReceiverCtrlTaskMsg::ReceivedSentUnreliable { delta };
                    let _todo = receiver.send_ctrl_task_msg.send(task_msg);
                    next_r = r;
                }
                read::ReceiverChanCtrlFrame::FinishSender(r) => break r,
            };
        };

        let (r, reliable_count) = r.reliable_count().await?;
        let task_msg = ReceiverCtrlTaskMsg::ReceivedFinishSender { reliable_count };
        let _todo = receiver.send_ctrl_task_msg.send(task_msg);
        r.finish().await?;

        Ok(())
    }

    /// Body of a receiver-side channel control task.
    async fn receiver_ctrl(
        &self,
        send_ctrl_task_msg: TokioUnboundedSender<ReceiverCtrlTaskMsg>,
        mut recv_ctrl_task_msg: TokioUnboundedReceiver<ReceiverCtrlTaskMsg>,
    ) -> Result<()> {
        let mut gateway_buf = Vec::new();
        let mut gateway_obj = None;
        let mut send_stream = None;

        let mut unreliable_ack_mgr = ReceiverUnreliableAckManager::default();
        let mut sent_unreliable_base = 0u64;

        while let Some(task_msg) = recv_ctrl_task_msg.recv().await {
            match task_msg {
                ReceiverCtrlTaskMsg::ReceiverTaken(mut gateway) => {
                    // we now have the ability to convey received messages to the application
                    for app_msg in gateway_buf.drain(..) {
                        (gateway)(app_msg)?;
                    }
                    gateway_obj = Some(gateway);
                },
                ReceiverCtrlTaskMsg::ReceivedMessage(app_msg) => {
                    // we have received a message from the remote side
                    match app_msg.message_num {
                        MessageNum::Reliable(n) => todo!(),
                        MessageNum::Unreliable(n) => {
                            let already_received = unreliable_ack_mgr
                                .on_receive(
                                    n,
                                    &send_ctrl_task_msg,
                                    send_stream.is_some(),
                                )?;
                            if already_received {
                                debug!("ignoring late-arriving unreliable message");
                                continue;
                            }
                        }
                    }

                    if let Some(gateway) = gateway_obj.as_mut() {
                        (gateway)(app_msg)?;
                    } else {
                        gateway_buf.push(app_msg);
                    }
                },
                ReceiverCtrlTaskMsg::CtrlStreamOpened(stream) => {
                    // we now have the ability to send control frames to the remote side
                    send_stream = Some(stream);
                    unreliable_ack_mgr.on_ctrl_stream_opened(&send_ctrl_task_msg);
                },
                ReceiverCtrlTaskMsg::ReceivedSentUnreliable { delta } => {
                    // the remote side declared having sent an unreliable message
                    sent_unreliable_base = sent_unreliable_base
                        .checked_add(delta.get())
                        .ok_or_else(|| anyhow!("received SentUnreliable overflow"))?;
                    // underflow safety: delta is non-zero
                    let n = sent_unreliable_base - 1;

                    unreliable_ack_mgr.on_declared(n, &send_ctrl_task_msg, send_stream.is_some())?;
                },
                ReceiverCtrlTaskMsg::ReceivedFinishSender { reliable_count } => {
                    let _ = reliable_count;
                    // TODO
                },
                ReceiverCtrlTaskMsg::CtrlStreamReset(code) => {
                    let _ = code;
                },
                ReceiverCtrlTaskMsg::UnreliableAckTimer => {
                    unreliable_ack_mgr
                        .on_timer_event(&send_ctrl_task_msg, &mut send_stream)
                        .await?;
                },
                ReceiverCtrlTaskMsg::ReliableAckTimer => {

                },
            }
        }
        Ok(())
    }

    /// Take messages from gateway, encode them, and transmit them on the given channel.
    ///
    /// Returning error triggers connection close.
    async fn drive_sender<M, E: EncoderAttacher<M>>(
        &self,
        chan_id: ChanIdLocalSenderMultishot,
        gateway: IntoReceiver<M>,
        mut encoder: E,
    ) -> Result<()> {
        let gateway = gateway.into_receiver();
        let mut single_stream = None;
        let mut message_num = 0;
        
        // loop for as long as we receive messages from the gateway
        let end_error: Option<RecvError> = loop {
            let msg: M = match gateway.recv().await {
                Ok(Some(msg)) => msg,
                Ok(None) => break None,
                Err(e) => break Some(e),
            };

            // encode-attach message payload
            let attach_target = AttachTarget(std::marker::PhantomData);
            let payload = catch_unwind(AssertUnwindSafe(|| encoder.encode(msg, attach_target)))
                .map_err(|_| anyhow!("application's EncoderAttacher panicked"))
                .and_then(|r| r.context("application's EncoderAttacher errored"))?;
            
            // encode frames
            let mut frames = write::Frames::new_with_version_if(&self.must_send_version);
            let attachments = write::Attachments::default();
            // TODO encode attachments
            frames.message(chan_id, message_num, attachments, payload);
            message_num += 1;
            
            // transmit frames
            match gateway.delivery_guarantees() {
                DeliveryGuarantees::Unconverted => {
                    unreachable!("we received a message, so it must be converted");
                },
                DeliveryGuarantees::Ordered => {
                    if single_stream.is_none() {
                        single_stream = Some(self.quic_conn.open_uni().await?);
                    }
                    frames.send_on_stream(single_stream.as_mut().unwrap()).await?;
                }
                DeliveryGuarantees::Unordered => {
                    frames.send_on_new_stream(&self.quic_conn).await?;
                }
                DeliveryGuarantees::Unreliable => {
                    frames.send_on_datagram(&self.quic_conn).await?;
                }
            };
        };

        // handle the optional end error
        match end_error {
            None => {
                if let Some(mut single_stream) = single_stream {
                    single_stream.finish().expect("this cannot error because we never reset it");
                }
                todo!()
            },
            Some(RecvError::Cancelled(_)) => todo!(),
            Some(RecvError::ConnectionLost(_)) => todo!(),
            Some(RecvError::ChannelLostInTransit(_)) => todo!(),
        }
    }

    /// Close the connection if result holds an error.
    fn close_on_error(&self, result: Result<()>) {
        if let Err(e) = result {
            error!(%e, "closing connection due to error");
            self.quic_conn.close(0u32.into(), &[]);
        }
    }
}


/// Logic for encoding a message and attaching its attachments in some format.
pub trait EncoderAttacher<M>: Send + Sync + 'static {
    fn encode(&mut self, msg: M, attach: AttachTarget) -> Result<MultiBytes>;
}

/// Logic for decoding a message and detaching its attachments in some format.
pub trait DecoderDetacher<M>: Send + Sync + 'static {
    fn decode(&mut self, encoded: MultiBytes, detach: DetachTarget) -> Result<M>;
}

/// Passed to an [`EncoderAttacher`] to attach attachments to.
pub struct AttachTarget<'a>(std::marker::PhantomData<&'a ()>);

impl<'a> AttachTarget<'a> {
    pub fn attachments(&self) -> usize {
        todo!()
    }

    pub fn attach_sender<M, D>(&mut self, sender: IntoSender<M>, decoder: D) -> usize
    where
        D: DecoderDetacher<M>,
    {
        let _ = (sender, decoder);
        todo!()
    }

    pub fn attach_receiver<M, E>(&mut self, receiver: IntoReceiver<M>, encoder: E) -> usize
    where
        E: EncoderAttacher<M>,
    {
        let _ = (receiver, encoder);
        todo!()
    }
    /*
    pub fn attach_oneshot_sender<M, D>(&mut self, sender: OneshotSender<M>, decoder: D) -> usize
    where
        D: DecoderDetacher<M>,
    {
        let _ = (sender, decoder);
        todo!()
    }

    pub fn attach_oneshot_receiver<M, E>(&mut self, receiver: OneshotReceiver<M>, encoder: E) -> usize
    where
        E: EncoderAttacher<M>,
    {
        let _ = (receiver, encoder);
        todo!()
    }*/
}

/// Passed to an [`DecoderDetacher`] to detach attachments from.
pub struct DetachTarget<'a>(std::marker::PhantomData<&'a ()>);

impl<'a> DetachTarget<'a> {
    pub fn attachments(&self) -> usize {
        todo!()
    }

    pub fn detach_sender<M, E: EncoderAttacher<M>>(
        &mut self,
        attachment_idx: u32,
        encoder: E,
    ) -> Option<IntoSender<M>> {
        let _ = (attachment_idx, encoder);
        todo!()
    }

    pub fn detach_receiver<M, D: DecoderDetacher<M>>(
        &mut self,
        attachment_idx: u32,
        decoder: D,
    ) -> Option<IntoReceiver<M>> {
        let _ = (attachment_idx, decoder);
        todo!()
    }
    /*
    pub fn detach_oneshot_sender<M, E: EncoderAttacher<M>>(
        &mut self,
        attachment_idx: u32,
        encoder: E,
    ) -> Option<OneshotSender<M>> {
        let _ = (attachment_idx, encoder);
        todo!()
    }

    pub fn detach_oneshot_receiver<M, D: DecoderDetacher<M>>(
        &mut self,
        attachment_idx: u32,
        decoder: D,
    ) -> Option<OneshotReceiver<M>> {
        let _ = (attachment_idx, decoder);
        todo!()
    }*/
}
