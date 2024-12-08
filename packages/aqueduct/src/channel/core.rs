// minimal safe API for the channel. the exposed API is a convenience wrapper around this.

use super::{
    seg_queue::SegQueue,
    node_queue::{NodeQueue, NodeHandle},
    polling::DropWakers,
};
use std::{
    sync::{
        atomic::{
            Ordering::Relaxed,
            AtomicU64,
            AtomicU8,
        },
        Arc,
        Mutex,
        MutexGuard,
    },
    task::{Poll, Context},
    future::Future,
    pin::Pin,
};


// ==== the channel itself ====


const NODE_POOL_SIZE: usize = 4;


// handle to a channel.
pub(crate) struct Channel<T>(Arc<Shared<T>>);

// channel shared state.
struct Shared<T> {
    // mutex around lockable state.
    lockable: Mutex<Lockable<T>>,

    // send reference count.
    send_count: AtomicU64,
    // recv reference count.
    recv_count: AtomicU64,

    // begins as SendState::Normal. may eventually change to a different value.
    //
    // - once changes to a value other than normal, never changes again.
    // - holds a value other than normal if and only if send node queue is purged.
    // - if holds a value other than normal, send operations immediately return a corresponding
    //   error.
    send_state: AtomicU8,
    // begins as RecvState::Normal. may eventually change to a different value.
    //
    // - once changes to a value other than normal, never changes again.
    // - holds a value other than normal if and only if send node queue is purged.
    // - if holds a value other than normal, recv operations immediately return a corresponding
    //   error (or "finished" value in the case of RecvState::Finished).
    recv_state: AtomicU8,
}

// channel lockable state.
struct Lockable<T> {
    // storage for elements.
    elems: SegQueue<T>,
    // elems maximum length.
    bound: Option<usize>,
    // node queue for send futures.
    send_nodes: NodeQueue,
    // node queue for recv futures.
    recv_nodes: NodeQueue,
    // whether the sending side has finished the channel. starts false, and may at some point
    // switch to true. if ever it is the case the all of these conditions are met:
    //
    // - recv_state is normal
    // - finished is true
    // - elems is empty
    //
    // then recv_state automatically transitions to finished (and the recv node queue is purged).
    finished: bool,
    // pool of spare unlinked node allocations.
    node_pool: [Option<NodeHandle>; NODE_POOL_SIZE],
}

// possible values for Shared.send_state
#[repr(u8)]
pub(crate) enum SendState {
    // sending may still be possible.
    Normal,
    // all receivers have been channel.
    NoReceivers,
    // the sender cancelled the stream.
    Cancelled,
    // the encompassing network connection was lost.
    ConnectionLost,
    // the recv half of the channel was sent through another networked channel in a message that
    // failed to be delivered to the remote side despite the connection as a whole remaining.
    LostInTransit,
}

// possible values for Shared.recv_state
#[repr(u8)]
pub(crate) enum RecvState {
    // receiving may still be possible.
    Normal,
    // the sender finished the channel and all messages in it have been received.
    Finished,
    // the sender cancelled the channel.
    Cancelled,
    // the encompassing network connection was lost.
    ConnectionLost,
    // the send half of the channel was sent through another networked channel in a message that
    // failed to be delivered to the remote side despite the connection as a whole remaining.
    LostInTransit,
}

impl<T> Channel<T> {
    // construct empty channel with send and recv counts of 1.
    pub(crate) fn new() -> Self {
        Channel(Arc::new(Shared {
            lockable: Mutex::new(Lockable {
                elems: SegQueue::new(),
                bound: None,
                send_nodes: NodeQueue::new(),
                recv_nodes: NodeQueue::new(),
                finished: false,
                node_pool: [None, None, None, None],
            }),
            send_count: AtomicU64::new(1),
            recv_count: AtomicU64::new(1),
            send_state: AtomicU8::new(SendState::Normal as u8),
            recv_state: AtomicU8::new(RecvState::Normal as u8),
        }))
    }

    // clone another handle to the channel.
    pub(crate) fn clone(&self) -> Self {
        Channel(Arc::clone(&self.0))
    }

