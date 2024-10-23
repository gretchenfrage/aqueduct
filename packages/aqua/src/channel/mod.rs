//! Aqueduct's mpmc channels, which can be either local or networked
//!
//! Aqueduct's channels take inspiration from the flume crate, but are different in some ways.
//!
//! An aqueduct channel is created by calling the [`channel`] function, which creates a linked pair
//! of [`IntoSender<T>`] and [`Receiver<T>`]. The reason it returns an `IntoSender` rather than a
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
//! "cancelled" error. A channel gets "locked in" to the first permanent error state it enters,
//! meaning that if a channel is cancelled, it won't switch from yielding "cancelled" errors to
//! "connection lost" errors even if the connection is lost, etc.
//!
//! The futures involved in this channel are fully fused and cancel-safe. If the channel is
//! ordered, sending and receiving is guaranteed to have effects in the order that futures were
//! _created_ by making calls to "send" and "recv" methods. Moreover, it's guaranteed that these
//! futures will have an effect if an only if they ever return `Poll::Ready`. This means that
//! futures to send into or receive from a channel form a queue of pending futures, such that a
//! future creating by calling a "send" or "recv" method blocks all "send" or "recv" futures
//! created after it until either it resolves or it is dropped.
//!
//! In a networked stream, the receiver side only buffers a small and constant number of messages,
//! regardless of whether the sender side is bounded or unbounded. It is the sender side which of
//! a networked unbounded channel which may consume an unbounded amount of RAM.
//!
//! Channels are backed by a linked segment queue structure, which ensures their memory usage both
//! grows and shrinks automatically in response to the number of queued messages.

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
    future::Future,
    pin::Pin,
    sync::atomic::{AtomicU64, Ordering},
    task::{Context, Poll},
    time::{Duration, Instant},
};
use seg_queue::SegQueue;
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

/// Create a new aqueduct channel
pub fn channel<T>() -> (IntoSender<T>, Receiver<T>) {
    let channel = Channel::new(
        MetaInfo {
            semantics: None,
            networked: false,
            error: None,
        },
        AtomicMetaInfo { sender_count: AtomicU64::new(1), receiver_count: AtomicU64::new(1) },
    );
    (IntoSender(channel.clone()), Receiver(channel))
}

/// Undifferentiated sender representing a channel with not-yet-determined semantics
pub struct IntoSender<T>(Channel<T>);

/// A sender handle to a channel with semantics that create backpressure
///
/// Send operations take effect strictly in the order their futures are created, and also all
/// sending futures are strictly cancel-safe. This means that send operation futures or threads
/// parked on a blocking send operation form a queue, such that when an async send method is called
/// and returns a future, the future will block all send futures created afterwards or blocking
/// send method calls initiated afterwards from resolving with anything other than perhaps a
/// timeout until previous futures are either resolved, dropped, or cancelled by calling their
/// [`cancel`][self::SendFut::cancel] method.
pub struct BlockingSender<T>(Channel<T>);

/// A sender handle to a channel with semantics that don't create backpressure
pub struct NonBlockingSender<T>(Channel<T>);

/// A receiver handle to a channel
///
/// Receive operations take effect strictly in the order their futures are created, and also all
/// receiving futures are strictly cancel-safe. This means that receive operation futures or
/// threads parked on a blocking receive operation form a queue, such that when an async receive
/// method is called and returns a future, the future will block all receive futures created
/// afterwards or blocking receive method calls initiated afterwards from resolving with anything
/// other than perhaps a timeout until previous futures are either resolved, dropped, or cancelled
/// by calling their [`cancel`][self::RecvFut::cancel] method.
pub struct Receiver<T>(Channel<T>);

impl<T> IntoSender<T> {
    /// inner helper method for differentiating
    fn init(&self, semantics: Semantics) {
        self.0.atomic_meta().sender_count.fetch_add(1, Ordering::Relaxed);
        self.0.lock_mutate(|_, meta| {
            meta.semantics = Some(semantics);
            (false, false)
        });
    }

