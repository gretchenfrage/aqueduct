
use std::{
    cell::Cell,
    collections::BTreeMap,
    iter,
};


/// Collection of integer ranges stored as a not-necessarily sorted vector.
#[derive(Default, Debug)]
pub struct VecAckRanges(Vec<(u64, u64)>);

impl VecAckRanges {
    /// Add a single integer as a range. Does not detect overlaps.
    ///
    /// Unspecified behavior occurs if `n` equals `u64::MAX`.
    ///
    /// O(1).
    pub fn add_point(&mut self, n: u64) {
        debug_assert!(n < u64::MAX);

        if let Some(&mut (_, ref mut end)) = self.0
            .iter_mut().rev().next()
            .filter(|&&mut (_, end)| end + 1 == n)
        {
            // this "expand range" case is just a memory optimization
            *end = n;
        } else {
            self.0.push((n, n));
        }
    }

    /// Sort ranges and produce a draining iterator over them which merges contiguous ranges,
    /// detects overlaps as it scans, and eventually leaves `self` in a default state.
    ///
    /// O(n log n).
    pub fn sort_merge_drain<'a>(&'a mut self) -> impl Iterator<Item=Result<(u64, u64), ()>> + 'a {
        // this is expected to probably be mostly sorted, wherein unstable-sort would be slower
        self.0.sort_by_key(|&(start, _)| start);
        let mut sorted = self.0.drain(..).peekable();
        iter::from_fn(move || -> Option<Result<(u64, u64), ()>> {
            let (a, mut b) = sorted.next()?;
            debug_assert!(b >= a);
            debug_assert!(a < u64::MAX);
            debug_assert!(b < u64::MAX);
            while let Some((a2, b2)) = sorted.next_if(|&(a2, _)| a2 <= b + 1) {
                debug_assert!(b2 >= a2);
                debug_assert!(a2 < u64::MAX);
                debug_assert!(b2 < u64::MAX);
                if a2 <= b {
                    return Some(Err(()));
                }
                debug_assert!(a2 == b + 1);
                b = b2;
            }
            Some(Ok((a, b)))
        })
    }
}


/// Collection of integer ranges stored as a b-tree.
#[derive(Default, Debug)]
pub struct TreeAckRanges(BTreeMap<Cell<u64>, u64>);

impl TreeAckRanges {
    /// Add an integer range. Error on overlap.
    ///
    /// Unspecific behavior occurs if the range overlaps `u64::max` or if the range has a
    /// non-positive length.
    ///
    /// O(log n).
    pub fn add_range(&mut self, a: u64, b: u64) -> Result<(), ()> {
        debug_assert!(b >= a);
        debug_assert!(b < u64::MAX);
        let mut range = self.0.range_mut(..=Cell::new(b + 1));
        if let Some((a2, b2)) = range.next_back() {
            debug_assert!(*b2 >= a2.get());
            debug_assert!(a2.get() < u64::MAX);
            debug_assert!(*b2 < u64::MAX);
            if a2.get() == b + 1 {
                // [a, b] merges with (at least) existing [a2, b2] on its right
                if let Some((a3, b3)) = range.next_back() {
                    debug_assert!(*b3 >= a3.get());
                    debug_assert!(a3.get() < u64::MAX);
                    debug_assert!(*b3 < u64::MAX);
                    debug_assert!(*b3 + 1 < a2.get());
                    if *b3 >= a {
                        // [a, b] intersects with existing [a3, b3]
                        return Err(());
                    } else if *b3 + 1 == a {
                        // [a, b] merges with existing [a3, b3] on its left, in addition to [a2, b2]
                        // 1. modify tree entry [a3, b3] -> [a3, b2]
                        // 2. delete tree entry [a2, b2]
                        *b3 = *b2;
                        let a2 = a2.clone();
                        self.0.remove(&a2);
                    } else {
                        // [a, b] only merges with [a2, b2]
                        // - modify tree entry [a2, b2] -> [a, b2]
                        a2.set(a);
                    }
                } else {
                    // [a, b] only merges with [a2, b2]
                    // - modify tree entry [a2, b2] -> [a, b2]
                    a2.set(a);
                }
            } else {
                if *b2 >= a {
                    // [a, b] intersects with existing [a2, b2]
                    return Err(());
                } else if *b2 + 1 == a {
                    // [a, b] merges with (only) existing [a2, b2] on its left
                    // - modify tree entry [a2, b2] -> [a2, b]
                    *b2 = b;
                } else {
                    // [a, b] does not merge with anything
                    // - insert tree entry [a, b]
                    self.0.insert(Cell::new(a), b);
                }
            }
        } else {
            // [a, b] does not merge with anything
            // - insert tree entry [a, b]
            self.0.insert(Cell::new(a), b);
        }
        Ok(())
    }

    /// Get the first integer that is not within a range. 
    ///
    /// O(1).
    pub fn first_unacked_point(&self) -> u64 {
        self.0.iter()
            .next()
            .filter(|&(ref a, _)| a.get() == 0)
            .map(|(_, &b)| b + 1)
            .unwrap_or_default()
    }
}