    // atomic-read the send state byte.
    pub(crate) fn send_state(&self) -> u8 {
        self.0.send_state.load(Relaxed)
    }

    // atomic-read the recv state byte.
    pub(crate) fn recv_state(&self) -> u8 {
        self.0.recv_state.load(Relaxed)
    }

    // get the send reference count atomic int.
    pub(crate) fn send_count(&self) -> &AtomicU64 {
        &self.0.send_count
    }

    // get the recv reference count atomic int.
    pub(crate) fn recv_count(&self) -> &AtomicU64 {
        &self.0.recv_count
    }

    // lock the channel
    pub(crate) fn lock(&self) -> Lock<'_, T> {
        Lock {
            shared: &self.0,
            lock: self.0.lockable.lock().unwrap(),
        }
    }
}

// lock on a channel.
pub(crate) struct Lock<'a, T> {
    shared: &'a Arc<Shared<T>>,
    lock: MutexGuard<'a, Lockable<T>>,
}

impl<'a, T> Lock<'a, T> {
    // set a maximum length.
    pub(crate) fn set_bound(&mut self, bound: usize) {
        self.lock.bound = Some(bound);
    }

    // take an unlinked node from the pool or allocate a new one
    fn allocate_node(&mut self) -> NodeHandle {
        let node = self.lock.node_pool.iter_mut()
            .filter_map(|opt| opt.take())
            .next()
            .unwrap_or_else(|| NodeHandle::new());
        debug_assert!(!node.is_linked());
        node
    }

    // construct a send future.
    //
    // panics if SendState is not Normal.
    pub(crate) fn send(&mut self, elem: T) -> Send<T> {
        // safety check
        assert_eq!(self.shared.send_state.load(Relaxed), SendState::Normal as u8, "internal bug");

        // allocate unlinked node
        let mut node = self.allocate_node();

        // link node.
        // safety:
        // - we know that send state is normal, therefore the send node queue is not purged.
        // - node is either newly allocated or from the pool, so it is not linked.
        unsafe { self.lock.send_nodes.push(&mut node); }

        // construct future
        Send(Some(SendInner { shared: Arc::clone(self.shared), elem, node }))
    }

    // construct a recv future.
    //
    // panics if RecvState is not normal.
    pub(crate) fn recv(&mut self) -> Recv<T> {
        // safety check
        assert_eq!(self.shared.recv_state.load(Relaxed), RecvState::Normal as u8, "internal bug");

        // allocate unlinked node
        let mut node = self.allocate_node();

        // link node.
        // safety:
        // - we know that recv state is normal, therefore the recv node queue is not purged.
        // - node is either newly allocated or from the pool, so it is not linked.
        unsafe { self.lock.recv_nodes.push(&mut node); }

        // construct future
        Recv(Some(RecvInner { shared: Arc::clone(self.shared), node }))
    }

    // mark the sending side as having finished the channel, if not already done.
    //
    // once the sending side is marked as finished, the next time elems becomes empty, which may be
    // immediately, recv state automatically transitions to finished, unless it's already in some
    // other non-normal state.
    pub(crate) fn finish(&mut self) {
        // short-circuit if already finished
        if self.lock.finished { return; }

        // mark as finished
        self.lock.finished = true;

        // if conditions are met, transition receivers into finished state
        let recv_state = self.shared.recv_state.load(Relaxed);
        if recv_state == RecvState::Normal as u8 && self.lock.elems.len() == 0 {
            self.shared.recv_state.store(RecvState::Finished as u8, Relaxed);
            self.lock.recv_nodes.purge();
        }
    }
}


// ==== send futures ====


// send future. internally locks the channel when dropped.
pub(crate) struct Send<T>(Option<SendInner<T>>);

// state for a `Send` which has not yet resolved or cancelled.
struct SendInner<T> {
    // handle to channel shared state.
    shared: Arc<Shared<T>>,
    // element to send.
    elem: T,
    // invariant: node is linked (into send node queue) so long as it is owned by SendInner.
    node: NodeHandle,
}

