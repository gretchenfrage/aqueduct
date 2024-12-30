
use std::iter::once;
use thiserror::Error;
use bytes::{Bytes, BytesMut};
use smallvec::SmallVec;

const MULTI_BYTES_INLINE: usize = 2;

/// Sequence of bytes stored as an array of [`bytes::Bytes`].
///
/// If there are 2 or fewer segments, the pointers to them are stored inline, without requiring an
/// additional allocation. Empty segments are automatically filtered out.
#[derive(Debug, Clone, Default)]
pub struct MultiBytes {
    // empty pages are alwasys filtered out
    pages: SmallVec<[Bytes; MULTI_BYTES_INLINE]>,
    // offset page idx
    // handles to pages before the offset are cleared so their memory can be reclaimed
    offset_page: usize,
    // offset byte index into offset page
    offset_bytes: usize,
    // total byte length starting at offset
    len: usize,
}

impl MultiBytes {
    /// Total number of bytes contained.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Copy the next `buf.len()` bytes to `buf` and advance `self` past them.
    ///
    /// Errors if not that many bytes remain.
    pub fn read(&mut self, mut buf: &mut [u8]) -> Result<(), TooFewBytesError> {
        self.len = self.len.checked_sub(buf.len()).ok_or(TooFewBytesError)?;
        loop {
            let page_rem = &self.pages[self.offset_page][self.offset_bytes..];

            if buf.len() >= page_rem.len() {
                let (buf1, buf2) = buf.split_at_mut(page_rem.len());
                buf1.copy_from_slice(page_rem);
                self.pages[self.offset_page].clear();
                self.offset_page += 1;
                self.offset_bytes = 0;
                buf = buf2;
            } else {
                buf.copy_from_slice(&page_rem[..buf.len()]);
                self.offset_bytes += buf.len();
                break;
            }
        }
        Ok(())
    }

    /// Construct a [`MultiBytes`] from the next `n` bytes in a zero-copy fashion and advance
    /// `self` past them.
    ///
    /// Errors if not that many bytes remain.
    pub fn read_zc(&mut self, mut n: usize) -> Result<MultiBytes, TooFewBytesError> {
        self.len = self.len.checked_sub(n).ok_or(TooFewBytesError)?;
        let mut out = MultiBytes::default();
        // you may wonder, why do we `loop` in `read` but add this redundant `n > 0` check here?
        // it's because slicing an array is free, but `.slice`ing a `Bytes` isn't quite.
        while n > 0 {
            let mut page_rem = self.pages[self.offset_page].slice(self.offset_bytes..);

            if n >= page_rem.len() {
                n -= page_rem.len();
                out.extend(once(page_rem));
                self.pages[self.offset_page].clear();
                self.offset_page += 1;
                self.offset_bytes = 0;
            } else {
                page_rem.truncate(n);
                out.extend(once(page_rem));
                self.offset_bytes += n;
                break;
            }
        }
        Ok(out)
    }

    /// Advance `self` past the next `n` bytes without copying them to anywhere.
    ///
    /// Errors if not that many bytes remain.
    pub fn skip(&mut self, mut n: usize) -> Result<(), TooFewBytesError> {
        self.len = self.len.checked_sub(n).ok_or(TooFewBytesError)?;
        loop {
            let page_rem = self.pages[self.offset_page].len() - self.offset_bytes;

            if n >= page_rem {
                n -= page_rem;
                self.pages[self.offset_page].clear();
                self.offset_page += 1;
                self.offset_bytes = 0;
            } else {
                self.offset_bytes += n;
                break;
            }
        }
        Ok(())
    }

    /// Iterate over pages.
    ///
    /// It is strongly recommended not to ascribe any particular meaning to page boundaries. They
    /// are considered an optimizations and may be automatically mangled in various cases.
    pub fn iter_pages(&self) -> impl Iterator<Item=&Bytes> {
        todo!();
        self.pages.iter()
    }

