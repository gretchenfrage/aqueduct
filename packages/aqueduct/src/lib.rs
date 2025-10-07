#![allow(unsafe_op_in_unsafe_fn)]
//#![doc = include_str!("../../../README.md")] // TODO: fix links

#[allow(unused_imports)]
#[macro_use]
extern crate tracing;

pub extern crate bytes;
pub extern crate multibytes;
pub extern crate qkai;

mod channel;
#[allow(dead_code)]
mod frame;
#[allow(dead_code)]
mod proto;
mod quic_zc;

pub use crate::public_api::*;

mod public_api {
    pub use crate::channel::{api::*, error::*};

    /// Future types
    pub mod future {
        pub use crate::channel::api::future::*;
    }
}
