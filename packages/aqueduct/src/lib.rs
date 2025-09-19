#![allow(unsafe_op_in_unsafe_fn)]
//#![doc = include_str!("../../../README.md")] // TODO: fix links

#[allow(unused_imports)]
#[macro_use]
extern crate tracing;

pub extern crate bytes;
pub extern crate multibytes;

mod channel;
mod frame;
mod quic_zc;

pub use crate::channel::api::*;

/// Error types
pub mod error {
    pub use crate::channel::error::*;
}

/// Future types
pub mod future {
    pub use crate::channel::api::future::*;
}

pub mod todo {
    pub use crate::frame::*;
}
