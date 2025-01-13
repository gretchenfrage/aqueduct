// internal utilities for bridging between our MultiBytes types and QUIC

use crate::zero_copy::MultiBytes;
use std::iter::once;
use bytes::Bytes;


const MAX_CHUNK_LENGTH: usize = 16384;

pub(crate) type Result<T> = std::result::Result<T, QuicStreamReadError>;

// wrapper around a quinn `RecvStream` which provides a `MultiBytes` cursor-like API
#[derive(Debug)]
pub(crate) struct QuicStreamReader {
    stream: quinn::RecvStream,
    chunk: Bytes,
    chunk_offset: usize,
}

impl QuicStreamReader {
    // construct around a QUIC stream
    pub(crate) fn new(stream: quinn::RecvStream) -> Self {
        QuicStreamReader {
            stream,
            chunk: Bytes::new(),
            chunk_offset: 0,
        }
    }

    // read the next buf.len() bytes from the stream and copy them to buf
    pub(crate) async fn read(&mut self, mut buf: &mut [u8]) -> Result<()> {
        while !buf.is_empty() {
            if self.chunk_offset == self.chunk.len() {
                self.chunk = self.stream
                    .read_chunk(MAX_CHUNK_LENGTH, true).await?
                    .ok_or(QuicStreamReadError::TooFewBytes)?
                    .bytes;
                self.chunk_offset = 0;
            }
            let chunk_rem = &self.chunk[self.chunk_offset..];
            if buf.len() >= chunk_rem.len() {
                let (buf1, buf2) = buf.split_at_mut(chunk_rem.len());
                buf1.copy_from_slice(chunk_rem);
                buf = buf2;
                self.chunk = Bytes::new();
                self.chunk_offset = 0;
            } else {
                buf.copy_from_slice(&chunk_rem[..buf.len()]);
                self.chunk_offset += chunk_rem.len();
                break;
            }
        }
        Ok(())
    }

    // read the next n bytes in a zero-copy fashion and return them as a MultiBytes
    pub(crate) async fn read_zc(&mut self, mut n: usize) -> Result<MultiBytes> {
        let mut out = MultiBytes::default();
        while n > 0 {
            if self.chunk_offset == self.chunk.len() {
                self.chunk = self.stream
                    .read_chunk(MAX_CHUNK_LENGTH, true).await?
                    .ok_or(QuicStreamReadError::TooFewBytes)?
                    .bytes;
                self.chunk_offset = 0;
            }
            let mut chunk_rem = self.chunk.slice(self.chunk_offset..);
            if n >= chunk_rem.len() {
                n -= chunk_rem.len();
                out.extend(once(chunk_rem));
                self.chunk = Bytes::new();
                self.chunk_offset = 0;
            } else {
                chunk_rem.truncate(n);
                out.extend(once(chunk_rem));
                self.chunk_offset += n;
                break;
            }
        }
        Ok(out)
    }

    // determine whether the stream finishes after the bytes that have been taken
    pub(crate) async fn is_done(&mut self) -> Result<bool> {
        while self.chunk_offset == self.chunk.len() {
            if let Some(chunk) = self.stream
                .read_chunk(MAX_CHUNK_LENGTH, true).await?
            {
                self.chunk = chunk.bytes;
                self.chunk_offset = 0;
            } else {
                self.chunk = Bytes::new();
                self.chunk_offset = 0;
                return Ok(true);
            }
        }
        Ok(false)
    }
}

// error for reading from a QUIC stream.
#[derive(Debug)]
pub(crate) enum QuicStreamReadError {
    // underlying QUIC error.
    Quic(quinn::ReadError),
    // expected more bytes.
    TooFewBytes,
}

impl From<quinn::ReadError> for QuicStreamReadError {
    fn from(e: quinn::ReadError) -> Self {
        QuicStreamReadError::Quic(e)
    }
}