    /// Become an ordered and bounded sender
    ///
    /// - **Ordered:**
    ///
    ///   Messages received will be received in the same order they were sent, with no holes.
    /// - **Bounded:**
    ///
    ///   The sender will buffer up to `bound` messages before blocking due to
    ///   backpressure. Blocking due to backpressure helps bound memory consumption if the receiver
    ///   is slow or even malicious.
    ///
    /// If networked, this will be backed by a single QUIC stream. Since the network may fail at
    /// any time, it is not possible to guarantee that all messages sent will be delivered.
    /// However, so long as the network connection is alive, a best effort will be made to transmit
    /// all sent messages to the receiver. The sequence of received messages is guaranteed to be a
    /// prefix of the sequence of sent messages.
    pub fn into_ordered_bounded(self, bound: usize) -> BlockingSender<T> {
        self.init(Semantics::Reliable { ordered: true, bound: Some(bound) });
        BlockingSender(self.0.clone())
    }

    /// Become an ordered and unbounded sender
    ///
    /// - **Ordered:**
    ///
    ///   Messages received will be received in the same order they were sent, with no holes.
    /// - **Unbounded:**
    ///
    ///   The sender will buffer arbitrarily many messages without backpressure if the receiver has
    ///   not acknowledged them. This means that the sender will never block, but it also means
    ///   that the sender has no inherent upper bound on worst case memory consumption.
    ///
    /// If networked, this will be backed by a single QUIC stream. Since the network may fail at
    /// any time, it is not possible to guarantee that all messages sent will be delivered.
    /// However, so long as the network connection is alive, a best effort will be made to transmit
    /// all sent messages to the receiver. The sequence of received messages is guaranteed to be a
    /// prefix of the sequence of sent messages.
    pub fn into_ordered_unbounded(self) -> NonBlockingSender<T> {
        self.init(Semantics::Reliable { ordered: true, bound: None });
        NonBlockingSender(self.0.clone())
    }

    /// Become an unordered (but reliable) and bounded sender
    ///
    /// - **Unordered:**
    ///
    ///   Messages may be received in a different order than they were sent in (although in most
    ///   real world networks these anomalies will occur only a small percentage of the time).
    ///
    ///   Holes may exist temporarily, but as long as the network connection is alive a best effort
    ///   will be made to eventually deliver all sent messages.
    ///
    ///   Compared to an ordered sender, this can improve latency for some messages by avoiding
    ///   [head-of-line blocking][1], because it prevents a delay in a single message from
    ///   cascading into delays for all messages sent after it.
    /// - **Bounded:**
    ///
    ///   The sender will buffer up to `bound` messages before blocking due to
    ///   backpressure. Blocking due to backpressure helps bound memory consumption if the receiver
    ///   is slow or even malicious.
    ///
    /// If networked, this will be backed by a different QUIC stream for each message. Since the
    /// network may fail at any time, it is not possible to guarantee that all messages sent will
    /// be delivered. However, so long as the network connection is alive, a best effort will be
    /// made to transmit all sent messages to the receiver.
    ///
    /// [1]: https://en.wikipedia.org/wiki/Head-of-line_blocking
    pub fn into_unordered_bounded(self, bound: usize) -> BlockingSender<T> {
        self.init(Semantics::Reliable { ordered: false, bound: Some(bound) });
        BlockingSender(self.0.clone())
    }

