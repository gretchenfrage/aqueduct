//! Utilities for zero-copy networking

mod multi_bytes;
mod multi_bytes_writer;

pub(crate) mod quic;

pub use self::{
    multi_bytes::{
        MultiBytes,
        Cursor,
        TooFewBytesError,
    },
    multi_bytes_writer::MultiBytesWriter,
};
