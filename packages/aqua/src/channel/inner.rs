//! Inner concurency structure for channel
//!
//! Takes rough architectural inspiration from flume.

use super::{
    seg_queue::SegQueue,
    AtomicMetaInfo,
    MetaInfo,
};
use std::{
    sync::{Arc, Mutex},
    ptr::NonNull,
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
    mem::swap,
};


const WAITER_POOL_SIZE: usize = 4;

/// shared handle to channel
pub struct Channel<T>(Arc<State<T>>);

/// channel inner state
struct State<T> {
    /// meta information that is somewhat de-coupled from concurrency details, which is also not
    /// stored within the mutex
    atomic_meta: AtomicMetaInfo,
    /// lockable subset of state
    lockable: Mutex<Lockable<T>>,
}

/// channel inner shared lockable state
struct Lockable<T> {
    /// meta information that is somewhat de-coupled from concurrency details
    meta: MetaInfo,
    /// elements in channel
    elems: SegQueue<T>,
    /// front and back nodes of waiter queue for tasks trying to send
    send_waiting_front_back: Option<(NonNull<WaiterNode>, NonNull<WaiterNode>)>,
    /// front and back nodes of waiter queue for tasks trying to recv
    recv_waiting_front_back: Option<(NonNull<WaiterNode>, NonNull<WaiterNode>)>,
    /// pool of spare waiter nodes
    pool: [Option<NonNull<WaiterNode>>; WAITER_POOL_SIZE],
}

/// heap allocated waiter node
#[derive(Default)]
struct WaiterNode {
    /// next node towards back of linked part
    to_back: Option<NonNull<WaiterNode>>,
    /// next node towards from of linked part
    to_front: Option<NonNull<WaiterNode>>,
    /// the last-polled-with waker for this node
    waker: Option<Waker>,
}

/// direct handle to a node within a channel's waiter queue
struct WaiterHandle<T> {
    /// handle to the owning channel
    channel: Channel<T>,
    /// pointer to the represented waiter node
    node: NonNull<WaiterNode>,
    /// whether the node has already been removed from the queue
    removed: bool,
}

impl<T> Channel<T> {
    /// construct empty
    pub fn new(meta: MetaInfo, atomic_meta: AtomicMetaInfo) -> Self {
        Channel(Arc::new(State {
            atomic_meta,
            lockable: Mutex::new(Lockable {
                meta,
                elems: SegQueue::new(),
                send_waiting_front_back: None,
                recv_waiting_front_back: None,
                pool: [None; 4],
            })
        }))
    }

    /// construct and push a new waiter node to the back of one of this channel's waiter queues
    unsafe fn push_node(
        &self,
        front_back: &mut Option<(NonNull<WaiterNode>, NonNull<WaiterNode>)>,
        pool: &mut [Option<NonNull<WaiterNode>>; WAITER_POOL_SIZE],
    ) -> WaiterHandle<T> {
        let mut node = pool
            .iter_mut().filter_map(|slot| slot.take()).next()
            .unwrap_or_else(||
                NonNull::new(Box::into_raw(Box::new(WaiterNode::default()))).unwrap());
        debug_assert!(node.as_ref().to_back.is_none());
        debug_assert!(node.as_ref().to_front.is_none());
        debug_assert!(node.as_ref().waker.is_none());
        if let &mut Some((_, ref mut back)) = front_back {
            debug_assert!(back.as_ref().to_back.is_none());
            back.as_mut().to_back = Some(node);
            node.as_mut().to_front = Some(*back);
            *back = node;
        } else {
            *front_back = Some((node, node));
        }
        WaiterHandle {
            channel: self.clone(),
            node,
            removed: false,
        }
    }

