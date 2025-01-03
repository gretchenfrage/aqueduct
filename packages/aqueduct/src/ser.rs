// application-level serializing and deserializing messages.

use crate::{
    zero_copy::*,
    Sender,
    IntoSender,
    IntoReceiver,
};
use anyhow::*;


/// Logic for encoding a message and attaching its attachments in some format.
pub trait EncoderAttacher<M>: Send + Sync + 'static {
    fn encode(&self, msg: M, attach: AttachTarget) -> Result<MultiBytes>;
}

/// Logic for decoding a message and detaching its attachments in some format.
pub trait DecoderDetacher<M>: Send + Sync + 'static {
    fn decode(&self, encoded: MultiBytes, detach: DetachTarget) -> Result<M>;
}

/// Passed to an [`EncoderAttacher`] to attach attachments to.
pub struct AttachTarget {

}

impl AttachTarget {
    pub fn attach_sender<M, D>(&mut self, _sender: IntoSender<M>, _decoder: D) -> u32
    where
        D: DecoderDetacher<M>,
    {
        todo!()
    }

    pub fn attach_receiver<M, E>(&mut self, _receiver: IntoReceiver<M>, _encoder: E) -> u32
    where
        E: EncoderAttacher<M>,
    {
        todo!()
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
    }*/
}

/// Passed to an [`DecoderDettacher`] to detach attachments from.
pub struct DetachTarget {
}

impl DetachTarget {
    pub fn detach_sender<M, E: EncoderAttacher<M>>(
        &mut self,
        _attachment_idx: u32,
        _encoder: E,
    ) -> Option<IntoSender<M>> {
        todo!()
    }

    pub fn detach_receiver<M, D: DecoderDetacher<M>>(
        &mut self,
        _attachment_idx: u32,
        _decoder: D,
    ) -> Option<IntoReceiver<M>> {
        todo!()
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
    }*/
}
