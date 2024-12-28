
use crate::zero_copy::MultiBytes;
use std::{
    iter::once,
    mem::take,
    io::{self, Write},
};
use bytes::Bytes;


// size that the buf starts at without any doublings.
const START_CAPACITY: usize = 64;

// if write_zc is called with a Bytes smaller than this, it will just be copied in in a non-zc way.
const MIN_ZC_BYTES: usize = 64;


/// Utility for writing bytes in memory build a [`MultiBytes`].
///
/// Uses heuristics to minimize both copying and fragmentation without requiring the writer to know
/// ahead of time how many bytes will be written, and also allows [`Bytes`] to be appended in a
/// zero-copy fashion.
#[derive(Debug, Clone, Default)]
pub struct MultiBytesWriter {
    // the algorithm is this: `inner` stores a vector of `Bytes` pages, which cannot be changed
    // once inserted. `buf` is a buffer for bytes at the end that are being copied in. if `buf`
    // would have to exceed its current capacity, rather than copying its existing contents to a
    // doubled allocation, we add the existing allocation as a page and make a new allocation with
    // doubled size but which starts with no content.
    // 
    // the pushing of a `Bytes` directly from the user resets this "doublings" counter.
    inner: MultiBytes,
    buf: Vec<u8>,
    doublings: u8,
}

impl MultiBytesWriter {
    /// Extend the byte collection by copying in `bytes`.
    pub fn write(&mut self, mut bytes: &[u8]) {
        while bytes.len() > 0 {
            if self.buf.capacity() == 0 {
                while START_CAPACITY << self.doublings < bytes.len() {
                    self.doublings += 1;
                }
                self.buf.reserve(START_CAPACITY << self.doublings);
            }

            let buf_rem = self.buf.capacity() - self.buf.len();

            if bytes.len() >= buf_rem {
                self.buf.extend(bytes[..buf_rem].iter().copied());
                self.inner.extend(once(Bytes::from(take(&mut self.buf))));
                self.doublings += 1;
                bytes = &bytes[buf_rem..];
            } else {
                self.buf.extend(bytes.iter().copied());
                break;
            }
        }
    }

    /// Extend the byte collection by appending `bytes` in a zero-copy fashion.
    pub fn write_zc<B: Into<MultiBytes>>(&mut self, bytes: B) {
        let bytes = bytes.into();
        if bytes.len() < MIN_ZC_BYTES {
            for chunk in bytes.into_chunks() {
                // TODO: make this mesh better with capacity reservation
                self.write(&chunk);
            }
        } else {
            for chunk in bytes.into_chunks() {
                self.inner.extend(once(Bytes::from(take(&mut self.buf))));
                self.doublings = 0;
                self.inner.extend(once(chunk));
            }
        }
    }

    /// Construct the final resultant [`MultiBytes`].
    pub fn build(mut self) -> MultiBytes {
        self.inner.extend(once(Bytes::from(take(&mut self.buf))));
        self.inner
    }

    /// Number of bytes written so far.
    pub fn len(&self) -> usize {
        self.inner.len() + self.buf.len()
    }
}

/// Convenience implementation for compatibility.
impl Write for MultiBytesWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.write(buf);
        Ok(buf.len())
    }
    
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
