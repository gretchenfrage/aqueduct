
#[macro_use]
extern crate tracing;

mod ed25519;
mod error;
mod base64;
mod url;

pub mod endpoint;
pub mod cert;

pub use crate::{
    error::Error,
    ed25519::{
        PublicKey,
        PrivateKey,
        KeyPair,
    },
    url::{
        QkaiUrl,
        QKAI_URL_MAX_LEN,
    },
    endpoint::{
        ToQkaiUrl,
    },
};
