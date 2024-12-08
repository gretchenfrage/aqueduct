// linked nodes part of channel.

use std::{
    ptr::NonNull,
    task::Waker,
};


// intrusively linked list of nodes.
#[derive(Default)]
pub(crate) struct NodeQueue {
    // front and back of queue, unless queue is empty.
    front_back: Option<(NonNull<NodeAlloc>, NonNull<NodeAlloc>)>,
    // purging the queue is an irreversible operation wherein purged is set to true, all heap
    // allocations for nodes are dropped and deallocated, and any wakers they have are waked.
    purged: bool,
}

// handle to a node allocation. this handle "sort of" owns the allocation:
//
// - when not linked, ptr may be dereferenced.
// - when linked, ptr should only be dereferenced while holding a reference to the NodeQueue this
//   is linked into, and only if NodeQueue.purged is false.
// - if this node is linked into a queue when the queue is purged, the queue drops this allocation,
//   and this handle becomes permanently stuck in the linked state.
pub(crate) struct NodeHandle {
    // allocation for node.
    ptr: NonNull<NodeAlloc>,
    // whether node is currently linked.
    linked: bool,
}

// heap allocation for node.
#[derive(Default)]
struct NodeAlloc {
    // next node towards back.
    to_back: Option<NonNull<NodeAlloc>>,
    // next node towards front.
    to_front: Option<NonNull<NodeAlloc>>,
    // optional waker.
    waker: Option<Waker>,
}

impl NodeQueue {
    // construct empty queue.
    pub(crate) fn new() -> Self {
        NodeQueue::default()
    }

    // whether this queue is purged.
    pub(crate) fn is_purged(&self) -> bool {
        if self.purged {
            debug_assert!(self.front_back.is_none());
        }
        self.purged
    }

    // link the node to the back of this queue.
    //
    // UB if:
    //
    // - the node is already linked.
    // - the queue is purged.
    pub(crate) unsafe fn push(&mut self, node: &mut NodeHandle) {
        debug_assert!(!node.linked, "UB");
        debug_assert!(!self.purged, "UB");

        node.linked = true;
        let alloc = node.ptr.as_mut();
        debug_assert!(alloc.to_front.is_none());
        debug_assert!(alloc.to_back.is_none());
        debug_assert!(alloc.waker.is_none());
        if let &mut Some((_, ref mut back)) = &mut self.front_back {
            // node becomes new back, and new to_back of previous back
            let back_alloc = back.as_mut();
            debug_assert!(back_alloc.to_back.is_none());
            back_alloc.to_back = Some(node.ptr);
            alloc.to_front = Some(*back);
            *back = node.ptr;
        } else {
            // edge case: node becomes only node in queue
            self.front_back = Some((node.ptr, node.ptr));
        }
    }

    // unlink the node from this queue. also, clear its waker.
    //
    // UB if:
    //
    // - the node is not linked.
    // - the node is linked to a different queue.
    // - the queue is purged.
    pub(crate) unsafe fn remove(&mut self, node: &mut NodeHandle) {
        debug_assert!(node.linked, "UB");
        debug_assert!(!self.purged, "UB");

        node.linked = false;
        let alloc = node.ptr.as_mut();
        alloc.waker = None;
        if let &mut Some((ref mut front, ref mut back)) = &mut self.front_back {
            if let Some(mut to_front) = alloc.to_front {
                // node's to_back becomes new to_back of node's to_front
                let to_front_alloc = to_front.as_mut();
                debug_assert_eq!(to_front_alloc.to_back.unwrap(), node.ptr);
                to_front_alloc.to_back = alloc.to_back;
            } else {
                // edge case: node was at the front of queue (but is not the back)
                // node's to_back becomes new front
                *front = alloc.to_back.unwrap();
            }

            if let Some(mut to_back) = alloc.to_back {
                // node's to_front becomes new to_front of node's to_back
                let to_back_alloc = to_back.as_mut();
                debug_assert_eq!(to_back_alloc.to_front.unwrap(), node.ptr);
                to_back_alloc.to_front = alloc.to_front;
            } else {
                // edge case: node was at the back of queue (but is not the front)
                // node's to_front becomes new back
                *back = alloc.to_front.unwrap();
            }
        } else {
            // edge case: node was only node in queue
            debug_assert!(alloc.to_front.is_none());
            debug_assert!(alloc.to_back.is_none());
            self.front_back = None;
        }
        if cfg!(debug_assertions) {
            alloc.to_front = None;
            alloc.to_back = None;
        }
    }

