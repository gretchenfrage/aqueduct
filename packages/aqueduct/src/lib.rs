#![allow(dead_code)] // TODO
//#![doc = include_str!("../../../README.md")] // TODO: fix links

pub extern crate bytes;
#[cfg(feature = "futures")]
pub extern crate futures;

pub mod docs;
pub mod zero_copy;

mod channel;

pub use crate::channel::api::*;

/// Error types
pub mod error {
    pub use crate::channel::error::*;
}

/// Future types
pub mod future {
    pub use crate::channel::api::future::*;
}
