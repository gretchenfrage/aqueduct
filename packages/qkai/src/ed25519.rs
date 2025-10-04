//! Simple API for ed25519 keys.

use crate::base64::{base64_decode, base64_encode, hex_decode, hex_encode};
use anyhow::{Error, bail, ensure};
use rand::{RngCore as _, rngs::OsRng};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use std::{
    cmp::Ordering,
    fmt::{self, Debug, Display, Formatter},
    fs::{DirEntry, File, read_dir},
    hash::{Hash, Hasher},
    io::{self, Read},
    path::Path,
    str::{self, FromStr},
};

/// A public key (ed25519). Wrapper around `[u8; 32]`.
///
/// Converts to and from its string representation as 43 characters of unpadded canonicalized url
/// safe base64. To break that down, that means:
///
/// - Unpadded: No equals signs at the end to round it up to a multiple of 4 chars, as doing that
///   is [actually pointless][1].
///
///   [1]: https://github.com/marshallpierce/rust-base64/tree/5d70ba7576f9aafcbf02bd8acfcb9973411fb95f?tab=readme-ov-file#i-want-canonical-base64-encodingdecoding
/// - Canonicalized: A base64 character encodes 6 bits of information, and so a 32 byte ed25519
///   key requires 32 ÷ 6 × 8 = 42.666 base64 characters. We of course have to round that up to 43,
///   which leaves 2 "extra bits" of information that don't do anything. We make sure we always
///   output those set to 0, and reject inputs in which they are not set to 0. This ensures that
///   we are able to compare through base64 string comparison if we wish.
/// - Url safe: We use the characters '-' instead of '+' and '_' instead of '/'. Thus, the overall
///   alphabet is:
///
///   ```txt
///   ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_
///   ```
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct PublicKey([u8; 32]);

/// A private key (ed25519). Wrapper around `[u8; 32]`.
///
/// Whereas [`PublicKey`] uses a base64-based string representation, `PrivateKey` instead formats
/// as lower-hex. This is just to make it slightly more noticeable if one accidentally copy/pastes
/// a private key instead of a public key or makes other similar errors.
#[derive(Copy, Clone)]
pub struct PrivateKey([u8; 32]);

/// A public/private key pair (ed25519).
///
/// Converts to and from its JSON representation as:
///
/// ```json
/// {
///   "public": "[public key as canonical no-padding base64 url safe string]",
///   "private": "[private key as lower-hex string]"
/// }
/// ```
#[derive(Copy, Clone, Debug, Serialize)]
pub struct KeyPair {
    public: PublicKey,
    private: PrivateKey,
}

impl PublicKey {
    /// Construct from public key raw bytes.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Get public key raw bytes.
    pub fn to_bytes(self) -> [u8; 32] {
        self.0
    }

    /// Attempt to parse from a canonical no-padding base64 url safe string.
    pub fn from_base64<S: AsRef<[u8]> + ?Sized>(string: &S) -> Result<Self, Error> {
        Ok(PublicKey(base64_decode(string.as_ref())?))
    }

    /// Encode as a canonical no-padding base64 url safe string.
    pub fn to_base64(self) -> String {
        String::from_utf8(self.to_base64_bytes().to_vec()).unwrap()
    }

    /// Encode as a canonical no-padding base64 url safe string as an in-place ascii char array.
    pub fn to_base64_bytes(self) -> [u8; 43] {
        base64_encode(self.to_bytes())
    }

    /// Encode as a canonical no-padding base64 url safe string into the provided buf.
    pub fn encode_base64_buf(self, buf: &mut String) {
        buf.push_str(str::from_utf8(&self.to_base64_bytes()).unwrap());
    }

    /// Encode as a canonical no-padding base64 url safe string into the provided `fmt::Write`.
    pub fn encode_base64_fmt<W: fmt::Write>(self, w: &mut W) -> Result<(), fmt::Error> {
        w.write_str(str::from_utf8(&self.to_base64_bytes()).unwrap())
    }

    /// Encode as a canonical no-padding base64 url safe string into the provided `io::Write`.
    pub fn encode_base64_io<W: io::Write>(self, w: &mut W) -> Result<(), io::Error> {
        w.write_all(&self.to_base64_bytes())
    }
}

