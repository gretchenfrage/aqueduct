#![allow(dead_code)] // TODO
#![doc = include_str!("../../../README.md")] // TODO: fix links

#[macro_use]
extern crate tracing;

pub extern crate bytes;
#[cfg(feature = "futures")]
pub extern crate futures;

pub mod docs;
pub mod zero_copy;

mod channel;
mod codec;
//mod ser;
mod proto;

pub use crate::{
    channel::api::*,
    proto::{
        EncoderAttacher,
        DecoderDetacher,
        AttachTarget,
        DetachTarget,
    },
    //ser::*,
};

/// Error types
pub mod error {
    pub use crate::channel::error::*;
}

/// Future types
pub mod future {
    pub use crate::channel::api::future::*;
}
