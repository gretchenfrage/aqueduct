
mod chan_id_mint;

use self::chan_id_mint::ChanIdMint;
use crate::{
    util::atomic_take::AtomicTake,
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
        atomic::AtomicBool,
        Arc,
    },
    panic::{AssertUnwindSafe, catch_unwind},
};
use dashmap::DashMap;
use tokio::{
    sync::{
        mpsc::{
            UnboundedSender as TokioUnboundedSender,
            UnboundedReceiver as TokioUnboundedReceiver,
            unbounded_channel as tokio_unbounded_channel,
        },
        Mutex as TokioMutex,
    },
    task::spawn,
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
    /// Message receiver of messages to the local side's channel control task, if that task does
    /// not yet exist. The local side's channel control task is spawned when the local side
    /// experiences the establishment of the channel control stream:
    ///
    /// - For Aqueduct channels created by the remote side, the local side creates the channel
    ///   control stream at the same time as it creates this Receiver, and thus the channel control
    ///   task is spawned immediately and this AtomicTake is initialized as None.
    /// - For Aqueduct channels create by the local side, the remote side creates the channel
    ///   control stream, and thus there is an asynchronous delay between the local side creating
    ///   this Receiver and the local side observing the establishment of the channel control
    ///   stream. As such, this AtomicTake is initialized as Some, and then its value is taken
    ///   later to be owned by the local side's channel control task.
    recv_ctrl_task_msg: AtomicTake<TokioUnboundedReceiver<ReceiverCtrlTaskMsg>>,
    /// Gateway for giving received messages to application deserialization logic. 
    gateway: TokioMutex<ReceiverGateway>,
}

/// Message to a receiver-side channel control task.
enum ReceiverCtrlTaskMsg {

}

/// Gateway for giving received messages to application deserialization logic.
enum ReceiverGateway {
    /// The application has not yet taken the receiver. The gateway holds a buffer of received
    /// but not yet application-level deserialized messages.
    Buffer(Vec<ReceivedMessage>),
    /// The application has taken the receiver. The dyn object will invoke application-level
    /// deserialization logic on the message, then relay it to the application's Aqueduct channel
    /// handle, or return error on failure or panic in the application-level deserialization logic.
    Object(Box<dyn FnMut(ReceivedMessage) -> Result<()> + Send>),
}

/// Message within a channel that has been fully received on the Aqueduct protocol level, but not
/// yet deserialized at the application level.
struct ReceivedMessage {
    payload: MultiBytes,
    attachments: Vec<Option<ReceivedAttachment>>,
}

/// Attachment in a `ReceivedMessage`.
enum ReceivedAttachment {
    Sender(ChanIdLocalSenderRemotelyMinted),
    Receiver(ChanIdLocalReceiverRemotelyMinted),
}

impl Connection {
    /// Handle an incoming unidirectional QUIC stream.
    ///
    /// Returning reset error is ignored. Returning other error triggers connection close.
    async fn handle_uni(&self, stream: quinn::RecvStream) -> ResetResult<()> {
        let r = read::uni_stream(stream, self.side, Arc::clone(&self.has_received_version)).await?;
        match r {
            read::UniFrames::Message(r) => self.handle_messages(r).await,
            read::UniFrames::ClosedChannelLost(r) => {
                let _ = r;
                todo!()
            }
        }
    }

    /// Handle a sequence of incoming Message frames.
    async fn handle_messages(&self, r: read::Message) -> ResetResult<()> {
        // route
        let is_reliable = r.reliable();
        let (r, sent_on) = r.sent_on().await?;

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
                            recv_ctrl_task_msg: AtomicTake::none(),
                            gateway: TokioMutex::new(ReceiverGateway::Buffer(Vec::new())),
                        }
                    })
                    .downgrade()
            }
        };

        // handle the remainder of the first Message frame
        let mut next_r = self.handle_routed_message(sent_on, is_reliable, &receiver, r).await?;

        // handle subsequent Message frames
        while let Some(r) = next_r.next_message().await? {
            let (r, sent_on_2) = r.sent_on().await?;

            if sent_on_2 != sent_on {
                return Err(anyhow!(
                    "received multiple Message frames on same stream with different sent_on"
                ).into());
            }

            next_r = self.handle_routed_message(sent_on, is_reliable, &*receiver, r).await?;
        }

        Ok(())
    }

    /// Handle an incoming Message frame that has already been routed to a receiver.
    async fn handle_routed_message(
        &self,
        sent_on: ChanIdLocalReceiver,
        is_reliable: bool,
        receiver: &NetReceiver,
        r: read::Message2,
    ) -> ResetResult<read::NextMessage> {
        // read the rest of the message
        let (r, message_num) = r.message_num().await?;
        let (mut r, _) = r.attachments_len().await?;
        let mut attachments = Vec::new();
        while let Some(attachment) = r.next_attachment().await? {
            attachments.push(Some(match attachment.sort_by_dir(self.side) {
                // TODO do something extra here
                SortByDir::LocalSender(chan_id) => ReceivedAttachment::Sender(chan_id),
                SortByDir::LocalReceiver(chan_id) => ReceivedAttachment::Receiver(chan_id),
            }));
        }
        let (r, _) = r.done().payload_len().await?;
        let (r, payload) = r.payload().await?;
        let received = ReceivedMessage {
            payload,
            attachments,
        };

        // convey it to the application
        match &mut *receiver.gateway.lock().await {
            &mut ReceiverGateway::Buffer(ref mut vec) => vec.push(received),
            &mut ReceiverGateway::Object(ref mut obj) => (obj)(received)?,
        }

        let _ = (sent_on, is_reliable, receiver, message_num);
        // TODO

        Ok(r)
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

    /// Handle incoming channel control frames for a channel with a local receiver.
    async fn handle_receiver_ctrl_frames(
        self: &Arc<Self>,
        chan_id: ChanIdLocalReceiverLocallyMinted,
        mut send_stream: quinn::SendStream,
        mut next_r: read::ReceiverChanCtrlFrames,
    ) -> ResetResult<()> {
        // get the ctrl task msg receiver, or short-circuit
        let Some(recv_ctrl_task_msg) = self.receivers
            .get(&ChanIdLocalReceiver::from(chan_id))
            .and_then(|receiver| receiver.recv_ctrl_task_msg.take())
        else {
            send_stream.reset((ResetCode::Lost as u32).into()).unwrap();
            return Ok(());
        };

        // spawn the channel control task
        spawn({
            let this = Arc::clone(self);
            async move {
                let result = this.receiver_ctrl(recv_ctrl_task_msg, send_stream).await;
                this.close_on_error(result);
            }
        });

        // read incoming control frames and relay them to the receiver task
        loop {
            match next_r.next_frame().await? {
                read::ReceiverChanCtrlFrame::SentUnreliable(r) => {
                    let (r, delta) = r.delta().await?;
                    // TODO
                    let _ = delta;
                    next_r = r;
                }
                read::ReceiverChanCtrlFrame::FinishSender(r) => {
                    let (r, reliable_count) = r.reliable_count().await?;
                    // TODO
                    let _ = reliable_count;
                    r.finish().await?;
                    break;
                }
            };
        }

        Ok(())
    }

    /// Body of a channel control task on the receiver side.
    async fn receiver_ctrl(
        &self,
        recv_ctrl_task_msg: TokioUnboundedReceiver<ReceiverCtrlTaskMsg>,
        send_stream: quinn::SendStream,
    ) -> Result<()> {
        let _ = (recv_ctrl_task_msg, send_stream);
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

/// Passed to an [`DecoderDettacher`] to detach attachments from.
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