impl PrivateKey {
    /// Construct from private key raw bytes.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Get private key raw bytes.
    pub fn to_bytes(self) -> [u8; 32] {
        self.0
    }

    /// Use the OS random number generator to randomly generate a private key.
    pub fn generate() -> Self {
        let mut buf = [0; 32];
        OsRng.fill_bytes(&mut buf);
        Self::from_bytes(buf)
    }

    /// Compute the public key corresponding to this private key.
    pub fn to_public_key(self) -> PublicKey {
        PublicKey::from_bytes(
            *ed25519_dalek::SigningKey::from_bytes(&self.to_bytes())
                .verifying_key()
                .as_bytes(),
        )
    }

    /// Attempt to parse from a lower-hex string.
    pub fn from_hex<S: AsRef<[u8]> + ?Sized>(string: &S) -> Result<Self, Error> {
        Ok(PrivateKey(hex_decode(string.as_ref())?))
    }

    /// Encode as a lower-hex string.
    pub fn to_hex(self) -> String {
        String::from_utf8(self.to_hex_bytes().to_vec()).unwrap()
    }

    /// Encode as a lower-hex string as an in-place ascii char array.
    pub fn to_hex_bytes(self) -> [u8; 64] {
        hex_encode(self.to_bytes())
    }

    /// Encode as a lower-hex string into the provided buf.
    pub fn encode_hex_buf(self, buf: &mut String) {
        buf.push_str(str::from_utf8(&self.to_hex_bytes()).unwrap());
    }

    /// Encode as a lower-hex string into the provided `fmt::Write`.
    pub fn encode_hex_fmt<W: fmt::Write>(self, w: &mut W) -> Result<(), fmt::Error> {
        w.write_str(str::from_utf8(&self.to_hex_bytes()).unwrap())
    }

    /// Encode as a lower-hex string into the provided `io::Write`.
    pub fn encode_hex_io<W: io::Write>(self, w: &mut W) -> Result<(), io::Error> {
        w.write_all(&self.to_hex_bytes())
    }

    /// Wrap with a type which formats as hex rather than redacting the content.
    pub fn to_hex_fmt(self) -> impl Debug + Display {
        struct D(PrivateKey);
        impl Debug for D {
            fn fmt(&self, f: &mut Formatter) -> fmt::Result {
                f.write_str(str::from_utf8(&self.0.to_hex_bytes()).unwrap())
            }
        }
        impl Display for D {
            fn fmt(&self, f: &mut Formatter) -> fmt::Result {
                f.write_str(str::from_utf8(&self.0.to_hex_bytes()).unwrap())
            }
        }
        D(self)
    }
}

impl KeyPair {
    /// Construct from public and private keys.
    ///
    /// Errors if they don't match.
    pub fn new(public: PublicKey, private: PrivateKey) -> Result<Self, Error> {
        ensure!(
            private.to_public_key() == public,
            "public and private keys within keypair do not match"
        );
        Ok(KeyPair { public, private })
    }

    /// Use the OS random number generator to randomly generate a key pair.
    pub fn generate() -> Self {
        let private = PrivateKey::generate();
        let public = private.to_public_key();
        KeyPair { public, private }
    }

    /// Get the public key.
    pub fn public_key(self) -> PublicKey {
        self.public
    }

    /// Get the private key.
    pub fn private_key(self) -> PrivateKey {
        self.private
    }

    /// Serialize as a json object.
    pub fn to_json(self, pretty: bool) -> String {
        if pretty {
            serde_json::to_string_pretty(&self)
        } else {
            serde_json::to_string(&self)
        }
        .unwrap()
    }

    /// Deserialize from a json object.
    pub fn from_json<S: AsRef<[u8]> + ?Sized>(json_string: &S) -> Result<Self, Error> {
        Ok(serde_json::from_slice(json_string.as_ref())?)
    }

