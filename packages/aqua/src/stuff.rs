
use crate::{
    frame::*,
    zero_copy::MultiBytes
};
use std::{
    cell::OnceCell,
    sync::Arc,
};
use tokio::{
    sync::Notify,
    task::spawn,
};
use dashmap::DashMap;
use quinn::*;
use anyhow::*;


// shared internal connection-level state
struct ConnectionCtx {
    // the QUIC connection
    connection: Connection,
    // map from channel IDs to objects to handle received encoded messages
    recv_handlers: DashMap<ChannelId, RecvHandler>,
}

// value in ConnectionState.recv_handlers
struct RecvHandler {
    // dyn object that contains user-provided type + format-specific decoding-detaching logic fused
    // with internal protocol logic.
    // 
    // when a remote receiver is received, we may begin receiving messages on the channel before we
    // receive the message the receiver was attached to. for that reason, we need to be able to
    // represent a not-yet-initialized RecvHandler and await its initialization.
    object: OnceCell<Box<dyn RecvHandlerObj>>,
    // notified after initializing `object`
    notify: Notify,
}

// see RecvHandler.object
trait RecvHandlerObj: Send + Sync + 'static {
    // called to handle a received message frame addressed to the corresponding channel
    fn handle(&self, frame: MessageFrame, ctx: &Arc<ConnectionCtx>);
}

// RecvHandlerObj implementation for a networked receiver
struct RecvHandlerChannelImpl<M, D> {
    // user-provided logic for decoding-detaching messages
    decoder: D,
    // internal sender corresponding to the user-owned networked receiver
    send: NonBlockingSender<M>, // TODO: how to provide sender backpressure?
}

impl<M, D: DecoderDetacher<M>> RecvHandlerObj for RecvHandlerChannelImpl<M, D> {
    fn handle(&self, frame: MessageFrame, ctx: &Arc<ConnectionCtx>) -> Result<(), Error> {
        let MessageFrame { dst: _, mut attachments, payload } = frame;
        let detach = DetachTarget {
            remaining_attachments: attachments.len(),
            attachments: &mut attachments,
            ctx,
        };
        let msg = self.decoder.decode(payload, detach)
            .context("message decoder-detacher error")?;
        self.send.send(msg); // TODO: how to handle this error?
        // TODO: deal with not-detached channels
        Ok(())
    }
}

// RecvHandlerObj implementation for a networked oneshot receiver
struct RecvHandlerOneshotImpl<M, D> {
    // user-provided logic for decoding-detaching messages
    decoder: D,
    // internal sender corresponding to the user-owned networked receiver
    send: OneshotSender<M>,
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
pub struct AttachTarget {

}

impl AttachTarget {
    pub fn attach_sender<M, D>(&mut self, sender: IntoSender<M>, decoder: D) -> u32
    where
        D: DecoderDetacher<M>,
    {
        todo!()
    }

    pub fn attach_receiver<M, E>(&mut self, receiver: Receiver<M>, encoder: E) -> u32
    where
        E: EncoderAttacher<M>,
    {
        todo!()
    }

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
}

/// Passed to an [`DecoderDettacher`] to detach attachments from.
pub struct DetachTarget<'a> {
    attachments: &'a mut [ChannelId],
    ctx: &'a Arc<ConnectionCtx>,
    remaining_attachments: usize,
}

impl<'a> DetachTarget<'a> {
    pub fn detach_sender<M, E: EncoderAttacher<M>>(
        &mut self,
        attachment_idx: u32,
        encoder: E,
    ) -> Result<IntoSender<M>, MissingAttachment> {
        todo!()
    }

    pub fn detach_receiver<M, D: DecoderDetacher<M>>(
        &mut self,
        attachment_idx: u32,
        decoder: D,
    ) -> Result<Receiver<M>, MissingAttachment> {
        todo!()
    }

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
}

pub struct MissingAttachment;