    /// remove the given node from one of this channel's waiter queues
    unsafe fn remove_node(
        &self,
        front_back: &mut Option<(NonNull<WaiterNode>, NonNull<WaiterNode>)>,
        pool: &mut [Option<NonNull<WaiterNode>>; WAITER_POOL_SIZE],
        mut node: NonNull<WaiterNode>,
    ) {
        let node_to_wake = node.as_ref().to_back.filter(|_| node.as_ref().to_front.is_none());

        // unlink
        if node.as_ref().to_front.is_none() && node.as_ref().to_back.is_none() {
            *front_back = None;
        } else {
            let &mut (ref mut front, ref mut back) = front_back.as_mut().unwrap();

            if let Some(mut to_front) = node.as_ref().to_front {
                to_front.as_mut().to_back = node.as_ref().to_back;
            } else {
                *front = node.as_ref().to_back.unwrap();
            }

            if let Some(mut to_back) = node.as_ref().to_back {
                to_back.as_mut().to_front = node.as_ref().to_front;
            } else {
                *back = node.as_ref().to_front.unwrap();
            }
        }

        // drop
        if let Some(slot) = pool.iter_mut().find(|slot| slot.is_none()) {
            node.as_mut().to_front = None;
            node.as_mut().to_back = None;
            node.as_mut().waker = None;
            *slot = Some(node);
        } else {
            drop(Box::from_raw(node.as_ptr()));
        }

        // if it was at the front of its queue, wake the node behind it, if able
        if let Some(mut node_to_wake) = node_to_wake {
            if let Some(waker) = node_to_wake.as_mut().waker.take() {
                waker.wake();
            }
        }
    }

    /// push a new node to the back of the send waiter queue
    pub fn push_send_node(&self) -> SendWaiter<T> {
        unsafe {
            let mut lock = self.0.lockable.lock().unwrap();
            let lockable = &mut *lock;
            SendWaiter(self.push_node(&mut lockable.send_waiting_front_back, &mut lockable.pool))
        }
    }

    /// push a new node to the back of the recv waiter queue
    pub fn push_recv_node(&self) -> SendWaiter<T> {
        unsafe {
            let mut lock = self.0.lockable.lock().unwrap();
            let lockable = &mut *lock;
            SendWaiter(self.push_node(&mut lockable.recv_waiting_front_back, &mut lockable.pool))
        }
    }

    /// lock the queue and let `f` mutate its inner state
    pub fn lock_mutate<F: FnOnce(&mut SegQueue<T>, &mut MetaInfo) -> O, O>(&self, f: F) -> O {
        let mut lock = self.0.lockable.lock().unwrap();
        let lockable = &mut *lock;
        f(&mut lockable.elems, &mut lockable.meta)
    }

    /// get this channel's `AtomicMetaInfo
    pub fn atomic_meta(&self) -> &AtomicMetaInfo {
        &self.0.atomic_meta
    }
}

impl<T> Clone for Channel<T> {
    fn clone(&self) -> Self {
        Channel(Arc::clone(&self.0))
    }
}

impl<T> Drop for Lockable<T> {
    fn drop(&mut self) {
        unsafe {
            for front_back in [self.send_waiting_front_back, self.recv_waiting_front_back] {
                let mut next = front_back.map(|(front, _)| front);
                while let Some(curr) = next {
                    next = curr.as_ref().to_back;
                    drop(Box::from_raw(curr.as_ptr()));
                }
            }

            for slot in self.pool {
                if let Some(node) = slot {
                    drop(Box::from_raw(node.as_ptr()));
                }
            }
        }
    }
}

