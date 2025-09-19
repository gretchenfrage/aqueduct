//! Sequence of bytes stored as an array of [`bytes::Bytes`].

pub extern crate bytes;

mod small_queue;

use crate::small_queue::SmallQueue;
use bytes::{Bytes, BytesMut};
use thiserror::Error;

// this many Bytes may be stored without allocating a heap allocated array of Bytes
const IN_PLACE_FRAGMENTS: usize = 2;
// size that newly allocated Bytes starts at without any doublings.
const START_CAPACITY: usize = 64;
// if write_zc is called with a Bytes smaller than this, it will just be copied in in a non-zc way.
const MIN_ZC_BYTES: usize = 64;

/// Sequence of bytes stored as an array of [`bytes::Bytes`].
///
/// If there are 2 or fewer fragments, the pointers to them are stored inline, without requiring an
/// additional allocation. Empty fragments are automatically filtered out.
#[derive(Debug, Clone, Default)]
pub struct MultiBytes {
    // invariant: we must never insert an empty Bytes
    fragments: SmallQueue<Bytes, IN_PLACE_FRAGMENTS>,
    // total length
    len: usize,
    // doublings starts at 0. every time we allocate a new heap allocation-backed Bytes to store
    // non-zc-copied bytes, we increment this. every time we store new zc-copied bytes, we reset
    // this to 0.
    doublings: u8,
}

impl MultiBytes {
    /// Construct empty.
    pub fn new() -> Self {
        Self::default()
    }