    /// Become an unordered (but reliable) and unbounded sender
    ///
    /// - **Unordered:**
    ///
    ///   Messages may be received in a different order than they were sent in (although in most
    ///   real world networks these anomalies will occur only a small percentage of the time).
    ///
    ///   Holes may exist temporarily, but as long as the network connection is alive a best effort
    ///   will be made to eventually deliver all sent messages.
    ///
    ///   Compared to an ordered sender, this can improve latency for some messages by avoiding
    ///   [head-of-line blocking][1], because it prevents a delay in a single message from
    ///   cascading into delays for all messages sent after it.
    /// - **Unbounded:**
    ///
    ///   The sender will buffer arbitrarily many messages without backpressure if the receiver has
    ///   not acknowledged them. This means that the sender will never block, but it also means
    ///   that the sender has no inherent upper bound on worst case memory consumption.
    ///
    /// If networked, this will be backed by a different QUIC stream for each message. Since the
    /// network may fail at any time, it is not possible to guarantee that all messages sent will
    /// be delivered. However, so long as the network connection is alive, a best effort will be
    /// made to transmit all sent messages to the receiver.
    ///
    /// [1]: https://en.wikipedia.org/wiki/Head-of-line_blocking
    pub fn into_unordered_unbounded(self) -> NonBlockingSender<T> {
        self.init(Semantics::Reliable { ordered: false, bound: None });
        NonBlockingSender(self.0.clone())
    }

    /// Become an unreliable (and unordered) sender.
    ///
    /// Messages may be received in a different order than they were sent in, and sent messages may
    /// not be received at all. Generally speaking, sent messages will be transmitted and received
    /// successfully most of the time when conditions are suitable, but if conditions arise such
    /// that the message could not be delivered to the receiver without some delay beyond the
    /// inherent network delay, the message will simply be dropped instead.
    ///
    /// Such conditions may include, but may not be limited to:
    ///
    /// - The network loses the packet (the sender will not bother to re-transmit it).
    /// - The sender's buffer(s) for unreliable messages become full (they will drop older outgoing
    ///   unreliable messages to make room for newer ones).
    /// - The receiver's buffer(s) for unreliable messages become full (they will drop older
    ///   incoming unreliable messages to make room for newer ones).
    ///
    /// An unreliable sender is always non-blocking. However, the user may provide an optional
    /// "eviction bound," which is a maximum number of queued messages beyond which sending an
    /// additional message into the channel will trigger the oldest queued message in the channel
    /// (the front of the queue) to be evicted and dropped. If no eviction bound is set, the sender
    /// will have no inherent upper bound on worst cast memory consumption.
    ///
    /// Sent messages will be transmitted as soon as possible to minimize delay. Overall, this
    /// exists for situations where a message ceases to be useful if it arrives late.
    ///
    /// If networked, this will be backed by QUIC's [Unreliable Datagram Extension][1]--although if
    /// sent messages are too large to fit in a datagram after serialization, they may be sent in
    /// one-off QUIC streams instead. QUIC streams may also be used to reliably convey some control
    /// data, such as the channel being closed or cancelled, or information about attachments to
    /// unreliable messages.
    ///
    /// [1]: https://datatracker.ietf.org/doc/html/rfc9221
    pub fn into_unreliable(self, eviction_bound: Option<usize>) -> NonBlockingSender<T> {
        self.init(Semantics::Unreliable { bound: eviction_bound });
        NonBlockingSender(self.0.clone())
    }

    /// Cancel the channel, abandoning the delivery of all messages, even currently queued ones
    ///
    /// This will trigger subsequent and pending send and receive operations to return a
    /// `Cancelled` error and all queued messages to be dropped as soon as possible without even
    /// delivering them. If networked (and reliable), this is done by QUIC stream cancellation,
    /// which takes advantages of QUIC's transport-level ability to abruptly abandon the
    /// (re-)transmission of a stream's data.
    ///
    /// Contrast to simply dropping all senders for this channel, which will cause the stream to
    /// be "finished"--a more graceful form of termination in which existing queued data will
    /// continue to be buffered and (re-)transmitted (if reliable) until delivered (unless the
    /// connection as a whole fails), after which receivers will return `Ok(None)` rather than
    /// `Err`. Conversely, cancellation is more-so designed for aborting operations.
    pub fn cancel(self) {
        // unlike the others, we take this by value rather by reference. this is to ensure type-
        // level prevention of a user cancelling an IntoSender and then sending it over the network
        enter_errored_state(&self.0, SendErrorReason::Cancelled);
    }
}

