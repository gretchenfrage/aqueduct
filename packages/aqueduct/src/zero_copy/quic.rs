// internal utilities for bridging between our MultiBytes types and QUIC

use crate::zero_copy::MultiBytes;
use std::iter::once;
use quinn::RecvStream;
use bytes::Bytes;
use anyhow::*;


const MAX_CHUNK_LENGTH: usize = 16384;

// wrapper around a quinn `RecvStream` which provides a `MultiBytes` cursor-like API
#[derive(Debug)]
pub struct QuicMultiBytesReader {
    stream: RecvStream,
    chunk: Bytes,
    chunk_offset: usize,
}

impl QuicMultiBytesReader {
    // construct around a QUIC stream
    pub fn new(stream: RecvStream) -> Self {
        QuicMultiBytesReader {
            stream,
            chunk: Bytes::new(),
            chunk_offset: 0,
        }
    }
    
    // read the next buf.len() bytes from the stream and copy them to buf
    pub async fn read(&mut self, mut buf: &mut [u8]) -> Result<(), Error> {
        while !buf.is_empty() {
            if self.chunk_offset == self.chunk.len() {
                self.chunk = self.stream
                    .read_chunk(MAX_CHUNK_LENGTH, true).await
                    .context("QUIC error")?
                    .ok_or(Error::msg("expected more bytes in QUIC stream"))?
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
    pub async fn read_zc(&mut self, mut n: usize) -> Result<MultiBytes, Error> {
        let mut out = MultiBytes::default();
        while n > 0 {
            if self.chunk_offset == self.chunk.len() {
                self.chunk = self.stream
                    .read_chunk(MAX_CHUNK_LENGTH, true).await
                    .context("QUIC error")?
                    .ok_or(Error::msg("expected more bytes in QUIC stream"))?
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
    pub async fn is_done(&mut self) -> Result<bool, Error> {
        while self.chunk_offset == self.chunk.len() {
            if let Some(chunk) = self.stream
                .read_chunk(MAX_CHUNK_LENGTH, true).await
                .context("QUIC error")?
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
    
    // read a single u8
    pub async fn read_u8(&mut self) -> Result<u8, Error> {
        let mut buf = [0];
        self.read(&mut buf).await?;
        let [b] = buf;
        Ok(b)
    }
    
    // read a variable-length encoded u64
    pub async fn read_var_len_u64(&mut self) -> Result<u64, Error> {
        const MASK: u8 = 0b01111111;
        const MORE: u8 = 0b10000000;
        
        let mut out: u64 = 0;
        let mut shift = 0;
        loop {
            ensure!(shift < 64, "too many bytes in var len u64");
            let b = self.read_u8().await?;
            out |= ((b & MASK) as u64) << shift;
            shift += 7;
            if (b & MORE) == 0 { break; }
        }
        Ok(out)
    }
    
    // read a variable-length encoded u64, then checked-convert it to `usize`
    pub async fn read_var_len_usize(&mut self) -> Result<usize, Error> {
        Ok(self.read_var_len_u64().await?
            .try_into()
            .map_err(|_| anyhow!("encoded u64 cannot fit into usize"))?)
    }
}
