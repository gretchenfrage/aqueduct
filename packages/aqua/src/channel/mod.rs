//! Aqueduct's mpmc channels, which can be either local or networked
//!
//! An aqueduct channel is created by calling the `channel` function, which creates a linked pair
//! of `IntoSender<T>` and `Receiver<T>`. The reason it returns an `IntoSender` rather than a
//! sender is because the channel starts in an "undifferentiated" state where its delivery,
//! ordering, and queueing semantics are not yet determined. The user must call a method on
//! `IntoSender` to convert it into a more specific sender type based on various tradeoffs:
//!
//! - Bounded vs. Unbounded: Is there an upper bound to how much RAM the sender's buffer will
//!   consume, or will the sender always be able to push a new message without blocking?
//! - Ordered vs. Unordered: Will messages always be delivered in the same order they were sent,
//!   or will messages be delivered ASAP even if previous messages were delayed by the network?
//! - Reliable vs. Unreliable: Will messages lost by the network be re-transmitted until they are
//!   received, or will network resources be spent solely on fresh messages?
//!
//! If an `IntoSender` or `Receiver` are sent in a message down some other channel which is
//! networked, the channel is question will automatically become networked, via the same network
//! connection as the other channel. Alternatively, a channel can be used entirely locally.
//!
//! If the channel is not networked, a channel being unordered will have no effect, and a channel
//! being unreliable may have no effect. However, the `network-adversity-simulator` feature can be
//! enabled to introduce simulated effects to help test code.
//!
//! A channel's sender or receiver may be sent to a different process than the one it was created
//! in, but not both. That is illegal and will cause a runtime error. This makes it very
//! predictable what the network topology of a channel will be; If it is networked, it is obvious
//! what network connection it will be running over. This simplifies the handling of network
//! errors, since network failure due to external conditions only occurs on the level of a network
//! connection as a whole, rather than just for some channels within the connection specifically.
//! Thus, if a program encounters a "connection lost" error trying to use a channel, it can handle
//! it under the assumption that all other channels going over the same network connection would
//! also give a "connection lost" error if one tried to use them.
//!
//! Channels have both a concept of being "finished" and a concept of being "cancelled." When the
//! last sender handle of a channel is dropped, the stream of messages in the channel is considered
//! to end gracefully after all messages sent up to that point. Thus, attempts will still be made
//! to transmit and deliver all those messages until they are all received by the receiver.
//! Conversely, cancelling a channel typically represents aborting whatever operation the receiving
//! side was trying to perform. When a channel if cancelled, the receiver will be told to stop
//! delivering received messages as soon as possible and switch to returning a "cancelled" error
//! instead, the sender will abandon (re-)transmission of un-acked messages, and un-delivered
//! messages stored in RAM will be dropped. Since a channel being cancelled (or finished) is solely
//! the result of actions by the sender's process, if the receiver encounters a channel being
//! cancelled (or finished) that was not expected to be able to do so, the receiver can simply
//! consider this to be misbehavior on the part of the sender's process and handle it as such.
//!
//! Both the sender and receiver halves can be cloned. If multiple receiver halves are used, each
//! message will be delivered to the first receiver that claims it. Thus, this is a
//! multi-producer-multi-consumer channel, not a broadcast channel.
//!
//! This channel does _not_ support being bounded at a capacity of 0 while still being able to
//! convey a message if send and receive are blocked on simultaneously. Thus, this is _not_ a
//! rendezvous channel.
//!
//! If all receivers have been dropped, attempting to send a message will give a "no receivers"
//! error. If the channel has been cancelled, attempting to send a message afterwards will give a
//! "cancelled" error.
//!
//! The futures involved in this channel are fully fused and cancel-safe. If the channel is
//! ordered, sending and receiving is guaranteed to have effects in the order that futures were
//! _created_ by making calls to "send" and "recv" methods. Moreover, it's guaranteed that these
//! futures will have an effect if an only if they ever return `Poll::Ready`. This means that
//! futures to send into or receive from a channel form a queue of pending futures, such that a
//! future creating by calling a "send" or "recv" method blocks all "send" or "recv" futures
//! created after it until either it resolves or it is dropped.

mod seg_queue;
mod inner;
mod pollster2;