    /// Convert into an iterator over pages.
    ///
    /// It is strongly recommended not to ascribe any particular meaning to page boundaries. They
    /// are considered an optimizations and may be automatically mangled in various cases.
    pub fn into_pages(self) -> impl Iterator<Item=Bytes> {
        todo!();
        self.pages.into_iter()
    }

    /// Convert into a single [`Bytes`], copying as necessary.
    pub fn defragment(self) -> Bytes {
        let mut out = BytesMut::new();
        for page in self.pages {
            if out.is_empty() && page.is_unique() {
                out = page.try_into_mut().ok().unwrap();
            } else {
                if out.is_empty() {
                    out.reserve(self.len);
                }
                out.extend(page);
            }
        }
        out.into()
    }

    // TODO: should be made public somehow
    pub(crate) fn pages_mut(&mut self) -> &mut [Bytes] {
        &mut self.pages
    }
}

// TODO: explicit transitive implementations of From<String>, etc

impl From<Bytes> for MultiBytes {
    fn from(bytes: Bytes) -> Self {
        once(bytes).collect()
    }
}

macro_rules! transitive_from {
    ($($t:ty),*)=>{$(
        impl From<$t> for MultiBytes {
            fn from(t: $t) -> Self {
                MultiBytes::from(Bytes::from(t))
            }
        }
    )*};
}

transitive_from!(&'static [u8], &'static str, Box<[u8]>, String, Vec<u8>);

impl FromIterator<Bytes> for MultiBytes {
    fn from_iter<I: IntoIterator<Item = Bytes>>(pages: I) -> Self {
        let mut len = 0;
        let pages = pages.into_iter()
            .filter(|page| !page.is_empty())
            .inspect(|page| len += page.len())
            .collect();
        MultiBytes {
            pages,
            offset_page: 0,
            offset_bytes: 0,
            len,
        }
    }
}

impl Extend<Bytes> for MultiBytes {
    fn extend<I: IntoIterator<Item = Bytes>>(&mut self, pages: I) {
        self.pages.extend(pages.into_iter()
            .filter(|page| !page.is_empty())
            .inspect(|page| self.len += page.len()));
    }
}

impl PartialEq<[u8]> for MultiBytes {
    fn eq(&self, mut rhs: &[u8]) -> bool {
        if self.len() != rhs.len() {
            return false;
        }
        for page in &self.pages {
            if *page != rhs[..page.len()] {
                return false;
            }
            rhs = &rhs[page.len()..];
        }
        true
    }
}

impl PartialEq<MultiBytes> for MultiBytes {
    fn eq(&self, rhs: &MultiBytes) -> bool {
        if self.len() != rhs.len() {
            return false;
        }
        let mut pages_1 = self.iter_pages().peekable();
        let mut offset_1 = 0;
        let mut pages_2 = rhs.iter_pages().peekable();
        let mut offset_2 = 0;
        while pages_1.peek().is_some() {
            let page_1 = pages_1.peek().unwrap();
            let page_2 = pages_2.peek().unwrap();
            let page_1_len = page_1.len() - offset_1;
            let page_2_len = page_2.len() - offset_2;
            let common_len = page_1_len.min(page_2_len);
            if page_1[offset_1..][..common_len] != page_2[offset_2..][..common_len] {
                return false;
            }

            if page_1_len > common_len {
                offset_1 += page_1_len;
            } else {
                offset_1 = 0;
                pages_1.next().unwrap();
            }

            if page_2_len > common_len {
                offset_2 += page_2_len;
            } else {
                offset_2 = 0;
                pages_2.next().unwrap();
            }
        }
        debug_assert!(pages_2.peek().is_none());
        true
    }
}

/// Error type for trying to read more bytes than remain in a collection of bytes.
#[derive(Error, Debug, Copy, Clone)]
#[error("too few bytes")]
pub struct TooFewBytesError;
