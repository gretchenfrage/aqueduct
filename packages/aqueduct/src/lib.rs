#![allow(dead_code)] // TODO

pub extern crate bytes;

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