impl<T> Future for Send<T> {
    // - if the elem is successfully sent, resolves to ok.
    // - if the channel enters a send state other than normal, resolves to err with the send state
    //   byte and the elem which was being sent.
    type Output = Result<(), (u8, T)>;

    // internally locks the channel. panics iff already resolved or cancelled. guaranteed that, if
    // resolves, all wakers previously cloned when polling are dropped by the time `poll` returns.
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), (u8, T)>> {
        // safety: Self is !Unpin only because T is !Unpin
        let this = unsafe { self.get_unchecked_mut() };

        // assert linked. make sure to return ownership of inner state back if we return pending.
        let mut inner = this.0.take()
            .expect("send future polled after already resolved or cancelled");

        // lock the channel
        let mut lock = inner.shared.lockable.lock().unwrap();

        // now that channel is locked, we can check for error without race conditions
        let send_state = inner.shared.send_state.load(Relaxed);
        if send_state != SendState::Normal as u8 {
            // resolve to error
            // safety: if send state is not normal, then send node queue has been purged, which
            //         would mean any previously cloned waker has already been dropped.
            return Poll::Ready(Err((send_state, inner.elem)));
        }

        // next, check whether we'll return pending.

        // safety:
        // - send state is normal, therefore send nodes is not purged.
        // - we already panicked if node is not linked.
        // - the type system ensures we would only link this node into the send node queue.
        let is_front = unsafe { lock.send_nodes.is_front(&inner.node) };

        if !is_front || lock.bound.is_some_and(|n| lock.elems.len() >= n) {
            // either backpressure or this future isn't at the front of the send node queue

            // install a waker from the current context
            // safety: same as above
            let waker = unsafe { lock.send_nodes.waker(&mut inner.node) };
            *waker = Some(cx.waker().clone());

            // put inner state back before returning pending
            drop(lock);
            this.0 = Some(inner);

            // return pending
            return Poll::Pending;
        }
        // at this point, we know we will send the elem and unlink the node now

        // send elem
        lock.elems.push(inner.elem);

        // unlink node.
        // safety:
        // - same as above.
        // - NodeQueue.remove automatically clears the node's waker.
        unsafe { lock.send_nodes.remove(&mut inner.node) };

        // return node to pool (otherwise we just let it be dropped)
        debug_assert!(!inner.node.is_linked());
        if let Some(slot) = lock.node_pool.iter_mut().find(|opt| opt.is_none()) {
            *slot = Some(inner.node);
        }
        
        // notify futures that are now unblocked
        if lock.elems.len() == 1 {
            // next recv node, if channel was previously empty
            lock.recv_nodes.wake_front();
        }
        if lock.bound.is_none_or(|n| lock.elems.len() < n) {
            // next send node, if channel is still not full
            lock.send_nodes.wake_front();
        }

        // done
        Poll::Ready(Ok(()))
    }
}

impl<T> Send<T> {
    // if not already resolved or cancelled, cancel the future and return the elem.
    //
    // internally locks the channel. never panics. guaranteed that all wakers previously cloned
    // when polling are dropped by the time `cancel` returns.
    pub(crate) fn cancel(&mut self) -> Option<T> {
        // assert linked or short-circuit
        let Some(mut inner) = self.0.take() else { return None };

        // lock the channel
        let mut lock = inner.shared.lockable.lock().unwrap();

        // now that channel is locked, we can check send state
        let send_state = inner.shared.send_state.load(Relaxed);
        if send_state == SendState::Normal as u8 {
            // safety:
            // - send state is normal, therefore send node queue is not purged.
            // - we already short-circuited if node is not linked.
            // - the type system ensures we would only link this node into the send node queue.
            let is_front = unsafe { lock.send_nodes.is_front(&inner.node) };

            // unlink node.
            // safety: same as above.
            // note: this drops any previously cloned waker.
            unsafe { lock.send_nodes.remove(&mut inner.node) };

            // return node to pool (otherwise we just let it be dropped)
            debug_assert!(!inner.node.is_linked());
            if let Some(slot) = lock.node_pool.iter_mut().find(|opt| opt.is_none()) {
                *slot = Some(inner.node);
            }

            // notify futures that are now unblocked
            if is_front && lock.bound.is_none_or(|n| lock.elems.len() < n) {
                // next send node, if we were previously at the front of the queue and the channel
                // is not full.
                lock.send_nodes.wake_front();
            }
        }
        // if send state is not normal, then send node queue has been purged, which would mean any
        // previously cloned waker has already been dropped.

        // done
        Some(inner.elem)
    }
}

