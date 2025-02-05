/*
//! Public-facing EncoderAttacher/DecoderAttacher API shell.

use anyhow::*;
use crate::{
    zero_copy::MultiBytes,
    proto::{AttachTargetInner, DetachTargetInner},
    channel::api::*,
};


/// Logic for encoding a message and attaching its attachments in some format.
pub trait EncoderAttacher<M>: Send + Sync + 'static {
    fn encode(&self, msg: M, attach: AttachTarget) -> Result<MultiBytes>;
}

/// Logic for decoding a message and detaching its attachments in some format.
pub trait DecoderDetacher<M>: Send + Sync + 'static {
    fn decode(&self, encoded: MultiBytes, detach: DetachTarget) -> Result<M>;
}

/// Passed to an [`EncoderAttacher`] to attach attachments to.
pub struct AttachTarget<'a>(pub(crate) AttachTargetInner<'a>);

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
pub struct DetachTarget<'a>(pub(crate) DetachTargetInner<'a>);

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


*/