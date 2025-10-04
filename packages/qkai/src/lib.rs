#[macro_use]
extern crate tracing;

mod base64;
mod ed25519;
mod error;
mod url;

pub mod cert;
pub mod endpoint;

pub use crate::{
    ed25519::{KeyPair, PrivateKey, PublicKey},
    endpoint::ToQkaiUrl,
    error::Error,
    url::{QKAI_URL_MAX_LEN, QkaiUrl},
};