// safety:
//
// - if poll resolves, it guarantees that it drops all previously cloned wakers before returning.
// - if cancel is called, it guarantees that it drops all previously cloned wakers before
//   returning.
// - poll and drop_wakers panic if and only if the future has previously resolved or cancelled, and
//   they panic without cloning any additional waker handles.
unsafe impl<T> DropWakers for Send<T> {
    type DropWakersOutput = T;

    fn drop_wakers(&mut self) -> T {
        // panic shouldn't be reachable, but only due to details of outer safe API, so we don't
        // rely on it for preventing UB.
        self.cancel().expect("internal bug")
    }
}

impl<T> Drop for Send<T> {
    fn drop(&mut self) {
        // clean up allocation and unblock other nodes if dropped without resolving.
        self.cancel();
    }
}


// ==== recv futures ====


// recv future. internally locks the channel when dropped.
pub(crate) struct Recv<T>(Option<RecvInner<T>>);

// state for a `Recv` which has not yet resolved or cancelled.
struct RecvInner<T> {
    // handle to channel shared state.
    shared: Arc<Shared<T>>,
    // invariant: node is linked (into recv node queue) so long as it is owned by RecvInner.
    node: NodeHandle,
}

impl<T> Future for Recv<T> {
    // - if an elem is successfully received, resolves to the elem.
    // - if the channel enters a recv state other than normal, resolves to err with the recv state
    //   byte.
    type Output = Result<T, u8>;

    // internally locks the channel. panics iff already resolved or cancelled. guaranteed that, if
    // resolves, all wakers previously cloned when polling are dropped by the same `poll` returns.
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<T, u8>> {
        // safety: Self is !Unpin only because T is !Unpin
        let this = unsafe { self.get_unchecked_mut() };

        // assert linked. make sure to return ownership of inner state back if we return pending.
        let mut inner = this.0.take()
            .expect("recv future polled after already resolved or cancelled");

        // lock the chanel
        let mut lock = inner.shared.lockable.lock().unwrap();

        // now that channel is locked, we can check for error without race conditions
        let recv_state = inner.shared.recv_state.load(Relaxed);
        if recv_state != RecvState::Normal as u8 {
            // resolve to error
            // safety: if recv state is not normal, then recv node queue has been purged, which
            //         would mean that any previously cloned waker has already been dropped.
            return Poll::Ready(Err(recv_state));
        }

        // next, check whether we'll return pending.

        // safety:
        // - recv state is normal, therefore recv nodes is not purged.
        // - we already panicked if node is not linked.
        // - the type system ensures we would only link this node into the send node queue.
        let is_front = unsafe { lock.send_nodes.is_front(&inner.node) };

        if !is_front || lock.elems.len() == 0 {
            // either queue is empty or this future isn't at the front of the recv node queue

            // install a waker from the current context
            // safety: same as above
            let waker = unsafe { lock.recv_nodes.waker(&mut inner.node) };
            *waker = Some(cx.waker().clone());

            // put inner state back before returning pending
            drop(lock);
            this.0 = Some(inner);

            // return pending
            return Poll::Pending;
        }
        // at this point, we know we will recv an elem and unlink the node now

        // recv elem.
        // panic safety: we short-circuited if elems was empty
        let elem = lock.elems.pop().unwrap();

        // unlink node.
        // safety:
        // - same as above
        // - NodeQueue.remove automatically clears the node's waker
        unsafe { lock.recv_nodes.remove(&mut inner.node) };

        // return node to pool (otherwise we just let it be dropped)
        debug_assert!(!inner.node.is_linked());
        if let Some(slot) = lock.node_pool.iter_mut().find(|opt| opt.is_none()) {
            *slot = Some(inner.node);
        }

        // notify futures that are now unblocked
        if lock.elems.len() == 0 {
            if lock.finished {
                // elems is empty and senders are finished
                // transition recv state to finished and purge recv node queue
                // (this automatically notifies all nodes in recv queue all at once)
                inner.shared.recv_state.store(RecvState::Finished as u8, Relaxed);
                lock.recv_nodes.purge();
            }
        } else {
            // elems is not yet empty, so notify next recv ndoe
            lock.recv_nodes.wake_front();
        }
        if lock.bound.is_some_and(|n| n == lock.elems.len() + 1) {
            // if backpressure was jsut relieved, notify the next send node
            lock.send_nodes.wake_front();
        }

        Poll::Ready(Ok(elem))
    }
}

