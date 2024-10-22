//! Segment queue (not concurrent itself)

use std::{
    mem::{size_of, transmute_copy},
    ptr::{NonNull, drop_in_place},
    marker::PhantomData,
    alloc::{Layout, alloc, dealloc, handle_alloc_error},
};

/// target segment capacity in bytes
///
/// note: this MUST be LESS THAN u16::MAX
const CAP_BYTES: usize = 1024;

/// segment capacity in elements
///
/// note: this MUST be LESS THAN u16::MAX
const fn cap<T>() -> usize {
    if size_of::<T>() == 0 {
        // edge case
        CAP_BYTES
    } else {
        let n = CAP_BYTES / size_of::<T>();
        if n < 1 { 1 } else { n }
    }
}

/// content of a segment other than the elements
struct SegMeta<T> {
    /// next segment towards back
    to_back: Option<SegPtr<T>>,
    /// next segment towards front,
    to_front: Option<SegPtr<T>>,
    /// index of the element at the front of this segment
    start: u16,
    /// number of elements in this segment
    len: u16,
}

/// non-null pointer to a heap allocated segment
///
/// the heap allocated segment is composed to a `SegMeta<T>`, then necessary padding, then
/// MaybeUninit<[T; cap::<T>()]>. due to current limitations of const expressions, we need to
/// use this wrapper.
struct SegPtr<T>(NonNull<u8>, PhantomData<T>);

/// compute segment layout and offset of elements within segment
///
/// we hope this gets optimized away into basically-a-constant
fn seg_layout<T>() -> (Layout, usize) {
    let layout_meta = Layout::new::<SegMeta<T>>();
    let layout_elems = Layout::array::<T>(cap::<T>()).unwrap();
    layout_meta.extend(layout_elems).unwrap()
}

impl<T> SegPtr<T> {
    /// allocate on the heap and initialize as empty
    unsafe fn alloc() -> Self {
        let (layout, _) = seg_layout::<T>();
        if let Some(ptr) = NonNull::new(alloc(layout)) {
            (ptr.as_ptr() as *mut SegMeta<T>)
                .write(SegMeta { to_back: None, to_front: None, start: 0, len: 0 });
            SegPtr(ptr, PhantomData)
        } else {
            handle_alloc_error(layout);
        }
    }

    /// whether the segment is empty
    unsafe fn is_empty(self) -> bool {
        (&*(self.0.as_ptr() as *const SegMeta<T>)).len == 0
    }

    /// whether the segment is full
    unsafe fn is_full(self) -> bool {
        (&*(self.0.as_ptr() as *const SegMeta<T>)).len == cap::<T>() as u16
    }

    /// push element to back. assumes it's not full.
    unsafe fn push(self, t: T) {
        debug_assert!(!self.is_full());
        let meta = &mut *(self.0.as_ptr() as *mut SegMeta<T>);
        let idx = (meta.start as usize + meta.len as usize) % cap::<T>();
        let (_, offset) = seg_layout::<T>();
        (self.0.as_ptr().add(offset) as *mut T).add(idx).write(t);
        meta.len += 1;
    }

    /// pop element from front. assumes it's not empty.
    unsafe fn pop(self) -> T {
        debug_assert!(!self.is_empty());
        let meta = &mut *(self.0.as_ptr() as *mut SegMeta<T>);
        let (_, offset) = seg_layout::<T>();
        let t = (self.0.as_ptr().add(offset) as *mut T).add(meta.start as usize).read();
        meta.start = (meta.start + 1) % cap::<T>() as u16;
        meta.len -= 1;
        t
    }

    /// deallocate segment and drop elements
    unsafe fn drop(self) {
        let meta = &mut *(self.0.as_ptr() as *mut SegMeta<T>);
        let (layout, offset) = seg_layout::<T>();
        for j in 0..meta.len as usize {
            let idx = (meta.start as usize + j) % cap::<T>() as usize;
            drop_in_place((self.0.as_ptr().add(offset) as *mut T).add(idx));
        }
        dealloc(self.0.as_ptr(), layout);
    }

    /// get mutable reference to segment's ptr to next segment towards back
    unsafe fn to_back(&self) -> &mut Option<SegPtr<T>> {
        &mut (&mut *(self.0.as_ptr() as *mut SegMeta<T>)).to_back
    }

    /// get mutable reference to segment's ptr to next segment towards back
    unsafe fn to_front(&self) -> &mut Option<SegPtr<T>> {
        &mut (&mut *(self.0.as_ptr() as *mut SegMeta<T>)).to_front
    }
}


impl<T> Clone for SegPtr<T> {
    fn clone(&self) -> Self {
        SegPtr(self.0, self.1)
    }
}

impl<T> Copy for SegPtr<T> {}

/// Segment queue of `T`
pub struct SegQueue<T> {
    /// total length of seg queue
    len: usize,
    /// front and back segments, unless no segments are linked
    /// invariant: no linked segments are empty
    front_back: Option<(SegPtr<T>, SegPtr<T>)>,
    /// basically a pool of empty segments to pull from before allocating a new one, except
    /// that the pool's maximum size is 1. this prevents repeated allocations if the queue
    /// length is fluctuating within a range of `cap::<T>()`.
    spare: Option<SegPtr<T>>,
}

impl<T> SegQueue<T> {
    /// Construct empty
    pub fn new() -> Self {
        SegQueue { len: 0, front_back: None, spare: None }
    }

    /// Elements in queue
    pub fn len(&self) -> usize {
        self.len
    }

    /// Push to back
    pub fn push(&mut self, t: T) {
        unsafe {
            self.len += 1;
            if size_of::<T>() == 0 { return; }

            if let Some((_, back)) = self.front_back.filter(|(_, back)| !back.is_full()) {
                back.push(t);
            } else {
                let new_back = self.spare.take().unwrap_or_else(|| SegPtr::alloc());
                debug_assert!(new_back.is_empty());
                debug_assert!(new_back.to_back().is_none());
                debug_assert!(new_back.to_front().is_none());
                new_back.push(t);
                if let &mut Some((_, ref mut back)) = &mut self.front_back {
                    debug_assert!(back.to_back().is_none());
                    *back.to_back() = Some(new_back);
                    *new_back.to_front() = Some(*back);
                    *back = new_back;
                } else {
                    self.front_back = Some((new_back, new_back));
                }
            }
        }
    }

    /// Pop from front
    pub fn pop(&mut self) -> Option<T> {
        unsafe {
            if self.len() == 0 { return None; }
            self.len -= 1;
            // hopefully this is fine
            if size_of::<T>() == 0 { return Some(transmute_copy(&())); }

            let (front, _) = self.front_back.unwrap();
            let t = front.pop();

            if front.is_empty() {
                // un-link
                if let Some(new_front) = front.to_back() {
                    *new_front.to_front() = None;
                    self.front_back.as_mut().unwrap().0 = *new_front;
                } else {
                    self.front_back = None;
                }

                // stash or drop
                if self.spare.is_none() {
                    *front.to_back() = None;
                    self.spare = Some(front);
                } else {
                    front.drop();
                }
            }

            Some(t)
        }
    }
}

impl<T> Drop for SegQueue<T> {
    fn drop(&mut self) {
        unsafe {
            if let Some(spare) = self.spare {
                spare.drop();
            }

            let mut next = self.front_back.map(|(front, _)| front);
            while let Some(curr) = next {
                next = *curr.to_back();
                curr.drop();
            }
        }
    }
}
