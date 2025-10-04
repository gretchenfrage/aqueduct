//! Handling qkai urls.

use crate::ed25519::PublicKey;
use anyhow::{Error, anyhow, ensure};
use std::{
    fmt::{self, Debug, Display, Formatter},
    io,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    str::{self, FromStr},
};

/// Maximum possible ascii character length of a valid qkai url (104).
///
/// This is calculated as the sum of:
///
/// - 7 characters for the optional `qkai://` scheme.
/// - 43 characters for the canonical no-padding base64 url safe public key.
/// - 1 character for the `@` separator.
/// - 2 characters for the `[]` that wrap around IPV6 addresses.
/// - 45 characters for the [longest possible IPV6 address][1].
///
///   [1]: https://stackoverflow.com/questions/166132/maximum-length-of-the-textual-representation-of-an-ipv6-address#166157
/// - 1 character for the `:` separator.
/// - 5 characters for the longest possible port (a 16-bit uint has at most 5 digits).
pub const QKAI_URL_MAX_LEN: usize = 104;

/// A parsed qkai url, consisting of a public key and a socket address.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct QkaiUrl {
    /// The public key that authenticates who to connect to.
    pub public_key: PublicKey,
    /// The socket address that locates who to connect to.
    pub addr: SocketAddr,
}

impl QkaiUrl {
    /// Construct from fields.
    pub fn new(public_key: PublicKey, addr: SocketAddr) -> Self {
        QkaiUrl { public_key, addr }
    }

    /// Attempt to parse from a url string.
    pub fn parse<S: AsRef<[u8]> + ?Sized>(string: &S) -> Result<Self, Error> {
        let mut bytes = string.as_ref();
        // sanity check / dos protection
        ensure!(bytes.len() <= QKAI_URL_MAX_LEN, "qkai URL too long");
        // strip scheme
        if let Some(without_scheme) = bytes.strip_prefix(b"qkai://") {
            bytes = without_scheme;
        }
        // out of bounds indexing protection
        ensure!(bytes.len() >= 45, "qkai URL too short");
        // syntax
        ensure!(bytes[43] == b'@', "qkai URL lacks at-sign at right place");
        // parse the key
        let public_key = PublicKey::from_base64(&bytes[..43])?;
        // find colon that separates the ip and port
        let socket_addr_bytes = &bytes[44..];
        let (colon_idx, _) = socket_addr_bytes
            .iter()
            // only search in last 6 chars for sanity check / dos protection
            .enumerate()
            .rev()
            .take(6)
            .find(|&(_, &c)| c == b':')
            .ok_or_else(|| anyhow!("qkai URL lacks colon at right place"))?;
        // parse ip address
        let ip_addr_bytes = &socket_addr_bytes[..colon_idx];
        // out of bounds indexing protection
        ensure!(ip_addr_bytes.len() >= 3, "qkai URL IP address too short");
        // detect IPV6 brackets
        let ip_addr = if ip_addr_bytes[0] == b'[' {
            // strip brackets
            let ip_addr_bytes_len = ip_addr_bytes.len();
            let ipv6_addr_bytes = &ip_addr_bytes[1..ip_addr_bytes_len - 1];
            // parse
            IpAddr::V6(
                Ipv6Addr::from_str(
                    str::from_utf8(ipv6_addr_bytes)
                        .map_err(|_| anyhow!("qkai URL IPV6 address not UTF-8"))?,
                )
                .map_err(|_| anyhow!("qkai URL invalid IPV6 address"))?,
            )
        } else {
            // parse
            IpAddr::V4(
                Ipv4Addr::from_str(
                    str::from_utf8(ip_addr_bytes)
                        .map_err(|_| anyhow!("qkai URL IPV4 address not UTF-8"))?,
                )
                .map_err(|_| anyhow!("qkai URL invalid IPV4 address"))?,
            )
        };
        // parse port
        // out of bounds indexing protection
        ensure!(
            socket_addr_bytes.len() > colon_idx + 1,
            "qkai URL port part is empty"
        );
        // parse it
        let port = u16::from_str(
            str::from_utf8(&socket_addr_bytes[colon_idx + 1..])
                .map_err(|_| anyhow!("qkai URL port is not UTF-8"))?,
        )
        .map_err(|_| anyhow!("qkai URL invalid port (not a valid u16)"))?;
        // done :)
        Ok(QkaiUrl::new(public_key, SocketAddr::new(ip_addr, port)))
    }

    /// Encode as a url string.
    pub fn to_string(self, scheme: bool) -> String {
        let mut buf = String::new();
        self.encode_buf(&mut buf, scheme);
        buf
    }

    /// Encode as a url string into the provided buf.
    pub fn encode_buf(self, buf: &mut String, scheme: bool) {
        self.encode_fmt(buf, scheme).unwrap();
    }

    /// Encode as a url string into the provided `fmt::Write`.
    pub fn encode_fmt<W: fmt::Write>(self, w: &mut W, scheme: bool) -> Result<(), fmt::Error> {
        if scheme {
            w.write_str("qkai://")?;
        }
        self.public_key.encode_base64_fmt(w)?;
        w.write_str("@")?;
        match self.addr.ip() {
            IpAddr::V4(addr) => write!(w, "{}", addr),
            IpAddr::V6(addr) => write!(w, "[{}]", addr),
        }?;
        write!(w, ":{}", self.addr.port())?;
        Ok(())
    }

    /// Encode as a url string into the provided `io::Write`.
    pub fn encode_io<W: io::Write>(self, w: &mut W, scheme: bool) -> Result<(), io::Error> {
        if scheme {
            w.write_all(b"qkai://")?;
        }
        self.public_key.encode_base64_io(w)?;
        w.write_all(b"@")?;
        match self.addr.ip() {
            IpAddr::V4(addr) => write!(w, "{}", addr),
            IpAddr::V6(addr) => write!(w, "[{}]", addr),
        }?;
        write!(w, ":{}", self.addr.port())?;
        Ok(())
    }
}

impl Debug for QkaiUrl {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        self.encode_fmt(f, false)
    }
}

impl Display for QkaiUrl {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        self.encode_fmt(f, false)
    }
}

impl FromStr for QkaiUrl {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

/// Type that be try to be converted to [`QkaiUrl`].
pub trait ToQkaiUrl {
    fn to_url(self) -> Result<QkaiUrl, Error>;
}

impl ToQkaiUrl for QkaiUrl {
    fn to_url(self) -> Result<QkaiUrl, Error> {
        Ok(self)
    }
}

impl<'a> ToQkaiUrl for &'a str {
    fn to_url(self) -> Result<QkaiUrl, Error> {
        QkaiUrl::parse(self)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn foobar() {
        let url =
            QkaiUrl::parse("qkai://KpEkgtLmGEs4JMD-uSSNMF_EnOQYesylTkvohvggK3A@127.0.0.1:8686")
                .unwrap();
        println!("{}", url);
        assert_eq!(url, QkaiUrl::parse(&url.to_string(false)).unwrap(),);
    }
}
