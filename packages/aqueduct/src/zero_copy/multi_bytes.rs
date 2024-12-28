
use std::{
    iter::{Peekable, once},
    io::{self, Read},
    slice,
};
use thiserror::Error;
use bytes::Bytes;
use smallvec::SmallVec;

const MULTI_BYTES_INLINE: usize = 2;

/// Sequence of bytes stored as an array of [`bytes::Bytes`].
///
/// If there are 2 or fewer segments, the pointers to them are stored inline, without requiring an
/// additional allocation. Empty segments are automatically filtered out.
#[derive(Debug, Clone, Default)]
pub struct MultiBytes {
    pages: SmallVec<[Bytes; MULTI_BYTES_INLINE]>,
    // total length
    len: usize,
}

impl MultiBytes {
    /// Total number of bytes contained.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Get a cursor for scanning and reading through these bytes.
    pub fn cursor(&self) -> Cursor<'_> {
        Cursor {
            inner: self,
            offset: 0,
            page_iter: self.pages.iter().peekable(),
            page_offset: 0,
        }
    }
    
    // TODO: these ought to be public
    pub(crate) fn into_chunks(self) -> impl Iterator<Item=Bytes> {
        self.pages.into_iter()
    }

    pub(crate) fn chunks_mut(&mut self) -> &mut [Bytes] {
        &mut self.pages
    }
}

// TODO: explicit transitive implementations of From<String>, etc

impl From<Bytes> for MultiBytes {
    fn from(bytes: Bytes) -> Self {
        once(bytes).collect()
    }
}

impl FromIterator<Bytes> for MultiBytes {
    fn from_iter<I: IntoIterator<Item = Bytes>>(pages: I) -> Self {
        let mut len = 0;
        let pages = pages.into_iter()
            .filter(|page| !page.is_empty())
            .inspect(|page| len += page.len())
            .collect();
        MultiBytes { pages, len }
    }
}

impl Extend<Bytes> for MultiBytes {
    fn extend<I: IntoIterator<Item = Bytes>>(&mut self, pages: I) {
        self.pages.extend(pages.into_iter()
            .filter(|page| !page.is_empty())
            .inspect(|page| self.len += page.len()));
    }
}

// TODO: eq impls with MultiBytes and contiguous bufs


/// Cursor for scanning and reading through a [`MultiBytes`].
#[derive(Debug, Clone)]
pub struct Cursor<'a> {
    inner: &'a MultiBytes,
    offset: usize,
    page_iter: Peekable<slice::Iter<'a, Bytes>>,
    // offset into page_iter's next page
    page_offset: usize,
}

impl<'a> Cursor<'a> {
    /// How many bytes have already been advanced past.
    ///
    /// Begins at 0.
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// How many bytes may still be advanced past before running out of bytes.
    ///
    /// Begins at `multi_bytes.len()`.
    pub fn remaining_len(&self) -> usize {
        self.inner.len() - self.offset
    }

    /// Copy the next `buf.len()` bytes to `buf` and advance past them.
    ///
    /// Errors if not that many bytes remain.
    pub fn read(&mut self, mut buf: &mut [u8]) -> Result<(), TooFewBytesError> {
        if buf.len() > self.remaining_len() {
            return Err(TooFewBytesError);
        }
        self.offset += buf.len();
        loop {
            let page_rem = &self.page_iter.peek().unwrap()[self.page_offset..];

            if buf.len() >= page_rem.len() {
                let (buf1, buf2) = buf.split_at_mut(page_rem.len());
                buf1.copy_from_slice(page_rem);
                self.page_iter.next().unwrap();
                self.page_offset = 0;
                buf = buf2;
            } else {
                buf.copy_from_slice(&page_rem[..buf.len()]);
                self.page_offset += buf.len();
                break;
            }
        }
        Ok(())
    }

    /// Convenience method to [`read`](Self::read) a single byte.
    pub fn read_byte(&mut self) -> Result<u8, TooFewBytesError> {
        let mut buf = [0];
        self.read(&mut buf)?;
        let [b] = buf;
        Ok(b)
    }

    /// Construct a [`MultiBytes`] from the next `n` bytes in a zero-copy fashion and advance past
    /// them.
    ///
    /// Errors if not that many bytes remain.
    pub fn read_zc(&mut self, mut n: usize) -> Result<MultiBytes, TooFewBytesError> {
        if n > self.remaining_len() {
            return Err(TooFewBytesError);
        }
        self.offset += n;
        let mut out = MultiBytes::default();
        // you may wonder, why do we `loop` in `read` but add this redundant `n > 0` check here?
        // it's because slicing an array is free, but `.slice`ing a `Bytes` isn't quite.
        while n > 0 {
            let mut page_rem = self.page_iter.peek().unwrap().slice(self.page_offset..);

            if n >= page_rem.len() {
                n -= page_rem.len();
                out.extend(once(page_rem));
                self.page_iter.next().unwrap();
                self.page_offset = 0;
            } else {
                page_rem.truncate(n);
                out.extend(once(page_rem));
                self.page_offset += n;
                break;
            }
        }
        Ok(out)
    }

    /// Advance past the next `n` bytes without copying them to anywhere.
    ///
    /// Errors if not that many bytes remain.
    pub fn skip(&mut self, mut n: usize) -> Result<(), TooFewBytesError> {
        if n > self.remaining_len() {
            return Err(TooFewBytesError);
        }
        self.offset += n;
        loop {
            let page_rem = self.page_iter.peek().unwrap().len() - self.page_offset;

            if n >= page_rem {
                n -= page_rem;
                self.page_iter.next().unwrap();
                self.page_offset = 0;
            } else {
                self.page_offset += n;
                break;
            }
        }
        Ok(())
    }
}

/// Error type for trying to read more bytes than remain in a collection of bytes.
#[derive(Error, Debug, Copy, Clone)]
#[error("too few bytes")]
pub struct TooFewBytesError;

/// Convenience implementation for compatibility.
impl<'a> Read for Cursor<'a> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = buf.len().min(self.remaining_len());
        self.read(&mut buf[..n]).unwrap();
        Ok(n)
    }
}
