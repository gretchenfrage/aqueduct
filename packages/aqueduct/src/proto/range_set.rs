use std::{cell::Cell, collections::BTreeMap};

// set of u64 with memory O(N log N) to number of contiguous ranges present
//
// internally, map from non-overlapping non-empty non-contiguous ranges' start to end (inclusive).
#[derive(Default)]
pub struct RangeSetU64(BTreeMap<Cell<u64>, u64>);

impl RangeSetU64 {
    // insert a new [n, m] range, returning whether any of it was already present
    pub fn insert(&mut self, n: u64, mut m: u64) -> bool {
        assert!(m >= n);
        let mut intersected = false;
        loop {
            let mut ranges = self
                .0
                .range_mut(..=Cell::new(m.saturating_add(1)))
                .rev()
                .take_while(|&(_, ref e)| **e >= m.saturating_sub(1));
            if let Some((s, e)) = ranges.next() {
                if *e >= n || s.get() <= m {
                    intersected = true;
                }
                if ranges.next().is_some() {
                    m = m.max(*e);
                    let s = s.clone();
                    self.0.remove(&s);
                } else {
                    s.set(n);
                    *e = m;
                    break;
                }
            } else {
                self.0.insert(Cell::new(n), m);
                break;
            }
        }
        intersected
    }

    // get whether self contains n
    pub fn contains(&self, n: u64) -> bool {
        self.0
            .range(..=Cell::new(n))
            .next_back()
            .is_some_and(|(_, &e)| e >= n)
    }

    // get lowest u64 that is not contained within self
    // returns None iff self contains [0, u64::MAX]
    pub fn lowest_absent(&self) -> Option<u64> {
        self.0
            .iter()
            .next()
            .filter(|&(ref start, _)| start.get() == 0)
            .map(|(_, &end)| end.checked_add(1))
            .unwrap_or(Some(0))
    }

    // iterate through ranges
    pub fn iter(&self) -> impl Iterator<Item = (u64, u64)> {
        self.0.iter().map(|(k, &v)| (k.get(), v))
    }

    // remove all content
    pub fn clear(&mut self) {
        self.0.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty();
    }
}

// safety: RangeSetU64 is naturally !Sync because of Cell keys, but any writes to them are guarded
//         by a &mut reference to the RangeSetU64.
unsafe impl Sync for RangeSetU64 {}
