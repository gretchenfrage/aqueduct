// segment queue part of a channel.

use std::{
    mem::{self, size_of},
    ptr::{NonNull, drop_in_place},
    marker::PhantomData,
    alloc::{Layout, alloc, dealloc, handle_alloc_error},
};


// compute segment capacity in elems.
//
// guaranteed to be strictly less than u16::MAX.
const fn cap<T>() -> usize {
    elem_size_to_cap(size_of::<T>())
}

// compute segment capacity in elems, given the byte size of an elem.
//
// guaranteed to be strictly less than u16::MAX.
const fn elem_size_to_cap(elem_size: usize) -> usize {
    // "target" byte capacity of a segment
    const IDEAL_CAP_BYTES: usize = 1024;

    if elem_size == 0 {
        // edge case: ZST
        //
        // in this case, this value doesn't matter, because the `SegQueue` has a special case to
        // not even allocate segments. thus, we choose a value which would cause a runtime error
        // to help surface bugs.
        usize::MAX
    } else {
        let n = IDEAL_CAP_BYTES / elem_size;
        if n < 1 {
            // edge case: elem larger than ideal segment capacity
            1
        } else {
            n
        }
    }
}

// compute segment layout and offset of elem array within segment.
//
// we hope this gets optimized away into basically-a-constant.
fn seg_layout<T>() -> (Layout, usize) {
    let layout_meta = Layout::new::<SegMeta<T>>();
    let layout_elems = Layout::array::<T>(cap::<T>()).unwrap();
    layout_meta.extend(layout_elems).unwrap()
}

// non-null pointer to a heap allocated segment
//
// the layout of the heap allocation is basically that of a struct containing:
//
// - `SegMeta<T>`
// - `MaybeUninit<[T; cap::<T>()]>`
//
// due to current limitations of const expressions, we need to use this wrapper.
struct SegPtr<T>(NonNull<u8>, PhantomData<T>);

// content of a segment other than the elements
struct SegMeta<T> {
    // next segment towards back
    to_back: Option<SegPtr<T>>,
    // next segment towards front
    to_front: Option<SegPtr<T>>,
    // if len > 0, front is elems[start]
    // invariant: start < N
    start: u16,
    // if len > 0, back is elems[(start + len - 1) % N]
    len: u16,
}

impl<T> SegPtr<T> {
    /// allocate on the heap and initialize as empty
    unsafe fn alloc() -> Self {
        let (layout, _) = seg_layout::<T>();
        let Some(ptr) = NonNull::new(alloc(layout)) else { handle_alloc_error(layout) };
        let meta = ptr.as_ptr() as *mut SegMeta<T>;
        meta.write(SegMeta { to_back: None, to_front: None, start: 0, len: 0 });
        SegPtr(ptr, PhantomData)
    }

    /// whether the segment is empty
    unsafe fn is_empty(self) -> bool {
        (&*(self.0.as_ptr() as *const SegMeta<T>)).len == 0
    }

    /// whether the segment is full
    unsafe fn is_full(self) -> bool {
        (&*(self.0.as_ptr() as *const SegMeta<T>)).len == cap::<T>() as u16
    }

    /// push element to back. assumes it's not full, or UB occurs.
    unsafe fn push(self, t: T) {
        debug_assert!(!self.is_full());
        let meta = &mut *(self.0.as_ptr() as *mut SegMeta<T>);
        let idx = (meta.start as usize + meta.len as usize) % cap::<T>();
        let (_, offset) = seg_layout::<T>();
        (self.0.as_ptr().add(offset) as *mut T).add(idx).write(t);
        meta.len += 1;
    }

    /// pop element from front. assumes it's not empty, or UB occurs.
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

    /// get mutable reference to segment's ptr to next segment towards front
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
            if size_of::<T>() == 0 {
                // ZST special case
                return;
            }

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
            if size_of::<T>() == 0 {
                // ZST special case
                //
                // this is fine, because:
                //
                // - T is a ZST, so any pointer constructed to it naturally would just be some
                //   garbage pointer, and actually reading where it points physically would be UB.
                //
                // - the length was greater than 0, so at some point an instance of T was passed
                //   to `push`, so T is not an uninhabited type and does have a possible value.
                //
                // https://discord.com/channels/273534239310479360/592856094527848449/1314741277392244736
                //
                // replace with conjure_zst() if that function ever gets stabilized.
                return Some(mem::zeroed());
            }

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

unsafe impl<T: Send> Send for SegQueue<T> {}
unsafe impl<T: Sync> Sync for SegQueue<T> {}


#[cfg(test)]
mod tests {
    use super::*;
    use rand::prelude::*;
    use rand_pcg::Pcg32;

    fn new_rng() -> impl Rng {
        Pcg32::from_seed(0xdeadbeefdeadbeefdeadbeefdeadbeefu128.to_le_bytes())
    }

