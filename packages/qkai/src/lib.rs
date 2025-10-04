//! QKAI ("QUIC Key At IP") is a URL format for DNS-free TLS-authentication.
//!
//! Basically, we have a "qkai URL" which looks like this:
//!
//! ```text
//! qkai://KpEkgtLmGEs4JMD-uSSNMF_EnOQYesylTkvohvggK3A@[fe00:0:0:1::92]:8686
//!        \_________________________________________/ \___________________/
//!         Ed25519 public key                          socket address (IPV6 example)
//! ```
//!
//! Here's another example which elides the URL scheme and uses an IPV4 address instead of an IPV6
//! one:
//!
//! ```text
//! NKVsh-LuNDGkbDKY2xK6pFEYDv1EQsu6KMMguaQ98-Y@202.76.160.43:12245
//! \_________________________________________/ \_________________/
//!  Ed25519 public key                          socket address (IPV4 example)
//! ```
//!
//! We put a raw IP address and port in the host part of the URL, and we "creatively" abuse the
//! username part of the URL to store the unpadded Base64URL-encoded Ed25519 public key we expect
//! the server to use for TLS. We expect the server to be using a self-signed TLS certificate with
//! that public key, and if the server can't prove that it possesses the corresponding private key
//! the client will refuse to connect.
//!
//! You can read this as "key at IP" because the key identifies what server we're trying to connect
//! to (in terms of authenticated cryptographic identity), the IP identifies where we're trying to
//! find that server (in terms of socket address to connect to send packets to), and they are
//! separated by an at sign.
//!
//! ## Motivation
//!
//! Currently, TLS-based systems are built around the assumption that servers will be located by a
//! DNS host name, and authenticated by certificate authorities which tie these host names to
//! trusted public keys. This certainly has its place, for example, once it's all set up it makes
//! it very convenient to go to e.g. [phoenixkahlo.com](https://phoenixkahlo.com) and get a nice
//! authenticated connection.
//!
//! However, this approach doesn't scale down in terms of simplicity very easily. For example, when
//! someone spins up a quick service running peer-to-peer or in LAN or in localhost, it can be more
//! hassle than it's worth to set it up with both DNS and CAs. If this is done in the "open
//! internet", it requires setting renting a domain name from someone, working with one of the
//! certificate authorities that exists in the world, and overall setting up infrastructure. If
//! this is done in some sort of private intranet sort of way, it requires setting up things on the
//! client-side to get those computers to interact with your custom DNS servers and certificate
//! authorities, which there isn't a very standardized way to do and often has to be done at the
//! OS-level. And then you may still have to host infrastructure. These things work for corporate
//! contexts but they're a hassle.
//!
//! Because of this, this often leads systems to be deployed either without encryption whatsoever,
//! or with encryption but without authentication. Not using encryption whatsoever can be done by
//! creating a raw TCP connection and not running TLS over it--but we are interested in replacing
//! TCP with QUIC, which always uses TLS, so we can't do that. Using encryption but not
//! authentication can be done by having the server use a self-signed certificate, and giving the
//! client a custom certificate verifier which blindly trusts whatever certificate the server has.
//! This can be done either with TLS-over-TCP (e.g. that's what Minecraft does) or with QUIC (e.g.
//! much QUIC example code does that), but it basically throws away almost all of the protection
//! TLS gives, as it's still totally vulnerable to a MITM attack at start-up time.
//!
//! The qkai URL format aims to improve the state of cybersecurity by filling the "missing middle"
//! of the security-convenience tradeoff spectrum between "no authentication" and "certificate
//! authorities." We can set up the server to randomly generate and use a self-signed certificate,
//! and then pair that certificates's public key with the server IP's address and port to form a
//! URL which is tied to the server's private key. Thus, if we give that URL to a client, and the
//! client successfully connects to it, we know that the client created a private connection to the
//! real server (unless, of course, the server's private key was stolen, or the URL was tampered
//! with before being given to the client).
//!
//! This is _almost_ as convenient as completely disabling authentication, with the gap being that
//! the URLs are now longer, and the URLs become invalid if the server's private key is lost. This
//! is _almost_ as secure as using certificate authorities, with the gap being that there is no
//! built-in way to revoke an old key pair if the private key is leaked or stolen. Be aware of
//! these limitations and the fact that this is not universally better than using certificate
//! authorities!
//!
//! ## How it works
//!
//! We give that qkai URL to the client, and it uses the socket address to know how to _locate_ the
//! server, and the public key to know how to _authenticate_ the server. The client does this by
//! plugging in a custom server certificate verifier to its TLS layer which asserts that the server
//! connection is tied to the expected public key.
//!
//! The client also passes the unpadded Base64URL-encoded Ed25519 public key as the expected server
//! name when initiating the TLS connection, so that the server knows what key the client is
//! expecting and may choose between multiple different key pairs it possesses based on that.

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