/// Error for trying to send a message into a channel
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct SendError<T> {
    /// The message that could not be sent
    pub message: T,
    /// The reason the message could not be sent
    pub reason: SendErrorReason,
}

/// Reason for a [`SendError`] occurring
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum SendErrorReason {
    /// All receiver handles have been dropped
    ///
    /// Any subsequent attempts to use the same channel will yield the same error.
    NoReceivers,
    /// This or some other sender handle was used to cancel the channel
    ///
    /// Any subsequent attempts to use the same channel will yield the same error.
    Cancelled,
    /// The channel's network connection was lost
    ///
    /// Any subsequent attempts to use the this or any channel on the same connection will yield
    /// the same error (except individual channels that have already entered some other permanent
    /// error state).
    ConnectionLost,
    /// The receiver was attached to a message which was sent into a channel which then lost the
    /// message
    ///
    ///
    AttacheeLost,
}

// TODO: make receiver not clonable any more? or make IntoReceiver? idk
// TODO: better convenience methods on error types
// TODO: sender/receiver count increment/decrement for futures
// TODO: attachee lost for cancellation too

/// Error for trying to send a message into a channel immediately or within a deadline
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct TrySendError<T> {
    /// The message that could not be sent
    pub message: T,
    /// The reason the message could not be sent
    pub reason: TrySendErrorReason,
}

/// Reason for a [`TrySendError`] occurring
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum TrySendErrorReason {
    /// The send operation could not be completed immediately / by the deadline
    NotReady,
    /// All receiver handles have been dropped
    ///
    /// Any subsequent attempts to use the same channel will yield the same error.
    NoReceivers,
    /// This or some other sender handle was used to cancel the channel
    ///
    /// Any subsequent attempts to use the same channel will yield the same error.
    Cancelled,
    /// The channel's network connection was lost
    ///
    /// Any subsequent attempts to use the this or any channel on the same connection will yield
    /// the same error (except individual channels that have already entered some other permanent
    /// error state).
    ConnectionLost,
}

impl<T> TrySendError<T> {
    /// Whether this is a `NotReady` error
    pub fn is_temporary(self) -> bool {
        self.reason.is_temporary()
    }
}

impl TrySendErrorReason {
    /// Whether this is a `NotReady` error
    pub fn is_temporary(self) -> bool {
        self == TrySendErrorReason::NotReady
    }
}

