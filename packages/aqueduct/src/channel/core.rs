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
    ChannelLostInTransit,
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
    ChannelLostInTransit,
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

    // lock the channel.
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

    // construct a linked send future.
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
        let inner = SendInnerLinked { shared: Channel(Arc::clone(self.shared)), elem, node };
        Send(Some(SendInner::Linked(inner)))
    }

    // construct a linked recv future.
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
        let inner = RecvInnerLinked { shared: Channel(Arc::clone(self.shared)), node };
        Recv(Some(RecvInner::Linked(inner)))
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

    // transition the send state to some error state, if it's currently in the normal state.
    pub(crate) fn set_send_error(&mut self, error: SendState) {
        if self.shared.send_state.load(Relaxed) == SendState::Normal as u8 {
            self.shared.send_state.store(error as u8, Relaxed);
            self.lock.send_nodes.purge();
        }
    }

    // transition the recv state to some error state, if it's currently in the normal state.
    pub(crate) fn set_recv_error(&mut self, error: RecvState) {
        if self.shared.recv_state.load(Relaxed) == RecvState::Normal as u8 {
            self.shared.recv_state.store(error as u8, Relaxed);
            self.lock.recv_nodes.purge();
        }
    }

    // push the elem to elems. if length goes from 0 to 1, notify the recv node at the front. if
    // length is not yet at the bound, notify the send node at the front.
    pub(crate) fn enqueue(&mut self, elem: T) {
        self.lock.elems.push(elem);
        if self.lock.elems.len() == 1 {
            // next recv node, if channel was previously empty
            self.lock.recv_nodes.wake_front();
        }
        if self.lock.bound.is_none_or(|n| self.lock.elems.len() < n) {
            // next send node, if channel is still not full
            self.lock.send_nodes.wake_front();
        }
    }

    // get the elems queue.
    pub(crate) fn elems(&mut self) -> &mut SegQueue<T> {
        &mut self.lock.elems
    }
}


// ==== send futures ====


// send future. internally locks the channel when dropped.
pub(crate) struct Send<T>(Option<SendInner<T>>);

// state for a `Send` which has not yet resolved or cancelled.
enum SendInner<T> {
    // SendInner that is actually linked into the send node queue.
    Linked(SendInnerLinked<T>),
    // optimization to avoid locking.
    // safety: a send future with the Cheap variant will never clone wakers.
    Cheap(u8, T),
}

// content of SendInner that is actually linked into the send node queue.
struct SendInnerLinked<T> {
    // handle to channel shared state.
    shared: Channel<T>,
    // element to send.
    elem: T,
    // invariant: node is linked (into send node queue) so long as it is owned by SendInnerLinked.
    node: NodeHandle,
}

impl<T> Future for Send<T> {
    // resolves to a tuple containing:
    // - a result:
    //   - if the elem is successfully sent, resolves to ok.
    //   - if the channel enters a send state other than normal, resolves to err with the send
    //     state byte and the elem which was being sent.
    // - an option: a channel handle, if this future was originally linked.
    type Output = (Result<(), (u8, T)>, Option<Channel<T>>);

    // internally locks the channel. panics iff already resolved or cancelled. guaranteed that, if
    // resolves, all wakers previously cloned when polling are dropped by the time `poll` returns.
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = self.get_mut();

        // assert linked. make sure to return ownership of inner state back if we return pending.
        let inner = this.0.take()
            .expect("send future polled after already resolved or cancelled");
        let mut inner = match inner {
            SendInner::Linked(linked) => linked,
            SendInner::Cheap(send_state, elem) => {
                return Poll::Ready((
                    Err((send_state, elem)),
                    None,
                ));
            },
        };

        // try to check for error without locking
        let send_state = inner.shared.0.send_state.load(Relaxed);
        if send_state != SendState::Normal as u8 {
            // resolve to error
            // safety: if send state is not normal, then send node queue has been purged, which
            //         would mean any previously cloned waker has already been dropped.
            return Poll::Ready((
                Err((send_state, inner.elem)),
                Some(inner.shared),
            ));
        }

        // lock the channel
        let mut lock = inner.shared.lock();

        // now that channel is locked, check for error again without race conditions
        let send_state = inner.shared.0.send_state.load(Relaxed);
        if send_state != SendState::Normal as u8 {
            // safety: same as above
            drop(lock);
            return Poll::Ready((
                Err((send_state, inner.elem)),
                Some(inner.shared)
            ));
        }

        // next, check whether we'll return pending.

        // safety:
        // - send state is normal, therefore send nodes is not purged.
        // - we already panicked if node is not linked.
        // - the type system ensures we would only link this node into the send node queue.
        let is_front = unsafe { lock.lock.send_nodes.is_front(&inner.node) };