impl<T> Recv<T> {
    // if not already resolved or cancelled, cancel the future.
    //
    // internally locks the channel. never panics. guaranteed that all wakers previously cloned
    // when polling are dropped by the time `cancel` returns.
    pub(crate) fn cancel(&mut self) {
        // assert linked or short-circuit
        let Some(mut inner) = self.0.take() else { return };

        // lock the channel
        let mut lock = inner.shared.lockable.lock().unwrap();

        // assert recv state normal or short-circuit
        let recv_state = inner.shared.recv_state.load(Relaxed);
        if recv_state != RecvState::Normal as u8 {
            // if recv state is not normal, then recv node queue has been purged, which would mean
            // any previously cloned waker has already been dropped.
            return;
        }

        // safety:
        // - recv state is normal, therefore recv node queue is not purged.
        // - we already short-circuited if node is not linked.
        // - the type system ensures we would only link this node into the recv node queue.
        let is_front = unsafe { lock.recv_nodes.is_front(&inner.node) };

        // unlink node.
        // safety: same as above.
        // note: this drops any previously cloned waker.
        unsafe { lock.recv_nodes.remove(&mut inner.node) };

        // return node to pool (otherwise we just let it be dropped)
        debug_assert!(!inner.node.is_linked());
        if let Some(slot) = lock.node_pool.iter_mut().find(|opt| opt.is_none()) {
            *slot = Some(inner.node);
        }

        // notify futures that are now unblocked
        if is_front && lock.elems.len() != 0 {
            // next recv node, if we were previously at the front of the queue and the channel
            // is not empty.
            lock.recv_nodes.wake_front();
        }
    }
}

// safety:
//
// - if poll resolves, it guarantees that it drops all previously cloned wakers before returning.
// - if cancel is called, it guarantees that it drops all previously cloned wakers before
//   returning.
// - poll and drop_wakers panic if and only if the future has previously resolved or cancelled, and
//   they panic without closing any additional waker handles.
unsafe impl<T> DropWakers for Recv<T> {
    type DropWakersOutput = ();

    fn drop_wakers(&mut self) {
        self.cancel();
    }
}

impl<T> Drop for Recv<T> {
    fn drop(&mut self) {
        // clean up allocation and unblock other nodes if dropped without resolving.
        self.cancel();
    }
}


// ==== tests ====


#[cfg(test)]
mod tests {
    use super::*;

    // assert that the only reason Send is !Unpin is because T may be !Unpin
    #[allow(dead_code)]
    #[allow(unreachable_code)]
    unsafe fn ensure_send_is_only_unpin_because_of_t<T: Unpin>() -> impl Unpin {
        panic!();
        std::mem::zeroed::<Send<T>>()
    }

    // assert that the only reason Recv is !Unpin is because T may be !Unpin
    #[allow(dead_code)]
    #[allow(unreachable_code)]
    unsafe fn ensure_recv_is_only_unpin_because_of_t<T: Unpin>() -> impl Unpin {
        panic!();
        std::mem::zeroed::<Recv<T>>()
    }
}
