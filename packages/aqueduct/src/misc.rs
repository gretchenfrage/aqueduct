
use std::{
    mem::{MaybeUninit, needs_drop},
    sync::atomic::{AtomicBool, Ordering},
};


// remove the first n elements of vec
pub(crate) fn remove_first<T>(vec: &mut Vec<T>, n: usize) {
    assert!(n <= vec.len(), "remove_first n too large");

    if n == 0 {
        return;
    } else if n == vec.len() {
        vec.clear();
        return;
    }

    unsafe {
        let old_len = vec.len();
        let elems = vec.as_mut_ptr();

        if needs_drop::<T>() {
            for i in 0..n {
                elems.add(i).drop_in_place();
            }
        }

        for i in n..old_len {
            elems.add(i - n).write(elems.add(i).read());
        }

        vec.set_len(old_len - n);
    }
}


// like an atomic Option<T> that can be `take`n once.
pub(crate) struct AtomicTake<T> {
    is_some: AtomicBool,
    val: MaybeUninit<T>,
}

impl<T> AtomicTake<T> {
    pub(crate) const fn some(val: T) -> Self {
        AtomicTake {
            is_some: AtomicBool::new(true),
            val: MaybeUninit::new(val),
        }
    }

    pub(crate) const fn none() -> Self {
        AtomicTake {
            is_some: AtomicBool::new(false),
            val: MaybeUninit::uninit()
        }
    }

    pub(crate) fn take(&self) -> Option<T> {
        unsafe {
            if self.is_some.swap(false, Ordering::Relaxed) {
                Some(self.val.as_ptr().read())
            } else {
                None
            }
        }
    }
}

impl<T> Drop for AtomicTake<T> {
    fn drop(&mut self) {
        drop(self.take());
    }
}