        if !is_front || lock.lock.bound.is_some_and(|n| lock.lock.elems.len() >= n) {
            // either backpressure or this future isn't at the front of the send node queue

            // install a waker from the current context
            // safety: same as above
            let waker = unsafe { lock.lock.send_nodes.waker(&mut inner.node) };
            *waker = Some(cx.waker().clone());

            // put inner state back before returning pending
            drop(lock);
            this.0 = Some(SendInner::Linked(inner));

            // return pending
            return Poll::Pending;
        }
        // at this point, we know we will send the elem and unlink the node now

        // unlink node.
        // safety:
        // - same as above.
        // - NodeQueue.remove automatically clears the node's waker.
        unsafe { lock.lock.send_nodes.remove(&mut inner.node) };

        // return node to pool (otherwise we just let it be dropped)
        debug_assert!(!inner.node.is_linked());
        if let Some(slot) = lock.lock.node_pool.iter_mut().find(|opt| opt.is_none()) {
            *slot = Some(inner.node);
        }
        
        // send elem and notify futures that are now unblocked
        lock.enqueue(inner.elem);

        // done
        drop(lock);
        Poll::Ready((
            Ok(()),
            Some(inner.shared),
        ))
    }
}

// safety: Send is only !Unpin only because T is !Unpin
impl<T> Unpin for Send<T> {}

impl<T> Send<T> {
    // construct a send future with a cheap variant that does not connect to the channel's shared
    // state and resolves to an error.
    pub(crate) fn cheap(send_state: u8, elem: T) -> Self {
        Send(Some(SendInner::Cheap(send_state, elem)))
    }

    // if not already resolved or cancelled, cancel the future and return the elem. upon
    // successfully cancelling, also returns a channel handle if this future was originally linked.
    //
    // internally locks the channel. never panics. guaranteed that all wakers previously cloned
    // when polling are dropped by the time `cancel` returns.
    pub(crate) fn cancel(&mut self) -> Option<(T, Option<Channel<T>>)> {
        // assert linked or short-circuit
        let Some(inner) = self.0.take() else { return None };
        let mut inner = match inner {
            SendInner::Linked(linked) => linked,
            SendInner::Cheap(_send_state, elem) => return Some((elem, None)),
        };

        // try to short-circuit based on send state without locking
        let send_state = inner.shared.0.send_state.load(Relaxed);
        if send_state != SendState::Normal as u8 {
            // safety: if send state is not normal, then send node queue has been purged, which
            //         would mean any previously cloned waker has already been dropped.
            return Some((inner.elem, Some(inner.shared)));
        }

        // lock the channel
        let mut lock = inner.shared.0.lockable.lock().unwrap();

        // now that channel is locked, check send state again without race conditions
        let send_state = inner.shared.0.send_state.load(Relaxed);
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
        drop(lock);
        Some((inner.elem, Some(inner.shared)))
    }