    fn dbg_elem<const ELEM_SIZE: usize>(elem: [u8; ELEM_SIZE]) -> u32 {
        use std::cmp::min;

        let mut n_bytes = [0; 4];
        let useable_len = min(ELEM_SIZE, 4);
        (&mut n_bytes[..useable_len]).copy_from_slice(&elem[..useable_len]);
        u32::from_ne_bytes(n_bytes)
    }

    fn elem_size_test<const ELEM_SIZE: usize>() {
        use std::{
            collections::VecDeque,
            cmp::min,
        };
        
        let mut rng = new_rng();

        for outer in 0..100 {
            let mut queue_1 = VecDeque::<[u8; ELEM_SIZE]>::new();
            let mut queue_2 = SegQueue::<[u8; ELEM_SIZE]>::new();
            println!("outer loop {}", outer);
            for i in 0u32..10_000 {
                if rng.gen_ratio(52, 100) {
                    println!("PUSH {}", i);
                    let mut elem = [0; ELEM_SIZE];
                    let useable_len = min(ELEM_SIZE, 4);
                    (&mut elem[..useable_len]).copy_from_slice(&i.to_ne_bytes()[..useable_len]);
                    queue_1.push_back(elem);
                    queue_2.push(elem);
                } else {
                    let expect = queue_1.pop_front();
                    let expect_dbg = expect.map(dbg_elem);
                    println!("POP {:?}", expect_dbg);
                    assert_eq!(queue_2.pop(), expect);
                }
                // assert equivalent
                assert_eq!(queue_1.len(), queue_2.len());
                unsafe {
                    if ELEM_SIZE == 0 {
                        assert!(queue_2.front_back.is_none());
                        assert!(queue_2.spare.is_none());
                        continue;
                    }

                    let mut prev_seg_ptr: Option<SegPtr<[u8; ELEM_SIZE]>> = None;
                    let mut next_seg_ptr = queue_2.front_back.map(|(front, _)| front);
                    let mut next_seg_idx = 0;
                    for (_i, &elem_1) in queue_1.iter().enumerate() {
                        // assertions
                        let seg_ptr = next_seg_ptr.unwrap();
                        assert!(!seg_ptr.is_empty());
                        let (_, offset) = seg_layout::<[u8; ELEM_SIZE]>();
                        let cap = elem_size_to_cap(ELEM_SIZE);
                        assert!(cap < u16::MAX.into());
                        let meta = &*(seg_ptr.0.as_ptr() as *const SegMeta<[u8; ELEM_SIZE]>);
                        assert!(meta.len > 0);
                        assert!((meta.start as usize) < cap);
                        let elems_ptr = seg_ptr.0.as_ptr().add(offset) as *const [u8; ELEM_SIZE];
                        let elem_physical_idx = (meta.start as usize + next_seg_idx) % cap;
                        let elem_ptr = elems_ptr.add(elem_physical_idx);
                        let elem_2 = elem_ptr.read();
                        assert_eq!(dbg_elem(elem_1), dbg_elem(elem_2));
                        assert_eq!(seg_ptr.to_front().map(|p| p.0), prev_seg_ptr.map(|p| p.0));
                        if seg_ptr.to_back().is_some() && seg_ptr.to_front().is_some() {
                            // only the first and last segment may be not completely full
                            assert_eq!(meta.len as usize, cap);
                        }

                        // advance
                        next_seg_idx += 1;
                        assert!(next_seg_idx <= meta.len as usize);
                        if next_seg_idx == meta.len as usize {
                            next_seg_idx = 0;
                            prev_seg_ptr = next_seg_ptr;
                            next_seg_ptr = meta.to_back;
                        }
                    }
                    assert!(next_seg_ptr.is_none());
                }
            }
        }
    }

    macro_rules! equivalence_size_tests {
        ($($t:ident $n:expr,)*)=>{
            mod equivalence_size_tests {
                use super::*;

                $(
                    #[test]
                    fn $t() {
                        elem_size_test::<$n>();
                    }
                )*
            }
        };
    }

    equivalence_size_tests!(
        // 0 and 1
        _0 0,
        _1 1,

        // powers of 2
        _2 2,
        _4 4,
        _8 8,
        _16 16,
        _32 32,
        _64 64,
        _128 128,
        _256 256,
        _512 512,
        _1024 1024,
        _2048 2048,
        _4096 4096,
        _8192 8192,
        _16384 16384,
        _32768 32768,
        _65536 65536,
        _131072 131072,

        // neighborhood of u16::MAX
        _65534 65534,
        _65535 65535,
        _65537 65537,

        // powers of 3
        _3 3,
        _9 9,
        _27 27,
        _81 81,
        _243 243,
        _729 729,
        _2187 2187,
        _6561 6561,
        _19683 19683,
        _59049 59049,
    );
}