use self::{
    inner::{Channel, SendWaiter, RecvWaiter},
    pollster2::{
        DropWakers,
        block_on,
        block_on_timeout,
        dont_block_on,
    },
};
use std::{
    sync::atomic::{AtomicU64, Ordering},
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::{Instant, Duration},
};
use tokio::time::{sleep, sleep_until, Sleep};


/// aqueduct information about channel that is somewhat de-coupled from concurrency internals,
/// which is not wrapped in the channel's mutex but rather accessed atomically
struct AtomicMetaInfo {
    /// number of sender handles, which includes not-resolved send futures
    sender_count: AtomicU64,
    /// number of receiver handles, which includes not-resolved recv futures
    receiver_count: AtomicU64,
}

/// aqueduct information about channel that is somewhat de-coupled from concurrency internals
struct MetaInfo {
    /// channel ordering/delivery/buffering semantics, if determined yet
    semantics: Option<Semantics>,
    /// whether the channel is networked
    networked: bool,
    /// whether the sender has cancelled the channel
    cancelled: bool,
    /// whether the encompassing network connection has been lost
    connection_lost: bool,
    /// error that any send/recv operation will return. we store this to make the error reason'
    /// "sticky", eg. if a channel is cancelled and then all senders are dropped it will continue
    /// to return a "cancelled" error rather than switching to a "senders dropped" error
    error: Option<SendErrorReason>,
}

/// channel ordering/delivery/buffering semantics
enum Semantics {
    /// channel is reliable
    Reliable {
        /// whether channel is ordered
        ordered: bool,
        /// optional bound on channel length before sending blocks
        bound: Option<usize>,
    },
    /// channel is not reliable
    Unreliable {
        /// optional bound on channel length before sending evicts front of queue
        bound: Option<usize>,
    },
}

pub fn channel<T>() -> (IntoSender<T>, Receiver<T>) {
    let channel = Channel::new(
        MetaInfo {
            semantics: None,
            networked: false,
            cancelled: false,
            connection_lost: false,
            error: None,
        },
        AtomicMetaInfo { sender_count: AtomicU64::new(1), receiver_count: AtomicU64::new(1) },
    );
    (IntoSender(channel.clone()), Receiver(channel))
}

pub struct IntoSender<T>(Channel<T>);

/// A sender handle to a channel with semantics which may make the sender block
pub struct BoundedSender<T>(Channel<T>);

/// A sender handle to a channel with semantics which ensure the sender will never block
pub struct UnboundedSender<T>(Channel<T>);

