#![allow(dead_code)] // TODO
#![allow(unsafe_op_in_unsafe_fn)]
//#![doc = include_str!("../../../README.md")] // TODO: fix links

#[macro_use]
extern crate tracing;

pub extern crate bytes;
pub extern crate multibytes;

mod channel;
mod util;
pub mod quic_zc;
pub mod frame;
//pub mod proto;

pub use crate::channel::api::*;

/// Error types
pub mod error {
    pub use crate::channel::error::*;
}

/// Future types
pub mod future {
    pub use crate::channel::api::future::*;
}