impl<T> From<SendError<T>> for TrySendError<T> {
    fn from(e: SendError<T>) -> Self {
        TrySendError {
            message: e.message,
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

/// Error for trying to receive a message from a channel
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum RecvError {
    /// A sender cancelled the channel
    ///
    /// Any subsequent attempts to use the same channel will yield the same error.
    Cancelled,
    /// The channel's network connection was lost
    ///
    /// Any subsequent attempts to use the this or any channel on the same connection will yield
    /// the same error (except individual channels that have already entered some other permanent
    /// error state).
    ConnectionLost,
}

/// Error for trying to receive a message from a channel immediately or within a deadline
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum TryRecvError {
    /// The receive operation could not be completed immediately / by the deadline
    NotReady,
    /// A sender cancelled the channel
    ///
    /// Any subsequent attempts to use the same channel will yield the same error.
    Cancelled,
    /// The channel's network connection was lost
    ///
    /// Any subsequent attempts to use the this or any channel on the same connection will yield
    /// the same error (except individual channels that have already entered some other permanent
    /// error state).
    ConnectionLost,
}

impl TryRecvError {
    /// Whether this is a `NotReady` error
    pub fn is_temporary(self) -> bool {
        self == TryRecvError::NotReady
    }
}

impl From<RecvError> for TryRecvError {
    fn from(e: RecvError) -> Self {
        match e {
            RecvError::Cancelled => TryRecvError::Cancelled,
            RecvError::ConnectionLost => TryRecvError::ConnectionLost,
        }
    }
}

/// place the channel into the given permanently errored state, notify both the send and receive
/// futures at the front of the waiter queues if they exist, and clear the elements, unless it's
/// already in some other errored state
fn enter_errored_state<T>(channel: &Channel<T>, error: SendErrorReason) {
    channel.lock_mutate(|queue, meta| {
        if meta.error.is_none() {
            *queue = SegQueue::new();
            meta.error = Some(error);
            (true, true)
        } else {
            (false, false)
        }
    });
}

impl<T> NonBlockingSender<T> {
    /// Send a message immediately
    pub fn send(&self, message: T) -> Result<(), SendError<T>> {
        dont_block_on(&mut SendFut {
            waiter: self.0.push_send_node(),
            elem: Some(message),
        }).unwrap()
    }

    /// Cancel the channel, abandoning the delivery of all messages, even currently queued ones
    ///
    /// This will trigger subsequent and pending send and receive operations to return a
    /// `Cancelled` error and all queued messages to be dropped as soon as possible without even
    /// delivering them. If networked (and reliable), this is done by QUIC stream cancellation,
    /// which takes advantages of QUIC's transport-level ability to abruptly abandon the
    /// (re-)transmission of a stream's data.
    ///
    /// Contrast to simply dropping all senders for this channel, which will cause the stream to
    /// be "finished"--a more graceful form of termination in which existing queued data will
    /// continue to be buffered and (re-)transmitted (if reliable) until delivered (unless the
    /// connection as a whole fails), after which receivers will return `Ok(None)` rather than
    /// `Err`. Conversely, cancellation is more-so designed for aborting operations.
    pub fn cancel(&self) {
        enter_errored_state(&self.0, SendErrorReason::Cancelled);
    }
}

impl<T> BlockingSender<T> {
    /// Send a message, awaiting until back-pressure permits it
    pub fn send(&self, message: T) -> SendFut<T> {
        SendFut {
            waiter: self.0.push_send_node(),
            elem: Some(message),
        }
    }

    /// Send a message, awaiting until back-pressure permits it or the timeout elapses
    ///
    /// Uses tokio's timer system.
    pub fn send_timeout(&self, message: T, timeout: Duration) -> SendTimeoutFut<T> {
        SendTimeoutFut {
            send: self.send(message),
            timeout: sleep(timeout),
        }
    }

    /// Send a message, awaiting until back-pressure permits it or the deadline is reached
    ///
    /// Uses tokio's timer system.
    ///
    /// Even if the deadline is in the past, the returned future will still attempt sending, and
    /// resolve to the result of that attempt if sending would not block at all.
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
                message: fut.cancel().unwrap(),
                reason: TrySendErrorReason::NotReady,
            }))
    }

    /// Send a message, blocking until back-pressure permits it or the deadline is reached
    ///
    /// Even if the deadline is in the past, this will still attempt sending, and return the result
    /// of that attempt if sending would not block at all.
    pub fn send_blocking_deadline(
        &self,
        message: T,
        deadline: Instant,
    ) -> Result<(), TrySendError<T>> {
        self.send_blocking_timeout(message, deadline.saturating_duration_since(Instant::now()))
    }

    /// Send a message immediately if back-pressure permits it without blocking
    pub fn try_send(&self, message: T) -> Result<(), TrySendError<T>> {
        let mut fut = self.send(message);
        dont_block_on(&mut fut)
            .map(|result| result.map_err(From::from))
            .unwrap_or_else(|| Err(TrySendError {
                message: fut.cancel().unwrap(),
                reason: TrySendErrorReason::NotReady,
            }))
    }

    /// Cancel the channel, abandoning the delivery of all messages, even currently queued ones
    ///
    /// This will trigger subsequent and pending send and receive operations to return a
    /// `Cancelled` error and all queued messages to be dropped as soon as possible without even
    /// delivering them. If networked (and reliable), this is done by QUIC stream cancellation,
    /// which takes advantages of QUIC's transport-level ability to abruptly abandon the
    /// (re-)transmission of a stream's data.
    ///
    /// Contrast to simply dropping all senders for this channel, which will cause the stream to
    /// be "finished"--a more graceful form of termination in which existing queued data will
    /// continue to be buffered and (re-)transmitted (if reliable) until delivered (unless the
    /// connection as a whole fails), after which receivers will return `Ok(None)` rather than
    /// `Err`. Conversely, cancellation is more-so designed for aborting operations.
    pub fn cancel(&self) {
        enter_errored_state(&self.0, SendErrorReason::Cancelled);
    }
}

