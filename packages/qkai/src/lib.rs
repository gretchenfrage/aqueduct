
#[macro_use]
extern crate tracing;

mod ed25519;
mod error;
mod base64;
mod url;
mod cert;
mod endpoint;

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
        Client,
        Server,
        ToQkaiUrl,
    },
};
