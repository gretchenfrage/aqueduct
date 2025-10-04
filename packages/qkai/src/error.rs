//! Error types for this crate.

use std::io;
use thiserror::Error;

// TODO: this is insanely superfluous

/// Error type for qkai.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    /// Public and private keys do not match when constructing a key pair.
    #[error("public and private keys do not match")]
    KeyPairMismatch,
    /// String was wrong length for base64 representation of ed25519 key.
    #[error("base64 wrong length for ed25519")]
    Base64WrongLength,
    /// Character was not in the alphabet for base64 representation of ed25519 key.
    #[error("base64 contained non-base64-url-safe character")]
    Base64IllegalCharacter,
    /// The final base64 character contained non-zero bits that get cut truncated.
    #[error("base64 contained non-zero extra bits in final character")]
    Base64ExtraBits,
    /// String was wrong length for hexadecimal representation of ed25519 key.
    #[error("hexadecimal wrong length for ed25519")]
    HexadecimalWrongLength,
    /// Character was not in the alphabet for lower-hex representation of ed25519 key.
    #[error("hexadecimal contained non-lower-hex character")]
    HexadecimalIllegalCharacter,
    /// A qkai url was too long to possibly be valid.
    #[error("url too long to be valid")]
    UrlTooLong,
    /// A qkai url had too few characters after the optional 'qkai://' prefix to possibly be valid.
    #[error("url too short (after optional 'qkai://') to be valid")]
    UrlTooShort,
    /// A qkai url did not have an '@' symbol at the 44th byte after the optional 'qkai://' prefix.
    #[error("url missing @ symbol at expected location")]
    UrlMissingAtSymbol,
    /// A qkai url did not have a ':' character within the last 6 characters.
    #[error("url missing colon character in expected range")]
    UrlMissingColon,
    /// A qkai url's ip address was too short to possibly be valid.
    #[error("url ip address too short")]
    UrlIpAddrTooShort,
    /// A qkai url's ipv6 address contained non-utf8 bytes.
    #[error("url ipv6 address not utf8")]
    UrlIpv6AddrNonUtf8,
    /// A qkai url's ipv6 address failed to parse as an ipv6 address.
    #[error("url ipv6 address not valid")]
    UrlIpv6AddrInvalid,
    /// A qkai url's ipv4 address contained non-utf8 bytes.
    #[error("url ipv4 address not utf8")]
    UrlIpv4AddrNonUtf8,
    /// A qkai url's ipv4 address failed to parse as an ipv4 address.
    #[error("url ipv6 address not valid")]
    UrlIpv4AddrInvalid,
    /// A qkai url's port was an empty string.
    #[error("url port was empty string")]
    UrlEmptyPort,
    /// A qkai url's port contained non-utf8 bytes.
    #[error("url port not utf8")]
    UrlPortNonUtf8,
    /// A qkai url's port failed to parse as a u16.
    #[error("url port not valid")]
    UrlPortInvalid,
    /// Serde json error.
    #[error("serde json error")]
    SerdeJson(serde_json::Error),
    /// IO error.
    #[error("io error")]
    Io(io::Error),
    /// A key pair json file was too large to pass a sanity check.
    #[error("key pair file too large")]
    KeyPairFileTooLarge,
    /// Tried to write a key pair to a file non-destructively, but the file already exists and its
    /// content is not the same.
    #[error("key pair file already exists with different content")]
    KeyPairFileDifferentContent,
    /// Could not find a suitable key pair file.
    #[error("unable to find suitable key pair file")]
    KeyPairNotFound,
    /// Calling an implementation of `ToSocketAddr` didn't work.
    #[error("ToSocketAddr failed to convert to socket addr")]
    ToSocketAddrFail,
    /// Quinn `ConnectError`.
    #[error("quinn ConnectError")]
    QuinnConnectError(quinn::ConnectError),
    /// Quinn `ConnectionError`.
    #[error("quinn ConnectionError")]
    QuinnConnectionError(quinn::ConnectionError),
    /// All senders into the channel on which new streams are accepted were dropped.
    #[error("new stream channel dropped")]
    NewStreamChannelDropped,
    /// Attempted to construct a client but failed to construct either an IPV4 or IPV6 endpoint.
    #[error("could not bind either an IPV4 or IPV6 address")]
    CouldNotBindClient,
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<quinn::ConnectError> for Error {
    fn from(e: quinn::ConnectError) -> Self {
        Error::QuinnConnectError(e)
    }
}

impl From<quinn::ConnectionError> for Error {
    fn from(e: quinn::ConnectionError) -> Self {
        Error::QuinnConnectionError(e)
    }
}