impl<T> Receiver<T> {
    /// Receive a message, awaiting until one is available
    pub fn recv(&self) -> RecvFut<T> {
        RecvFut(self.0.push_recv_node())
    }

    /// Receive a message, awaiting until one is available or the timeout elapses
    ///
    /// Uses tokio's timer system.
    pub fn recv_timeout(&self, timeout: Duration) -> RecvTimeoutFut<T> {
        RecvTimeoutFut {
            recv: self.recv(),
            timeout: sleep(timeout),
        }
    }

    /// Receive a message, awaiting until one is available or the deadline is reached
    ///
    /// Uses tokio's timer system.
    ///
    /// Even if the deadline is in the past, the returned future will still attempt receiving, and
    /// resolve to the result of that attempt if receiving would not block at all.
    pub fn recv_deadline(&self, deadline: Instant) -> RecvTimeoutFut<T> {
        self.recv_timeout(deadline.saturating_duration_since(Instant::now()))
    }

    /// Receive a message, blocking until one is available
    pub fn recv_blocking(&self) -> Result<Option<T>, RecvError> {
        block_on(&mut self.recv())
    }

    /// Receive a message, blocking until one is available or the timeout elapses
    pub fn recv_blocking_timeout(&self, timeout: Duration) -> Result<Option<T>, TryRecvError> {
        block_on_timeout(&mut self.recv(), timeout)
            .map(|result| result.map_err(From::from))
            .unwrap_or_else(|| Err(TryRecvError::NotReady))
    }

    /// Receive a message, blocking until one is available or the deadline is reached
    ///
    /// Even if the deadline is in the past, this will still attempt receiving, and return the
    /// result of that attempt if receiving would not block at all.
    pub fn recv_blocking_deadline(&self, deadline: Instant) -> Result<Option<T>, TryRecvError> {
        self.recv_blocking_timeout(deadline.saturating_duration_since(Instant::now()))
    }

