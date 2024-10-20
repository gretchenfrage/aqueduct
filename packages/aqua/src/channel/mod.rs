
mod seg_queue;

use self::seg_queue::SegQueue;
use std::{
    sync::{Arc, Mutex},
    ptr::NonNull,
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
};


const WAITER_POOL_SIZE: usize = 4;

/// shared handle to channel
struct Channel<T>(Arc<Mutex<State<T>>>);

/// channel inner shared lockable state
struct State<T> {
    /// elements in queue
    len: usize,
    /// capacity, if bounded. may become bounded after construction.
    cap: Option<usize>,
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
}

impl<T> Channel<T> {
    /// construct empty
    fn new() -> Self {
        Channel(Arc::new(Mutex::new(State {
            len: 0,
            cap: None,
            elems: SegQueue::new(),
            send_waiting_front_back: None,
            recv_waiting_front_back: None,
            pool: [None; 4],
        })))
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
            channel: Channel(Arc::clone(&self.0)),
            node,
        }
    }

    /// remove the given node from one of this channel's waiter queues
    unsafe fn remove_node(
        &self,
        front_back: &mut Option<(NonNull<WaiterNode>, NonNull<WaiterNode>)>,
        pool: &mut [Option<NonNull<WaiterNode>>; WAITER_POOL_SIZE],
        mut node: NonNull<WaiterNode>,
    ) {
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
    }
}

pub struct SendFut<T> {
    waiter: WaiterHandle<T>,
    elem: Option<T>,
}

impl<T> Future for SendFut<T> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<()> {

    }
}

impl<T> SendFut<T> {
    pub fn cancel(&mut self) -> Option<T> {

    }
}

pub struct RecvFut<T> {
    waiter: WaiterHandle<T>,
    done: bool,
}


impl<T> Future for RecvFut<T> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<()> {

    }
}

/// Error returned by [`Receiver::recv`][crate::Receiver::recv] and similar.
pub enum RecvError {
    /// The sender half cancelled the channel.
    Cancelled,
    /// The encompassing network connection was lost before the channel closed otherwise.
    NetConnectionLost,
}

/// Error returned by [`Receiver::try_recv`][crate::Receiver::try_recv].
pub enum TryRecvError {
    /// There is not currently a value available (although there may be in the future).
    Empty,
    /// The sender half cancelled the channel.
    Cancelled,
    /// The encompassing network connection was lost before the channel closed otherwise.
    NetConnectionLost,
}