    /// Read and deserialize from a json file at the given path.
    pub fn read_from_file<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        Self::read_from_reader(File::open(path)?)
    }

    /// Read and deserialize as json from the given `io::Read`.
    pub fn read_from_reader<R: Read>(mut reader: R) -> Result<Self, Error> {
        // sanity check / dos protection by only allowing up to 1 KiB
        let mut buf = [0; 1024];
        let mut size = 0;
        loop {
            size += match reader.read(&mut buf[size..])? {
                0 => break,
                n => n,
            };
            ensure!(size < buf.len(), "keypair file is unreasonably large");
        }
        Ok(Self::from_json(&buf[..size])?)
    }

    /// Serialize and write to a json file at the given path non-destructively.
    ///
    /// What "non-destructively" means is that, if a file already exists at that path, this will
    /// read the file's content and checks whether it already contains this key pair. If it does,
    /// this function simply returns ok, and otherwise returns an error.
    pub fn write_to_file_safe<P: AsRef<Path>>(self, path: P) -> Result<(), Error> {
        let mut file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .append(false)
            .truncate(false)
            .open(path)?;
        if file.metadata()?.len() == 0 {
            // whatever, it should be fine to consider empty files the same as non-existent
            serde_json::to_writer_pretty(&mut file, &self)?;
        } else {
            ensure!(
                Self::read_from_reader(&mut file)? == self,
                "keypair file already exists and contains different key than the in-memory one"
            );
        }
        Ok(())
    }

    /// Serialize and write to a json file at the given path, overwriting if one already exists.
    pub fn write_to_file_overwrite<P: AsRef<Path>>(self, path: P) -> Result<(), Error> {
        serde_json::to_writer(File::create(path)?, &self)?;
        Ok(())
    }

    /// Find a key pair file within the directory by file name and read and deserialize it.
    ///
    /// This expects key files to be named with the format `qkai-[prefix of public key].secret`.
    /// This function searches the direct children of `dir`, looking for a key file which's public
    /// key begins with `prefix`, and attempts to read it.
    ///
    /// Calling with `dir` = `"."` and `prefix` = `""` will simply try to read any key pair in
    /// current directory.
    pub fn find_in_dir_and_read<P: AsRef<Path>>(dir: P, prefix: &str) -> Result<Self, Error> {
        fn try_entry(
            entry: Result<DirEntry, io::Error>,
            prefix: &str,
        ) -> Result<Option<KeyPair>, Error> {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                return Ok(None);
            }
            let file_name = entry.file_name();
            let file_name = match file_name.to_str() {
                Some(file_name) => file_name,
                None => return Ok(None),
            };
            let file_name_prefix = match file_name
                .strip_prefix("qkai-")
                .and_then(|s| s.strip_suffix(".secret"))
            {
                Some(s) => s,
                None => return Ok(None),
            };
            let common_len = usize::min(file_name_prefix.len(), prefix.len());
            if file_name_prefix.as_bytes()[..common_len] != prefix.as_bytes()[..common_len] {
                return Ok(None);
            }
            let key_pair = KeyPair::read_from_file(entry.path()).map_err(|e| dbg!(e))?;
            if key_pair
                .public_key()
                .to_base64_bytes()
                .starts_with(prefix.as_bytes())
            {
                Ok(Some(key_pair))
            } else {
                Ok(None)
            }
        }
        for entry in read_dir(dir)? {
            match try_entry(entry, prefix) {
                Ok(Some(yay_we_did_it)) => return Ok(yay_we_did_it),
                Ok(None) => (),
                Err(e) => warn!(%e, "error in find_in_dir_and_read"),
            };
        }
        bail!("keypair not found");
    }

    /// Serialize and write to a json file in the given directory, automatically choosing the name.
    ///
    /// The file will be named `qkai-[first 8 characters of public key].secret`. This is designed
    /// for compatibility with `find_in_dir_and_read`.
    ///
    /// In terms of its behavior if the file already exists, this works the same as
    /// `write_to_file_safe`.
    pub fn write_to_file_in_dir<P: AsRef<Path>>(self, dir: P) -> Result<(), Error> {
        let base64_bytes = self.public_key().to_base64_bytes();
        let prefix = str::from_utf8(&base64_bytes[..8]).unwrap();
        self.write_to_file_safe(dir.as_ref().join(format!("qkai-{}.secret", prefix)))
    }
}

impl Debug for PublicKey {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        self.encode_base64_fmt(f)
    }
}

impl Display for PublicKey {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        self.encode_base64_fmt(f)
    }
}