pub struct Receiver<T>(Channel<T>);

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct SendError<T> {
    pub value: T,
    pub reason: SendErrorReason,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum SendErrorReason {
    NoReceivers,
    Cancelled,
    ConnectionLost,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct TrySendError<T> {
    pub value: T,
    pub reason: TrySendErrorReason,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum TrySendErrorReason {
    TimedOutFull,
    NoReceivers,
    Cancelled,
    ConnectionLost,
}

impl<T> From<SendError<T>> for TrySendError<T> {
    fn from(e: SendError<T>) -> Self {
        TrySendError {
            value: e.value,
            reason: e.reason.into(),
        }
    }
}

impl From<SendErrorReason> for TrySendErrorReason {
    fn from(e: SendErrorReason) -> Self {
        match e {
            SendErrorReason::NoReceivers => TrySendErrorReason::NoReceivers,
            SendErrorReason::Cancelled => TrySendErrorReason::Cancelled,
            SendErrorReason::ConnectionLost => TrySendErrorReason::ConnectionLost,
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum RecvError {
    Cancelled,
    ConnectionLost,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum TryRecvError {
    TimedOutEmpty,
    Cancelled,
    ConnectionLost,
}

impl From<RecvError> for TryRecvError {
    fn from(e: RecvError) -> Self {
        match e {
            RecvError::Cancelled => TryRecvError::Cancelled,
            RecvError::ConnectionLost => TryRecvError::ConnectionLost,
        }
    }
}

impl<T> BoundedSender<T> {
    /// Send a message, awaiting until back-pressure permits it
    pub fn send(&self, message: T) -> SendFut<T> {
        SendFut {
            waiter: self.0.push_send_node(),
            elem: Some(message),
        }
    }

    /// Send a message, awaiting until back-pressure permits it or the timeout elapses
    pub fn send_timeout(&self, message: T, timeout: Duration) -> SendTimeoutFut<T> {
        SendTimeoutFut {
            send: self.send(message),
            timeout: sleep(timeout),
        }
    }

    /// Send a message, awaiting until back-pressure permits it or the deadline is reached
    pub fn send_deadline(&self, message: T, deadline: Instant) -> SendTimeoutFut<T> {
        SendTimeoutFut {
            send: self.send(message),
            timeout: sleep_until(deadline.into()),
        }
    }

    /// Send a message, blocking until back-pressure permits it
    pub fn send_blocking(&self, message: T) -> Result<(), SendError<T>> {
        block_on(&mut self.send(message))
    }

    /// Send a message, blocking until back-pressure permits it or the timeout elapses
    pub fn send_blocking_timeout(
        &self,
        message: T,
        timeout: Duration,
    ) -> Result<(), TrySendError<T>> {
        let mut fut = self.send(message);
        block_on_timeout(&mut fut, timeout)
            .map(|result| result.map_err(From::from))
            .unwrap_or_else(|| Err(TrySendError {
                value: fut.cancel().unwrap(),
                reason: TrySendErrorReason::TimedOutFull,
            }))
    }

    /// Send a message, blocking until back-pressure permits it or the deadline is reached
    pub fn send_blocking_deadline(
        &self,
        message: T,
        deadline: Instant,
    ) -> Result<(), TrySendError<T>> {
        self.send_blocking_timeout(message, deadline.saturating_duration_since(Instant::now()))
    }

    /// Send a message if back-pressure permits it without blocking
    pub fn try_send(&self, message: T) -> Result<(), TrySendError<T>> {
        let mut fut = self.send(message);
        dont_block_on(&mut fut)
            .map(|result| result.map_err(From::from))
            .unwrap_or_else(|| Err(TrySendError {
                value: fut.cancel().unwrap(),
                reason: TrySendErrorReason::TimedOutFull,
            }))
    }
}

impl<T> UnboundedSender<T> {
    /// Send a message immediately
    pub fn send(&self, message: T) -> Result<(), SendError<T>> {
        dont_block_on(&mut SendFut {
            waiter: self.0.push_send_node(),
            elem: Some(message),
        }).unwrap()
    }
}

/// Future for sending a message into a channel
///
/// Blocks all send futures on the same channel created after this future until this future either
/// resolves or is dropped or cancelled. If this future is dropped or cancelled without resolving,
/// it is guaranteed that it will have no effect on the channel state.
pub struct SendFut<T> {
    waiter: SendWaiter<T>,
    elem: Option<T>,
}

impl<T> SendFut<T> {
    /// Cancel this send operation if it has not already resolved
    ///
    /// If and only if this future has not returned `Poll::Ready`, this returns `Some` with the
    /// message which would have been sent, and unblocks the next send operation. Attempting to
    /// `poll` this future after calling this method will panic.
    ///
    /// This has the same effect as dropping this future, except it doesn't require ownership of
    /// the future to be consumed and it gives back the message.
    pub fn cancel(&mut self) -> Option<T> {
        let removed = self.waiter.remove();
        debug_assert_eq!(removed, self.elem.is_some());
        self.elem.take()
    }
}

impl<T> Future for SendFut<T> {
    type Output = Result<(), SendError<T>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        this.waiter
            .poll(cx, |queue, meta, _| {
                if let Some(reason) = meta.error {
                    let error = SendError {
                        value: this.elem.take().expect("SendFut polled after cancelled"),
                        reason,
                    };
                    return (Some(Err(error)), false);
                }

                let has_capacity = match meta.semantics {
                    Some(Semantics::Reliable { bound: Some(bound), .. }) => queue.len() < bound,
                    Some(Semantics::Reliable { bound: None, .. }) => true,
                    Some(Semantics::Unreliable { bound: Some(bound) }) => {
                        // bounded unreliable eviction
                        if queue.len() >= bound { queue.pop(); }
                        debug_assert!(queue.len() < bound);
                        true
                    }
                    Some(Semantics::Unreliable { bound: None }) => true,
                    // a SendFut should not be constructed while the channel is still undifferentiated
                    None => unreachable!(),
                };
                if has_capacity {
                    queue.push(this.elem.take().expect("SendFut polled after cancelled"));
                    let was_empty = queue.len() == 1;
                    (Some(Ok(())), was_empty)
                } else {
                    (None, false)
                }
            })
            .map(Poll::Ready)
            .unwrap_or(Poll::Pending)
    }
}

unsafe impl<T> DropWakers for SendFut<T> {
    fn drop_wakers(&mut self) {
        self.waiter.remove();
    }
}

/// Version of `SendFut<T>` with a time-based deadline
pub struct SendTimeoutFut<T> {
    send: SendFut<T>,
    timeout: Sleep,
}

impl<T> Future for SendTimeoutFut<T> {
    type Output = Result<(), TrySendError<T>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        unsafe {
            let this = self.get_unchecked_mut();

            let send = Pin::new_unchecked(&mut this.send);
            if let Poll::Ready(result) = send.poll(cx) {
                return Poll::Ready(result.map_err(From::from));
            }

            let timeout = Pin::new_unchecked(&mut this.timeout);
            if let Poll::Ready(()) = timeout.poll(cx) {
                return Poll::Ready(Err(TrySendError {
                    value: this.send.cancel().unwrap(),
                    reason: TrySendErrorReason::TimedOutFull,
                }));
            }

            Poll::Pending
        }
    }
}

/// Future for receiving a message from a channel
///
/// Resolves to `Ok(None)` if the channel finished gracefully and all messages in it have been
/// taken.
///
/// Blocks all send futures on the same channel created after this future until this future either
/// resolves or is dropped or cancelled. If this future is dropped or cancelled without resolving,
/// it is guaranteed that it will have no effect on the channel state.
pub struct RecvFut<T>(RecvWaiter<T>);

impl<T> RecvFut<T> {
    /// Cancel this recv operation if it has not already resolved
    ///
    /// If and only if this future has not returned `Poll::Ready`, this unblocks the next recv
    /// operation and returns true. Attempting to `poll` this future after calling this method
    /// will panic.
    ///
    /// This has the same effect as dropping this future, except it doesn't require ownership of
    /// the future to be consumed.
    pub fn cancel(&mut self) -> bool {
        self.0.remove()
    }
}

impl<T> Future for RecvFut<T> {
    type Output = Result<Option<T>, RecvError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        this.0
            .poll(cx, |queue, meta, channel| {
                if let Some(reason) = meta.error {
                    let error = match reason {
                        SendErrorReason::Cancelled => RecvError::Cancelled,
                        SendErrorReason::ConnectionLost => RecvError::ConnectionLost,
                        // the fact that this line of code is running should imply that this
                        // RecvFut exists and has not resolved, which should imply that it's
                        // keeping the channel's receiver count non-zero, which should make this
                        // impossible
                        SendErrorReason::NoReceivers => unreachable!(),
                    };
                    return (Some(Err(error)), false);
                }

                if let Some(elem) = queue.pop() {
                    let was_at_capacity = matches!(
                        meta.semantics,
                        Some(Semantics::Reliable {
                            bound: Some(bound),
                            ..
                        }) if queue.len() + 1 == bound
                    );

                    (Some(Ok(Some(elem))), was_at_capacity)
                } else if channel.atomic_meta().sender_count.load(Ordering::Relaxed) == 0 {
                    (Some(Ok(None)), false)
                } else {
                    (None, false)
                }
            })
            .map(Poll::Ready)
            .unwrap_or(Poll::Pending)
    }
}

unsafe impl<T> DropWakers for RecvFut<T> {
    fn drop_wakers(&mut self) {
        self.0.remove();
    }
}

/// Version of `SendFut<T>` with a time-based deadline
pub struct RecvTimeoutFut<T> {
    recv: RecvFut<T>,
    timeout: Sleep,
}

impl<T> Future for RecvTimeoutFut<T> {
    type Output = Result<Option<T>, TryRecvError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        unsafe {
            let this = self.get_unchecked_mut();

            let send = Pin::new_unchecked(&mut this.recv);
            if let Poll::Ready(result) = send.poll(cx) {
                return Poll::Ready(result.map_err(From::from));
            }

            let timeout = Pin::new_unchecked(&mut this.timeout);
            if let Poll::Ready(()) = timeout.poll(cx) {
                return Poll::Ready(Err(TryRecvError::TimedOutEmpty));
            }

            Poll::Pending
        }
    }
}

/*
enum Lifecycle {
    /// sender is still in IntoSender form
    Undifferentiated,
    /// sender and receiver are both active
    Active {
        /// optional maximum length
        capacity: Option<usize>,
    },

}
*/

/*
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
*/