    /// Receive a message immediately if one is available without blocking
    pub fn try_recv(&self) -> Result<Option<T>, TryRecvError> {
        dont_block_on(&mut self.recv())
            .map(|result| result.map_err(From::from))
            .unwrap_or_else(|| Err(TryRecvError::NotReady))
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
                        message: this.elem.take().expect("SendFut polled after cancelled"),
                        reason,
                    };
                    return (Some(Err(error)), false);
                }

                let has_capacity = match meta.semantics {
                    Some(Semantics::Reliable { bound: Some(bound), .. }) => queue.len() < bound,
                    Some(Semantics::Reliable { bound: None, .. }) => true,
                    Some(Semantics::Unreliable { bound: Some(bound) }) => {
                        // bounded unreliable eviction
                        // TODO: trigger attachee lost
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

/// Future for sending a message into a channel with a time-based deadline
///
/// Uses tokio's timer system.
///
/// Blocks all send futures on the same channel created after this future until this future either
/// resolves or is dropped or cancelled. If this future is dropped or cancelled without resolving,
/// it is guaranteed that it will have no effect on the channel state.
pub struct SendTimeoutFut<T> {
    send: SendFut<T>,
    timeout: Sleep,
}

impl<T> SendTimeoutFut<T> {
    /// Cancel this send operation if it has not already resolved
    ///
    /// If and only if this future has not returned `Poll::Ready`, this returns `Some` with the
    /// message which would have been sent, and unblocks the next send operation. Attempting to
    /// `poll` this future after calling this method will panic.
    ///
    /// This has the same effect as dropping this future, except it doesn't require ownership of
    /// the future to be consumed and it gives back the message.
    pub fn cancel(&mut self) -> Option<T> {
        self.send.cancel()
    }
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
                    message: this.send.cancel().unwrap(),
                    reason: TrySendErrorReason::NotReady,
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

/// Future for receiving a message from a channel with a time-based deadline
///
/// Uses tokio's timer system.
///
/// Resolves to `Ok(None)` if the channel finished gracefully and all messages in it have been
/// taken.
///
/// Blocks all send futures on the same channel created after this future until this future either
/// resolves or is dropped or cancelled. If this future is dropped or cancelled without resolving,
/// it is guaranteed that it will have no effect on the channel state.
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
                return Poll::Ready(Err(TryRecvError::NotReady));
            }

            Poll::Pending
        }
    }
}

impl<T> RecvTimeoutFut<T> {
    /// Cancel this recv operation if it has not already resolved
    ///
    /// If and only if this future has not returned `Poll::Ready`, this unblocks the next recv
    /// operation and returns true. Attempting to `poll` this future after calling this method
    /// will panic.
    ///
    /// This has the same effect as dropping this future, except it doesn't require ownership of
    /// the future to be consumed.
    pub fn cancel(&mut self) -> bool {
        self.recv.cancel()
    }
}

impl<T> Clone for BlockingSender<T> {
    fn clone(&self) -> Self {
        self.0.atomic_meta().sender_count.fetch_add(1, Ordering::Relaxed);
        Self(self.0.clone())
    }
}

impl<T> Clone for NonBlockingSender<T> {
    fn clone(&self) -> Self {
        self.0.atomic_meta().sender_count.fetch_add(1, Ordering::Relaxed);
        Self(self.0.clone())
    }
}

impl<T> Clone for Receiver<T> {
    fn clone(&self) -> Self {
        self.0.atomic_meta().receiver_count.fetch_add(1, Ordering::Relaxed);
        Self(self.0.clone())
    }
}

/// decrement the channel's sender count, and if this causes it to reach zero, notify the next
/// receive future, unless the channel is already in an errored state
fn decrement_sender_count<T>(channel: &Channel<T>) {
    if channel.atomic_meta().sender_count.fetch_sub(1, Ordering::Relaxed) == 1 {
        channel.lock_mutate(|_, meta| (false, meta.error.is_none()));
    }
}

impl<T> Drop for IntoSender<T> {
    fn drop(&mut self) {
        decrement_sender_count(&self.0)
    }
}

impl<T> Drop for BlockingSender<T> {
    fn drop(&mut self) {
        decrement_sender_count(&self.0)
    }
}

impl<T> Drop for NonBlockingSender<T> {
    fn drop(&mut self) {
        decrement_sender_count(&self.0)
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        // decrement the channel's receiver count, and if this causes it to reach zero, notify the next
        // send future, put the channel into a `NoReceivers` errored state, and clear the elements,
        // unless the channel is already in an errored state
        if self.0.atomic_meta().receiver_count.fetch_sub(1, Ordering::Relaxed) == 1 {
            self.0.lock_mutate(|queue, meta| {
                if meta.error.is_none() {
                    *queue = SegQueue::new();
                    meta.error = Some(SendErrorReason::NoReceivers);
                    (false, true)
                } else {
                    (false, false)
                }
            })
        }
    }
}
