//! Queue that can store a small number of elements inline.

use std::{
    mem::MaybeUninit,
    ptr::drop_in_place,
    ops::{Index, IndexMut},
    fmt::{self, Formatter, Debug},
};


// heap allocation size to make in elements upon first heap allocating
const INITIAL_ALLOC_LEN: usize = 16;


/// Queue that can store a small number of elements inline.
pub struct SmallQueue<T, const N: usize> {
    // memory layout:
    //
    // - parts 1 and 2 concatenated together form the space of "storage indices"
    // - a "logical index" (exposed to the user) exists if it's in [0, len)
    // - a logical index N maps to the storage index (start + N) % (max storage index)
    // - storage locations are assumed to be initialized iff a logical index maps to them
    start: usize,
    len: usize,
    part_1: [MaybeUninit<T>; N],
    // we don't need to use Option because we rely on stdlib to fake the heap allocation of ZSTs
    part_2: Box<[MaybeUninit<T>]>,
}

impl<T, const N: usize> SmallQueue<T, N> {
    /// Construct empty.
    pub fn new() -> Self {
        Self::with_alloc_cap(0)
    }

    /// Construct empty with a given capacity of the heap allocated part. 
    pub fn with_alloc_cap(alloc_cap: usize) -> Self {
        SmallQueue {
            start: 0,
            len: 0,
            part_1: [const { MaybeUninit::uninit() }; N],
            part_2: Box::new_uninit_slice(alloc_cap),
        }
    }

    /// Current length in elements.
    pub fn len(&self) -> usize {
        self.len
    }

    // currently allocated capacity, including both in-place and heap parts.
    fn cap(&self) -> usize {
        N + self.part_2.len()
    }

    // convert from logical index to storage index, or panic on out-of-bounds.
    fn storage_idx(&self, idx: usize) -> usize {
        debug_assert!(self.len() <= self.cap(), "SmallQueue len > cap (internal bug)");
        assert!(idx < self.len(), "SmallQueue index out of bounds");
        (self.start + idx) % self.cap()
    }

    // get raw pointer by logical index, or panic on out-of-bounds.
    fn pointer(&self, idx: usize) -> *const T {
        let storage_idx = self.storage_idx(idx);
        if storage_idx < N {
            self.part_1[storage_idx].as_ptr()
        } else {
            self.part_2[storage_idx].as_ptr()
        }
    }

    // get raw pointer by logical index (mutably), or panic on out-of-bounds.
    fn pointer_mut(&mut self, idx: usize) -> *mut T {
        let storage_idx = self.storage_idx(idx);
        if storage_idx < N {
            self.part_1[storage_idx].as_mut_ptr()
        } else {
            self.part_2[storage_idx].as_mut_ptr()
        }
    }

    /// Push to back of queue.
    pub fn push_back(&mut self, elem: T) {
        // maybe upsize
        if self.len() == self.cap() {
            // decide upsized size
            let new_alloc_cap = (self.part_2.len() * 2).max(INITIAL_ALLOC_LEN);
            // allocate
            let mut new_self = Self::with_alloc_cap(new_alloc_cap);
            new_self.len = self.len();
            // copy elements
            for i in 0..self.len() {
                unsafe { new_self.pointer_mut(i).write(self.pointer(i).read()); }
            }
            // mark old elements as uninitialized to prevent their destructors running
            self.len = 0;
            // drop old self's allocations and replace with new self
            *self = new_self;
        }

        // add element
        let idx = self.len;
        self.len += 1;
        unsafe { self.pointer_mut(idx).write(elem); }
    }

    /// Pop from front of queue.
    pub fn pop_front(&mut self) -> Option<T> {
        // short-circuit if empty
        if self.len() == 0 { return None; }

        // take element, mark as no longer initialized / updated indexes
        let elem = unsafe { self.pointer(0).read() };
        self.len -= 1;
        self.start += 1;

        // done
        Some(elem)
    }

    /// Create iterator from front to back by reference.
    pub fn iter(&self) -> impl Iterator<Item=&T> {
        (0..self.len()).map(move |i| &self[i])
    }
}

impl<T, const N: usize> Drop for SmallQueue<T, N> {
    fn drop(&mut self) {
        // drop initialized elements
        for i in 0..self.len() {
            unsafe { drop_in_place(self.pointer_mut(i)); }
        }
    }
}

impl<T: Clone, const N: usize> Clone for SmallQueue<T, N> {
    fn clone(&self) -> Self {
        let mut cloned = SmallQueue::with_alloc_cap(self.len().saturating_sub(N));
        for i in 0..self.len() {
            cloned.push_back(self[i].clone());
        }
        cloned
    }
}

impl<T, const N: usize> Index<usize> for SmallQueue<T, N> {
    type Output = T;

    fn index(&self, idx: usize) -> &T {
        // safety: `pointer` does bounds checking
        unsafe { &*self.pointer(idx) }
    }
}

impl<T, const N: usize> IndexMut<usize> for SmallQueue<T, N> {
    fn index_mut(&mut self, idx: usize) -> &mut T {
        // safety: `pointer` does bounds checking
        unsafe { &mut *self.pointer_mut(idx) }
    }
}

impl<T, const N: usize> Default for SmallQueue<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Debug, const N: usize> Debug for SmallQueue<T, N> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let mut f = f.debug_list();
        for i in 0..self.len() {
            f.entry(&self[i]);
        }
        f.finish()
    }
}

impl<T, const N: usize> Iterator for SmallQueue<T, N> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        self.pop_front()
    }
}