    // whether the node is at the front of this queue.
    //
    // UB if:
    //
    // - the node is not linked.
    // - the node is linked to a different queue.
    // - the queue is purged.
    pub(crate) unsafe fn is_front(&self, node: &NodeHandle) -> bool {
        debug_assert!(node.linked, "UB");
        debug_assert!(!self.purged, "UB");

        let alloc = node.ptr.as_ref();
        if alloc.to_front.is_none() {
            debug_assert_eq!(self.front_back.unwrap().0, node.ptr);
        }
        alloc.to_front.is_none()
    }

    // borrow the node's waker.
    //
    // UB if:
    //
    // - the node is not linked.
    // - the node is linked to a different queue.
    // - the queue is purged.
    pub(crate) unsafe fn waker(&mut self, node: &mut NodeHandle) -> &mut Option<Waker> {
        debug_assert!(node.linked, "UB");
        debug_assert!(!self.purged, "UB");
        &mut node.ptr.as_mut().waker
    }

    // take and wake the waker of the node at the front of this queue, if this queue is not purged,
    // not empty, and the node at the front has a waker.
    pub(crate) fn wake_front(&mut self) {
        unsafe {
            if self.purged { return; }
            if let Some((mut front, _)) = self.front_back {
                if let Some(waker) = front.as_mut().waker.take() {
                    waker.wake();
                }
            }
        }
    }

    // purge the queue, if it is not already purged. this causes all wakers to be waked.
    pub(crate) fn purge(&mut self) {
        unsafe {
            if self.purged { return; }
            self.purged = true;
            let mut next = self.front_back.map(|(front, _)| front);
            while let Some(curr) = next {
                let mut alloc_box = Box::from_raw(curr.as_ptr());
                next = alloc_box.to_back;
                if let Some(waker) = alloc_box.waker.take() {
                    waker.wake();
                }
                drop(alloc_box);
            }
            if cfg!(debug_assertions) {
                self.front_back = None;
            }
        }
    }
}

impl NodeHandle {
    // construct un-linked node allocation.
    pub(crate) fn new() -> Self {
        let ptr = unsafe { NonNull::new_unchecked(Box::into_raw(Box::new(NodeAlloc::default()))) };
        NodeHandle { ptr, linked: false }
    }

    // whether this node is linked.
    pub(crate) fn is_linked(&self) -> bool {
        self.linked
    }
}

impl Drop for NodeQueue {
    fn drop(&mut self) {
        // all linked nodes are automatically dropped when their NodeQueue is dropped
        self.purge();
    }
}

impl Drop for NodeHandle {
    fn drop(&mut self) {
        // all non-linked nodes are automatically dropped when their NodeHandle is dropped
        unsafe {
            if !self.linked {
                drop(Box::from_raw(self.ptr.as_ptr()));
            }
        }
    }
}


unsafe impl Send for NodeHandle {}
unsafe impl Sync for NodeHandle {}

unsafe impl Send for NodeQueue {}
unsafe impl Sync for NodeQueue {}


#[cfg(test)]
mod tests {
    use super::*;

    #[allow(dead_code)]
    #[allow(unreachable_code)]
    #[allow(invalid_value)]
    unsafe fn ensure_node_handle_is_send() -> impl Send {
        panic!();
        std::mem::zeroed::<NodeHandle>()
    }

    #[allow(dead_code)]
    #[allow(unreachable_code)]
    #[allow(invalid_value)]
    unsafe fn ensure_node_handle_is_sync() -> impl Sync {
        panic!();
        std::mem::zeroed::<NodeHandle>()
    }

    #[allow(dead_code)]
    #[allow(unreachable_code)]
    unsafe fn ensure_node_queue_is_send() -> impl Send {
        panic!();
        std::mem::zeroed::<NodeQueue>()
    }

    #[allow(dead_code)]
    #[allow(unreachable_code)]
    unsafe fn ensure_node_queue_is_sync() -> impl Sync {
        panic!();
        std::mem::zeroed::<NodeQueue>()
    }
}
