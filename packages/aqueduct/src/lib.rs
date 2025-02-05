#![allow(dead_code)] // TODO
//#![doc = include_str!("../../../README.md")] // TODO: fix links

#[macro_use]
extern crate tracing;

pub extern crate bytes;

pub mod zero_copy;

mod channel;
pub mod util;
pub mod frame;
//mod message_encoder_api;
pub mod proto;

pub use crate::channel::api::*;

/// Error types
pub mod error {
    pub use crate::channel::error::*;
}

/// Future types
pub mod future {
    pub use crate::channel::api::future::*;
}
