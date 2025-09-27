use std::{cell::Cell, collections::BTreeMap};

// set of u64 with memory O(N log N) to number of contiguous ranges present
//
// internally, map from non-overlapping non-empty non-contiguous ranges' start to end (inclusive).
#[derive(Default)]
pub struct RangeSetU64(BTreeMap<UnsafeAssertSync<Cell<u64>>, u64>);

impl RangeSetU64 {
    // insert a new [n, m] range, returning whether any of it was already present
    pub fn insert(&mut self, n: u64, mut m: u64) -> bool {
        assert!(m >= n);
        let mut intersected = false;
        loop {
            let mut ranges = self
                .0
                .range_mut(..=UnsafeAssertSync(Cell::new(m.saturating_add(1))))
                .rev()
                .take_while(|&(_, ref e)| **e >= m.saturating_sub(1));
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

    pub fn delete_range_by_start(&mut self, start: u64) {
        self.0.remove(&UnsafeAssertSync(Cell::new(start))).unwrap();
    }

    pub fn delete_range_prefix(&mut self, old_start: u64, new_start: u64) {
        assert!(new_start > new_start);
        let (start, &end) = self
            .0
            .get_key_value(&UnsafeAssertSync(Cell::new(old_start)))
            .unwrap();
        assert!(new_start <= end);
        start.0.set(new_start);
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

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

// safety: RangeSetU64 is naturally !Sync because of Cell keys, but any writes to them are guarded
//         by a &mut reference to the RangeSetU64.
#[derive(Ord, PartialOrd, Eq, PartialEq, Copy, Clone)]
struct UnsafeAssertSync<T>(T);

unsafe impl<T> Sync for UnsafeAssertSync<T> {}
