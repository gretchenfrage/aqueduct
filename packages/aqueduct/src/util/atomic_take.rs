//! Low-level concurrency utility.

use std::{
    mem::MaybeUninit,
    sync::atomic::{
        Ordering::Relaxed,
        AtomicBool,
    },
};

/// Like an atomic `Option<T>` that can be `take`n once.
pub(crate) struct AtomicTake<T> {
    // val is initialized if is_some is true. the thread that transitions it from true to false
    // claims the right to read it and take ownership of it.
    is_some: AtomicBool,
    val: MaybeUninit<T>,
}

impl<T> AtomicTake<T> {
    /// Construct with a value.
    pub const fn some(val: T) -> Self {
        AtomicTake {
            is_some: AtomicBool::new(true),
            val: MaybeUninit::new(val),
        }
    }

    /// Construct without a value.
    pub const fn none() -> Self {
        AtomicTake {
            is_some: AtomicBool::new(false),
            val: MaybeUninit::uninit()
        }
    }

    /// Try to atomically take the value.
    pub fn take(&self) -> Option<T> {
        if self.is_some.swap(false, Relaxed) {
            Some(unsafe { self.val.as_ptr().read() })
        } else {
            None
        }
    }
}

impl<T> Drop for AtomicTake<T> {
    fn drop(&mut self) {
        // make sure the value gets dropped if not yet taken
        drop(self.take());
    }
}
