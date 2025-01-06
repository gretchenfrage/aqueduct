
use std::mem::needs_drop;


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
