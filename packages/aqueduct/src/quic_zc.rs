///! Internal utilities for bridging between our MultiBytes types and QUIC.

use multibytes::MultiBytes;
use bytes::Bytes;


const MAX_CHUNK_LENGTH: usize = 16384;


// ==== error types ====


/// Result type for QUIC zero-copy reading.
pub type Result<T> = std::result::Result<T, ReadError>;

/// Error type for QUIC zero-copy reading.
#[derive(Debug)]
pub enum ReadError {
    /// Expected more bytes.
    TooFewBytes,
    /// Other QUIC error.
    Quic(quinn::ReadError),
}

impl From<quinn::ReadError> for ReadError {
    fn from(e: quinn::ReadError) -> Self {
        ReadError::Quic(e)
    }
}


// ==== QUIC stream reader ====


/// Wrapper around a [`quinn::RecvStream`] which provides a `MultiBytes`-like API for zero-copy.
#[derive(Debug)]
pub struct QuicStreamReader {
    stream: quinn::RecvStream,
    chunk: Bytes,
}

impl QuicStreamReader {
    /// Wrap around a QUIC stream.
    pub fn new(stream: quinn::RecvStream) -> Self {
        QuicStreamReader {
            stream,
            chunk: Bytes::new(),
        }
    }

    /// Read the next `buf.len()` bytes from the stream and copy them to buf.
    pub async fn read(&mut self, mut buf: &mut [u8]) -> Result<()> {
        while !buf.is_empty() {
            // maybe read chunk
            if self.chunk.is_empty() {
                self.chunk = self.stream
                    .read_chunk(MAX_CHUNK_LENGTH, true).await?
                    .ok_or(ReadError::TooFewBytes)?
                    .bytes;
                debug_assert!(!self.chunk.is_empty());
            }

            // copy
            let common_len = buf.len().min(self.chunk.len());
            let copy_src = self.chunk.split_to(common_len);
            let (buf1, buf2) = buf.split_at_mut(common_len);
            buf1.copy_from_slice(&copy_src);
            buf = buf2;
        }
        Ok(())
    }

    /// Read the next `n` bytes in a zero-copy fashion and return them as a [`MultiBytes`].
    pub async fn read_zc(&mut self, mut n: usize) -> Result<MultiBytes> {
        let mut out = MultiBytes::default();
        while n > 0 {
            // maybe read chunk
            if self.chunk.is_empty() {
                self.chunk = self.stream
                    .read_chunk(MAX_CHUNK_LENGTH, true).await?
                    .ok_or(ReadError::TooFewBytes)?
                    .bytes;
                debug_assert!(!self.chunk.is_empty());
            }

            // transfer
            let common_len = n.min(self.chunk.len());
            out.write_zc(self.chunk.split_to(common_len));
            n -= common_len;
        }
        Ok(out)
    }

    /// Determine whether the stream finishes gracefully after the bytes that have been taken.
    pub async fn is_done(&mut self) -> Result<bool> {
        Ok(if self.chunk.is_empty() {
            if let Some(chunk) = self.stream.read_chunk(MAX_CHUNK_LENGTH, true).await? {
                self.chunk = chunk.bytes;
                false
            } else {
                true
            }
        } else {
            false
        })
    }
}
