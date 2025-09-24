use std::{cell::Cell, collections::BTreeMap};

// set of u64 with memory O(N log N) to number of contiguous ranges present
//
// internally, map from non-overlapping non-empty non-contiguous ranges' start to end (inclusive).
#[derive(Default)]
pub struct RangeSetU64(BTreeMap<Cell<u64>, u64>);

impl RangeSetU64 {
    // insert if not already present
    pub fn insert(&mut self, n: u64) {
        // get all ranges intersecting with [0, n + 1]
        let mut rangerange = self.0.range_mut(..=Cell::new(n.saturating_add(1)));
        if let Some((s1, e1)) = rangerange.next_back() {
            // there is at least 1 range intersecting with [0, n + 1]
            if n.checked_add(1).is_some_and(|nplus1| s1.get() == nplus1) {
                // s1 == n + 1; [n, n] is contiguous with [s1, e1] on n's right
                if let Some((s2, e2)) = rangerange.next_back() {
                    // there are at least 2 ranges intersecting with [0, n + 1]
                    debug_assert!(
                        s1.get()
                            .checked_sub(2)
                            .is_some_and(|s1minus2| *e2 <= s1minus2),
                        "invariant broken: [{}, {}] and [{}, {}] continguous but not merged",
                        s2.get(),
                        *e2,
                        s1.get(),
                        *e1
                    );
                    if *e2 == n - 1 {
                        // e2 == n - 1; [n, n] is (also) contiguous with [s2, e2] on n's left
                        // resolution: merge both [n, n] and [s1, e1] into [s2, e2]
                        *e2 = *e1;
                        let s1 = s1.clone();
                        self.0.remove(&s1);
                    } else {
                        // [n, n] is not contiguous with any range on n's left
                        // resolution: merge [n, n] into [s1, e1]
                        s1.set(n);
                    }
                } else {
                    // there is only 1 range intersecting with [0, n + 1]
                    // (that being [s1, e1], which [n, n] is contiguous with on n's right)
                    // therefore: [n, n] is not contiguous with any range on n's left
                    // resolution: merge [n, n] into [s1, e1]
                    s1.set(n);
                }
            } else if s1.get() <= n && *e1 >= n {
                // [n, n] is contained within [s1, e1]
                // resolution: there is nothing to do
            } else if n.checked_sub(1).is_some_and(|nminus1| *e1 == nminus1) {
                // e1 == n - 1; [n, n] is not contiguous with any range on its right but is
                //              contiguous with [s1, e1] on its right
                // resolution: merge [n, n] into [s1, e1]
                *e1 = n;
            } else {
                // [n, n] is not contiguous with any range on n's left or right
                // resolution: [n, n] becomes new range
                self.0.insert(Cell::new(n), n);
            }
        } else {
            // there are no ranges intersecting with [0, n + 1]
            // therefore: [n, n] is not contiguous with any range on n's left or right
            // resolution: [n, n] becomes new range
            self.0.insert(Cell::new(n), n);
        }
    }

    pub fn contains(&self, n: u64) -> bool {
        self.0
            .range(..=Cell::new(n))
            .next_back()
            .is_some_and(|(_, &e)| e >= n)
    }
}

// safety: RangeSetU64 is naturally !Sync because of Cell keys, but any writes to them are guarded
//         by a &mut reference to the RangeSetU64.
unsafe impl Sync for RangeSetU64 {}
