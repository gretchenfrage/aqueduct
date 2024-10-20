
use std::{
    mem::size_of,
    collections::{LinkedList, VecDeque},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
        Arc,
        Weak,
    },
    ptr::NonNull,
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
    cell::UnsafeCell,
};
use tokio::sync::{
    Notify,
    oneshot as tokio_oneshot,
};

const SEGMENT_CAP_BYTES: usize = 1024;

/// segment queue segment capacity in elements
const fn segment_cap<T>() -> usize {
    if size_of::<T>() == 0 {
        // edge case is pretty whatever, just dont divide by zero
        usize::MAX
    } else {
        // item is maybe bigger than SEGMENT_CAP_BYTES
        let n = SEGMENT_CAP_BYTES / size_of::<T>();
        if n < 1 { 1 } else { n }
    }
}

/// non-concurrent segment queue that backs the channel
/// 
/// bunch of VecDequeues linked together. we make sure to never trigger VecDequeue resizes.
#[derive(Default)]
struct SegQueue<T>(LinkedList<VecDeque<T>>);

impl<T> SegQueue<T> {
    /// push to the back
    fn push(&mut self, elem: T) {
        if let Some(seg) = self.0.back_mut().filter(|seg| seg.len() < seg.capacity()) {
            seg.push_back(elem);
        } else {
            let mut seg = VecDeque::with_capacity(segment_cap::<T>());
            seg.push_back(elem);
            self.0.push_back(seg);
        }
    }
    
    /// pop from the front
    fn pop(&mut self) -> Option<T> {
        self.0.front_mut()
            .and_then(|seg| seg.pop_front())
            // leave at least one segment as an optimization
            .inspect(|_| if self.0.len() > 1 && self.0.front().unwrap().is_empty() {
                self.0.pop_front().unwrap();
            })
    }
}

// wait nodes can form doubly linked queues which help synchronize the channel with tasks waiting
// to send or receiving a message.
// 
// the WaitNode is heap allocated and the heap allocation is stored as raw pointers. raw pointers
// to an heap allocation for a WaitNode exist in the following process:
// 
// 1. by being linked together into an intrusively doubly-linked queue, pointers to the end of
//    which are stored in a WaitQueue, which in turns in stored in Lockable.
// 2. in a WaitNodeHandle, which controls the lifetime of the heap allocation by dropping the heap
//    allocation (with some extra complications) when the WaitNodeHandle is dropped.
// 
// a WaitNode contains an UnsafeCell<WaitNodeInner>. one is allowed to get a mutable reference to
// the inner data if any one of the conditions is met:
// 
// 1. the node is "detached", as indicated by `detached` being true
// 2. the channel has been dropped, as indicated by the strong count of the Arc<Shared>
// 3. one holds a mutex on the Lockable
// 
// "detaching" is a process wherein:
// 
// 1. the thread doing the detaching must hold a mutex on the channel's lockable state while detaching
// 2. the node is removed from its linked list
// 3. the value `true` is stored in `detached`
// 
// the heap allocation is dropped when the future is dropped. before the heap allocation is dropped,
// the destructor must ensure that the node is removed from the channel's linked list, unless the
// channel as a whole has already been dropped.

/// channel inner state
struct Inner<T>(Arc<Mutex<Lockable<T>>>);

/// lockable subset of channel state
struct Lockable<T> {
    /// capacity, if initialized and bounded
    cap: Option<usize>,
    /// elems in queue
    queue: SegQueue<T>,
    /// wait queue for tasks waiting to send into the queue
    send_wait: WaitQueue<T>,
    /// wait queue for tasks waiting to receive from the queue
    recv_wait: WaitQueue<T>,
}

struct WaitQueue<T> {
    /// optional node stored in-place, which goes in front of the front of the linked queue, as an
    /// optimization
    pre_front: Option<Waker>,
    /// front and back of the linked queue, unless the linked queue is empty
    front_back: Option<(NonNull<WaitNode<T>>, NonNull<WaitNode<T>>)>,
}

struct WaitNode<T> {
    detached: AtomicBool,
    inner: UnsafeCell<WaitNodeInner<T>>,
}

struct WaitNodeInner<T> {
    /// next node towards front of queue. if None, this is the front of the linked queue. value
    /// meaningless if node detached.
    to_front: Option<NonNull<WaitNode<T>>>,
    /// next node towards back of queue. if None, this is the back of the linked queue. value
    /// meaningless if node detached.
    to_back: Option<NonNull<WaitNode<T>>>,
    /// most recently polled-with waker
    waker: Option<Waker>,
}

struct WaitNodeHandle<T> {
    shared: Weak<Mutex<Lockable<T>>>,
    /// if None, this node is stored in the queue's `pre_front`
    node: Option<NonNull<WaitNode<T>>>,
}

impl<T> Inner<T> {
    /// construct with defaults
    fn new() -> Self {
        Inner(Arc::new(Mutex::new(Lockable {
            cap: None,
            queue: SegQueue(Default::default()),
            send_wait: WaitQueue {
                pre_front: None,
                front_back: None,
            },
            recv_wait: WaitQueue {
                pre_front: None,
                front_back: None,
            },
        })))
    }
    
    /// construct a wait node and push it to the back of one of the wait queues
    fn push_wait_node<Q>(&self, q: Q) -> WaitNodeHandle<T>
    where
        Q: Fn(&mut Lockable<T>) -> &mut WaitQueue<T>,
    {
        let mut lock = self.0.lock().unwrap();
        let queue = q(&mut *lock);
        let mut node = WaitNode {
            detached: AtomicBool::new
        }
        if queue.front_back.is_none() && queue.pre_front.is_none() {
            // pre-front optimization
            queue.pre_front = 
        } else {
            
        }
    }
}


/*
/// Future for sending a value into the channel.
/// 
/// If the future is cancelled, 
pub struct SendFut<T>(*const WaitNode<T>);*/


/*
/// synchronization state for a thread-like blocking on sending
/// 
/// when possible, message is moved into the queue and then notify is called
struct SendWaiting<T> {
    msg: Mutex<Option<T>>,
    notify: Notify,
}

//pub struct SendFut<T>(Option<tokio_oneshot::)

/// future for receiving a message from a channel
pub struct RecvFut<T>(RecvFutInner<T>);

enum RecvFutInner<T> {
    
}

impl<T> Future for RecvFut<T> {
    
}


impl<T> Shared<T> {
    
}
*/