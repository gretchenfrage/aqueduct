//! Utilities for zero-copy networking

pub(crate) mod multi_bytes;
pub(crate) mod quic;

// public re-exports
pub use self::multi_bytes::{
    MultiBytes,
    TooFewBytesError,
};

//mod multi_bytes_writer;
/*

pub use self::{
    multi_bytes::{
        MultiBytes,
        TooFewBytesError,
    },
    multi_bytes_writer::MultiBytesWriter,
};
*/