impl<T> WaiterHandle<T> {
    /// inner backing of SendWaiter.poll and RecvWaiter.poll
    unsafe fn poll<F, O>(&mut self, cx: &mut Context, f: F, is_send: bool) -> Option<O>
    where
        F: FnOnce(&mut SegQueue<T>, &mut MetaInfo, &Channel<T>) -> (Option<O>, bool),
    {
        assert!(
            !self.removed,
            "{}Fut polled after already resolved",
            if is_send { "Send" } else { "Recv" },
        );

        let mut lock = self.channel.0.lockable.lock().unwrap();
        let lockable = &mut *lock;
        let mut this_front_back = &mut lockable.send_waiting_front_back;
        let mut other_front_back = &mut lockable.recv_waiting_front_back;
        if !is_send {
            swap(&mut this_front_back, &mut other_front_back);
        }

        if self.node.as_ref().to_front.is_none() {
            let (return_val, should_wake_other) =
                f(&mut lockable.elems, &mut lockable.meta, &self.channel);
            if should_wake_other {
                if let Some((mut other_front, _)) = other_front_back {
                    if let Some(waker) = other_front.as_mut().waker.take() {
                        waker.wake();
                    }
                }
            }
            if return_val.is_some() {
                self.channel.remove_node(this_front_back, &mut lockable.pool, self.node);
                self.removed = true;
            } else {
                self.node.as_mut().waker = Some(cx.waker().clone());
            }
            return_val
        } else {
            self.node.as_mut().waker = Some(cx.waker().clone());
            None
        }
    }

    /// remove this node from the channel if it isn't already removed
    unsafe fn remove(&mut self, is_send: bool) -> bool {
        if self.removed {
            false
        } else {
            let mut lock = self.channel.0.lockable.lock().unwrap();
            let lockable = &mut *lock;
            let front_back =
                if is_send { &mut lockable.send_waiting_front_back }
                else { &mut lockable.recv_waiting_front_back };
            self.channel.remove_node(front_back, &mut lockable.pool, self.node);
            self.removed = true;
            true
        }
    }
}

/// owning handle to node within the send waiter queue, which removes the node when dropped, unless
/// already removed. whenever this node is removed for any reason, if it was at the front of its
/// waiter queue, the node behind it gets its waker taken and waken.
pub struct SendWaiter<T>(WaiterHandle<T>);

/// owning handle to node within the recv waiter queue, which removes the node when dropped, unless
/// already removed. whenever this node is removed for any reason, if it was at the front of its
/// waiter queue, the node behind it gets its waker taken and waken.
pub struct RecvWaiter<T>(WaiterHandle<T>);

impl<T> SendWaiter<T> {
    /// if this waiter node is at the front of the send waiter queue, call `f`. `f` may mutate its
    /// inputs, and returns a 2-tuple:
    ///
    /// - the first returned value, an `Option`, is returned from this function.
    /// - if the first returned value is Some, this node gets removed from the waiter queue.
    /// - if the first returned value is None, `cx`'s waker is set as this node's waker.
    /// - if the second returned value, a boolean, is true, the recv waiter node at the front of
    ///   the recv waiter queue will have its waker taken and waken, if one exists.
    ///
    /// if this waiter node is not at the front of the send waiter queue, returns None.
    ///
    /// panics if this node has already been removed.
    pub fn poll<F, O>(&mut self, cx: &mut Context, f: F) -> Option<O>
    where
        F: FnOnce(&mut SegQueue<T>, &mut MetaInfo, &Channel<T>) -> (Option<O>, bool),
    {
        unsafe { self.0.poll(cx, f, true) }
    }

    /// remove this waiter node from its waiter queue if it isn't already removed, and return
    /// whether it was removed
    pub fn remove(&mut self) -> bool {
        unsafe { self.0.remove(true) }
    }
}

impl<T> RecvWaiter<T> {
    /// same as SendWaiter.poll but opposite
    pub fn poll<F, O>(&mut self, cx: &mut Context, f: F) -> Option<O>
    where
        F: FnOnce(&mut SegQueue<T>, &mut MetaInfo, &Channel<T>) -> (Option<O>, bool),
    {
        unsafe { self.0.poll(cx, f, false) }
    }

    /// remove this waiter node from its waiter queue if it isn't already removed, and return
    /// whether it was removed
    pub fn remove(&mut self) -> bool {
        unsafe { self.0.remove(false) }
    }
}

impl<T> Drop for SendWaiter<T> {
    fn drop(&mut self) {
        self.remove();
    }
}

impl<T> Drop for RecvWaiter<T> {
    fn drop(&mut self) {
        self.remove();
    }
}