    /// Total number of bytes contained.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Copy the first `buf.len()` bytes to `buf` and advance `self` past them.
    ///
    /// Errors if not that many bytes remain.
    pub fn read(&mut self, mut buf: &mut [u8]) -> Result<(), TooFewBytesError> {
        self.len = self.len.checked_sub(buf.len()).ok_or(TooFewBytesError)?;
        while buf.len() > 0 {
            let copy_src = if buf.len() < self.fragments[0].len() {
                // copy part of the next fragment
                self.fragments[0].split_to(buf.len())
            } else {
                // copy the entirety of the next fragment
                self.fragments.pop_front().unwrap()
            };
            let (buf1, buf2) = buf.split_at_mut(copy_src.len());
            buf1.copy_from_slice(&copy_src);
            buf = buf2;
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
        while n > 0 {
            let out_fragment = if n < self.fragments[0].len() {
                // add part of the next fragment
                self.fragments[0].split_to(n)
            } else {
                // add the entirety of the next fragment
                self.fragments.pop_front().unwrap()
            };
            n -= out_fragment.len();
            out.write_zc(out_fragment);
        }
        Ok(out)
    }

    /// Advance `self` past the next `n` bytes without copying them to anywhere.
    ///
    /// Errors if not that many bytes remain.
    pub fn skip(&mut self, mut n: usize) -> Result<(), TooFewBytesError> {
        self.len = self.len.checked_sub(n).ok_or(TooFewBytesError)?;

        while n > 0 {
            let skip_fragment = if n < self.fragments[0].len() {
                // skip part of the next fragment
                self.fragments[0].split_to(n)
            } else {
                // skip the entirety of the next fragment
                self.fragments.pop_front().unwrap()
            };
            n -= skip_fragment.len();
        }
        Ok(())
    }

    /// Extend the byte sequence by copying in `buf`.
    pub fn write(&mut self, mut buf: &[u8]) {
        self.len += buf.len();
        if buf.len() == 0 {
            return;
        }

        // try to write as much as possible into existing spare capacity
        if let Some(mut back) = self
            .fragments
            .len()
            .checked_sub(1)
            .and_then(|back_idx| self.fragments[back_idx].clone().try_into_mut().ok())
        {
            let copy_len = buf.len().min(back.capacity());
            let (buf1, buf2) = buf.split_at(copy_len);
            back.extend_from_slice(buf1);
            buf = buf2;
        }

        if buf.len() == 0 {
            return;
        }

        // manage the successive doublings of allocation size as bytes are copied in
        while START_CAPACITY << self.doublings < buf.len() {
            self.doublings += 1;
        }
        let new_alloc_cap = START_CAPACITY << self.doublings;
        self.doublings += 1;

        // add a new Bytes with additional spare capacity, and the rest of the buf copied in
        let mut new_alloc = BytesMut::new();
        new_alloc.reserve(new_alloc_cap);
        new_alloc.extend_from_slice(buf);
        self.fragments.push_back(new_alloc.into());
    }

    /// Extend the byte collection by appending `bytes` in a zero-copy fashion.
    pub fn write_zc(&mut self, bytes: impl Into<MultiBytes>) {
        let bytes = bytes.into();

        if bytes.len() < MIN_ZC_BYTES {
            // optimization to avoid excessive fragmentation
            for fragment in bytes.fragments() {
                self.write(&fragment);
            }
        } else {
            self.len += bytes.len();
            for fragment in bytes.fragments() {
                self.fragments.push_back(fragment);
            }
            self.doublings = 0;
        }
    }

    /// Convert into an iterator over fragments.
    ///
    /// It is strongly recommended not to ascribe any particular meaning to page boundaries. They
    /// are considered an optimizations and may be automatically mangled in various cases.
    pub fn fragments(self) -> impl Iterator<Item = Bytes> {
        self.fragments
    }

    /// Convert into a single [`Bytes`], copying as necessary.
    pub fn defragment(self) -> Bytes {
        let mut out = BytesMut::new();
        for fragment in self.fragments {
            if out.is_empty() && fragment.is_unique() {
                out = fragment.try_into_mut().ok().unwrap();
            } else {
                if out.is_empty() {
                    out.reserve(self.len);
                }
                out.extend(fragment);
            }
        }
        out.into()
    }
}

impl From<Bytes> for MultiBytes {
    fn from(bytes: Bytes) -> Self {
        let len = bytes.len();
        let mut fragments = SmallQueue::new();
        if !bytes.is_empty() {
            fragments.push_back(bytes);
        }
        MultiBytes {
            fragments,
            len,
            doublings: 0,
        }
    }
}

// implement `MultiBytes: From<T>` by composing `Bytes: From<T>` + `MultiBytes: From<Bytes>`
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

impl Extend<Bytes> for MultiBytes {
    fn extend<I: IntoIterator<Item = Bytes>>(&mut self, bytes_iter: I) {
        for bytes in bytes_iter {
            self.write_zc(bytes);
        }
    }
}

impl FromIterator<Bytes> for MultiBytes {
    fn from_iter<I: IntoIterator<Item = Bytes>>(bytes_iter: I) -> Self {
        let mut out = MultiBytes::new();
        out.extend(bytes_iter);
        out
    }
}

impl PartialEq<[u8]> for MultiBytes {
    fn eq(&self, mut rhs: &[u8]) -> bool {
        if self.len() != rhs.len() {
            return false;
        }
        for fragment in self.fragments.iter() {
            let (rhs1, rhs2) = rhs.split_at(fragment.len());
            if &*fragment != rhs1 {
                return false;
            }
            rhs = rhs2;
        }
        true
    }
}

impl PartialEq<MultiBytes> for MultiBytes {
    fn eq(&self, rhs: &MultiBytes) -> bool {
        if self.len() != rhs.len() {
            return false;
        }

        let mut fragments_1 = self.fragments.iter().peekable();
        let mut fragments_2 = rhs.fragments.iter().peekable();
        let mut offset_1 = 0;
        let mut offset_2 = 0;

        while fragments_1.peek().is_some() {
            // unwrap safety:
            // - we make sure there are never empty fragments
            // - there is another lhs fragment, implying there's more bytes
            // - we already short-circuited if different lengths
            let slice_1 = &fragments_1.peek().unwrap()[offset_1..];
            let slice_2 = &fragments_2.peek().unwrap()[offset_2..];
            let common_len = slice_1.len().min(slice_2.len());

            // compare
            if &slice_1[..common_len] != &slice_2[..common_len] {
                return false;
            }

            // advance
            for (slice_n, fragments_n, offset_n) in [
                (slice_1, &mut fragments_1, &mut offset_1),
                (slice_2, &mut fragments_2, &mut offset_2),
            ] {
                if slice_n.len() == common_len {
                    fragments_n.next().unwrap();
                    *offset_n = 0;
                } else {
                    *offset_n += common_len;
                }
            }
        }

        debug_assert!(fragments_1.next().is_none());
        debug_assert!(fragments_2.next().is_none());

        true
    }
}

/// Error type for trying to read more bytes than remain in a collection of bytes.
#[derive(Error, Debug, Copy, Clone)]
#[error("too few bytes")]
pub struct TooFewBytesError;