    // whether this future is resolved or cancelled.
    pub(crate) fn is_terminated(&self) -> bool {
        self.0.is_none()
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
    type DropWakersOutput = ();

    fn drop_wakers(&mut self) {
        // note: a previous prototype of this dropped wakers by cancelling, and unwrapping and
        //       returning the elem. however, we preserve the ability to poll the future instead.

        // assert linked or short-circuit
        let Some(inner) = self.0.as_mut() else { return };
        let inner = match inner {
            &mut SendInner::Linked(ref mut linked) => linked,
            &mut SendInner::Cheap(_, _) => return,
        };

        // try to short-circuit based on send state without locking
        let send_state = inner.shared.0.send_state.load(Relaxed);
        // safety: if send state is not normal, then send node queue has been purged, which would
        //         mean any previously cloned waker has already been dropped.
        if send_state != SendState::Normal as u8 { return };

        // lock the channel
        let mut lock = inner.shared.0.lockable.lock().unwrap();

        // now that channel is locked, check send state again without race conditions
        let send_state = inner.shared.0.send_state.load(Relaxed);
        // safety: same as above
        if send_state != SendState::Normal as u8 { return };

        // safety:
        // - send state is normal, therefore send nodes is not purged.
        // - we already short-circuited if node is not linked.
        // - the type system ensures we would only link this node into the send node queue.
        let waker = unsafe { lock.send_nodes.waker(&mut inner.node) };

        // simply drop any waker we previously installed up in our send node
        *waker = None;
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
enum RecvInner<T> {
    // RecvInner that is actually linked into the recv node queue.
    Linked(RecvInnerLinked<T>),
    // optimization to avoid locking.
    // safety: a recv future with the Cheap variant will never clone wakers.
    Cheap(u8),
}

struct RecvInnerLinked<T> {
    // handle to channel shared state.
    shared: Channel<T>,
    // invariant: node is linked (into recv node queue) so long as it is owned by RecvInner.
    node: NodeHandle,
}

impl<T> Future for Recv<T> {
    // resolves to a tuple containing:
    // - a result:
    //   - if an elem is successfully received, resolves to the elem.
    //   - if the channel enters a recv state other than normal, resolves to err with the recv state
    //     byte.
    // - an option: a channel handle, if this future was originally linked.
    type Output = (Result<T, u8>, Option<Channel<T>>);

    // internally locks the channel. panics iff already resolved or cancelled. guaranteed that, if
    // resolves, all wakers previously cloned when polling are dropped by the same `poll` returns.
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = self.get_mut();

        // assert linked. make sure to return ownership of inner state back if we return pending.
        let inner = this.0.take()
            .expect("recv future polled after already resolved or cancelled");
        let mut inner = match inner {
            RecvInner::Linked(linked) => linked,
            RecvInner::Cheap(recv_state) => {
                return Poll::Ready((
                    Err(recv_state),
                    None,
                ));
            },
        };

        // try to check recv state without locking
        let recv_state = inner.shared.0.recv_state.load(Relaxed);
        if recv_state != RecvState::Normal as u8 {
            // resolve to error
            // safety: if recv state is not normal, then recv node queue has been purged, which
            //         would mean that any previously cloned waker has already been dropped.
            return Poll::Ready((
                Err(recv_state),
                Some(inner.shared),
            ));
        }

        // lock the chanel
        let mut lock = inner.shared.0.lockable.lock().unwrap();

        // now that channel is locked, check for error again without race conditions
        let recv_state = inner.shared.0.recv_state.load(Relaxed);
        if recv_state != RecvState::Normal as u8 {
            // resolve to error
            // safety: same as above
            drop(lock);
            return Poll::Ready((
                Err(recv_state),
                Some(inner.shared),
            ));
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
            this.0 = Some(RecvInner::Linked(inner));

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
                inner.shared.0.recv_state.store(RecvState::Finished as u8, Relaxed);
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

        drop(lock);
        Poll::Ready((
            Ok(elem),
            Some(inner.shared),
        ))
    }
}

// safety: Recv is only !Unpin only because T is !Unpin
impl<T> Unpin for Recv<T> {}

impl<T> Recv<T> {
    // construct a recv future with a cheap variant that does not connect to the channel's shared
    // state and resolves to an `Err`.
    pub(crate) fn cheap(recv_state: u8) -> Self {
        Recv(Some(RecvInner::Cheap(recv_state)))
    }

    // if not already resolved or cancelled, cancel the future. if both successfully cancels and
    // also was originally linked, returns a channel handle.
    //
    // internally locks the channel. never panics. guaranteed that all wakers previously cloned
    // when polling are dropped by the time `cancel` returns.
    pub(crate) fn cancel(&mut self) -> Option<Channel<T>> {
        // assert linked or short-circuit
        let Some(inner) = self.0.take() else { return None };
        let mut inner = match inner {
            RecvInner::Linked(linked) => linked,
            RecvInner::Cheap(_recv_state) => return None,
        };

        // try to short-circuit based on recv state without locking
        let recv_state = inner.shared.0.recv_state.load(Relaxed);
        if recv_state != RecvState::Normal as u8 {
            // safety: if recv state is not normal, then recv node queue has been purged, which
            //         would mean any previously cloned waker has already been dropped.
            return Some(inner.shared);
        }

        // lock the channel
        let mut lock = inner.shared.0.lockable.lock().unwrap();

        // now that channel is locked, check recv state again without race conditions
        let recv_state = inner.shared.0.recv_state.load(Relaxed);
        if recv_state != RecvState::Normal as u8 {
            // safety: same as above
            drop(lock);
            return Some(inner.shared);
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

        drop(lock);
        Some(inner.shared)
    }

    // whether this future is resolved or cancelled.
    pub(crate) fn is_terminated(&self) -> bool {
        self.0.is_none()
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
        // note: a previous prototype of this dropped wakers by cancelling. however, we preserve
        //       the ability to poll the future instead.

        // assert linked or short-circuit
        let Some(inner) = self.0.as_mut() else { return };
        let inner = match inner {
            &mut RecvInner::Linked(ref mut linked) => linked,
            &mut RecvInner::Cheap(_) => return,
        };

        // try to short-circuit based on recv state without locking
        let recv_state = inner.shared.0.recv_state.load(Relaxed);
        // safety: if recv state is not normal, then recv node queue has been purged, which would
        //         mean any previously cloned waker has already been dropped.
        if recv_state != RecvState::Normal as u8 { return };

        // lock the channel
        let mut lock = inner.shared.0.lockable.lock().unwrap();

        // now that channel is locked, check recv state again without race conditions
        let recv_state = inner.shared.0.recv_state.load(Relaxed);
        if recv_state != RecvState::Normal as u8 { return };

        // safety:
        // - recv state is normal, therefore recv nodes is not purged.
        // - we already short-circuited if node is not linked.
        // - the type system ensures we would only link this node into the recv node queue.
        let waker = unsafe { lock.recv_nodes.waker(&mut inner.node) };

        // simply drop any waker we previously installed up in our send node
        *waker = None;
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

    #[allow(dead_code)]
    #[allow(unreachable_code)]
    unsafe fn ensure_send_is_unpin<T>() -> impl Unpin {
        panic!();
        std::mem::zeroed::<Send<T>>()
    }
}