impl Debug for PrivateKey {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.write_str("PrivateKey(..)")
    }
}

impl FromStr for PublicKey {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_base64(s)
    }
}

impl FromStr for PrivateKey {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_hex(s)
    }
}

fn serialize<const N: usize, F: Fn([u8; 32]) -> [u8; N], S: Serializer>(
    encode: F,
    bytes: [u8; 32],
    s: S,
) -> Result<S::Ok, S::Error> {
    if s.is_human_readable() {
        let utf8 = encode(bytes);
        s.serialize_str(str::from_utf8(&utf8).unwrap())
    } else {
        s.serialize_bytes(&bytes)
    }
}

fn deserialize<'d, F: Fn(&[u8]) -> Result<[u8; 32], Error>, D: Deserializer<'d>>(
    decode: F,
    d: D,
) -> Result<[u8; 32], D::Error> {
    struct V<F>(F);
    impl<'d, F: Fn(&[u8]) -> Result<[u8; 32], Error>> serde::de::Visitor<'d> for V<F> {
        type Value = [u8; 32];

        fn expecting(&self, f: &mut Formatter) -> fmt::Result {
            f.write_str("ed25519 key (base64 or binary)")
        }

        fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
            (self.0)(v.as_bytes()).map_err(E::custom)
        }

        fn visit_bytes<E: serde::de::Error>(self, v: &[u8]) -> Result<Self::Value, E> {
            if v.len() == 32 {
                let mut buf = [0; 32];
                buf.copy_from_slice(v);
                Ok(buf)
            } else {
                Err(E::custom(
                    "deserializing ed25519 binary key, wrong number of bytes",
                ))
            }
        }
    }
    if d.is_human_readable() {
        d.deserialize_str(V(decode))
    } else {
        d.deserialize_bytes(V(decode))
    }
}

impl Serialize for PublicKey {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        serialize(base64_encode, self.to_bytes(), s)
    }
}

impl<'d> Deserialize<'d> for PublicKey {
    fn deserialize<D: Deserializer<'d>>(d: D) -> Result<Self, D::Error> {
        deserialize(base64_decode, d).map(Self::from_bytes)
    }
}

impl Serialize for PrivateKey {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        serialize(hex_encode, self.to_bytes(), s)
    }
}

impl<'d> Deserialize<'d> for PrivateKey {
    fn deserialize<D: Deserializer<'d>>(d: D) -> Result<Self, D::Error> {
        deserialize(hex_decode, d).map(Self::from_bytes)
    }
}

impl<'d> Deserialize<'d> for KeyPair {
    fn deserialize<D: Deserializer<'d>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct KeyPairUnvalidated {
            public: PublicKey,
            private: PrivateKey,
        }
        KeyPairUnvalidated::deserialize(d).and_then(|key_pair| {
            Self::new(key_pair.public, key_pair.private).map_err(D::Error::custom)
        })
    }
}

// implement KeyPair comparison operations by delegating to the public key to avoid timing attacks

impl Eq for KeyPair {}

impl PartialEq for KeyPair {
    fn eq(&self, rhs: &Self) -> bool {
        self.public == rhs.public
    }
}

impl Ord for KeyPair {
    fn cmp(&self, rhs: &Self) -> Ordering {
        self.public.cmp(&rhs.public)
    }
}

impl PartialOrd for KeyPair {
    fn partial_cmp(&self, rhs: &Self) -> Option<Ordering> {
        Some(self.cmp(rhs))
    }
}

impl Hash for KeyPair {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.public.hash(h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foobar() {
        println!("hello world");
        let keys = KeyPair::generate();
        println!("{:#?}", keys);
        assert_eq!(
            PublicKey::from_base64(&keys.public_key().to_base64_bytes()).unwrap(),
            keys.public_key(),
        );
        assert_eq!(
            PrivateKey::from_hex(&keys.private_key().to_hex_bytes())
                .unwrap()
                .to_bytes(),
            keys.private_key().to_bytes(),
        );
        //keys.write_to_file_in_dir(".").unwrap();
        let keys2 = KeyPair::find_in_dir_and_read(".", "").unwrap();
        println!("{}", keys.to_json(true));
    }
}
