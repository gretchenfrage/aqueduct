use std::{
    cell::Cell,
    collections::btree_map::{self, BTreeMap},
};

// TODO: I wonder if the arithmetic would be overall nicer if we made the upper bounds exclusive

// set of u64, all of which are strictly less than `u64::MAX`, represented as a sorted set of
// non-overlapping non-empty non-contiguous ranges.
#[derive(Default)]
pub struct RangeSetU64(BTreeMap<UnsafeAssertSync<Cell<u64>>, u64>);

impl RangeSetU64 {
    // insert a new [n, m] range, returning whether any of it was already present
    pub fn insert(&mut self, n: u64, mut m: u64) -> bool {
        assert!(m >= n);
        assert!(m < u64::MAX);

        let mut intersected = false;
        loop {
            let mut ranges = self
                .0
                .range_mut(..=UnsafeAssertSync(Cell::new(m + 1)))
                .rev()
                .take_while(|&(_, ref e)| **e + 1 >= m);
            if let Some((s, e)) = ranges.next() {
                if *e >= n || s.0.get() <= m {
                    intersected = true;
                }
                if ranges.next().is_some() {
                    m = m.max(*e);
                    let s = s.clone();
                    self.0.remove(&s);
                } else {
                    s.0.set(n);
                    *e = m;
                    break;
                }
            } else {
                self.0.insert(UnsafeAssertSync(Cell::new(n)), m);
                break;
            }
        }
        intersected
    }

    // get whether self contains n
    pub fn contains(&self, n: u64) -> bool {
        self.0
            .range(..=UnsafeAssertSync(Cell::new(n)))
            .next_back()
            .is_some_and(|(_, &e)| e >= n)
    }

    // iterate through ranges
    pub fn iter(&self) -> impl Iterator<Item = (u64, u64)> + Send {
        self.0.iter().map(|(k, &v)| (k.0.get(), v))
    }

    // remove all content
    pub fn clear(&mut self) {
        self.0.clear();
    }

    // get whether is empty
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    // get an entry for in-place manipulation of the first range
    pub fn first_range_entry(&mut self) -> Option<RangeEntry<'_>> {
        self.0.first_entry().map(RangeEntry)
    }
}

pub struct RangeEntry<'a>(btree_map::OccupiedEntry<'a, UnsafeAssertSync<Cell<u64>>, u64>);

impl<'a> RangeEntry<'a> {
    // get the start of this range
    pub fn start(&self) -> u64 {
        self.0.key().0.get()
    }

    // get the end of this range
    pub fn end(&self) -> u64 {
        *self.0.get()
    }

    // delete all elements of this range that are less than `n`, and return the number of elements
    // that were deleted, or panic if no elements were deleted
    pub fn delete_lt(self, n: u64) -> u64 {
        assert!(self.start() < n);
        let start = self.start();
        (if self.end() + 1 < n {
            self.0.key().0.set(n - 1);
            n
        } else {
            self.0.remove() + 1
        }) - start
    }
}

// safety: RangeSetU64 is naturally !Sync because of Cell keys, but any writes to them are guarded
//         by a &mut reference to the RangeSetU64.
#[derive(Ord, PartialOrd, Eq, PartialEq, Copy, Clone)]
struct UnsafeAssertSync<T>(T);

unsafe impl<T> Sync for UnsafeAssertSync<T> {}
