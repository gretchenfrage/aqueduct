#[macro_use]
extern crate tracing;

mod base64;
mod ed25519;
mod url;

pub mod rustls;

pub use crate::{
    ed25519::{KeyPair, PrivateKey, PublicKey},
    url::{QKAI_URL_MAX_LEN, QkaiUrl, ToQkaiUrl},
};
