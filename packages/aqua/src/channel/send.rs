//! File to contain channel's sender API

use super::{
    inner::{Channel, SendWaiter},
    pollster2::{
        DropWakers,
        block_on,
        block_on_timeout,
        dont_block_on,
    },
    error::{SendError, SendErrorReason, TrySendError, TrySendErrorReason},
    Semantics,
    enter_errored_state,
    decrement_sender_count,
};
use std::{
    future::Future,
    pin::Pin,
    sync::atomic::Ordering,
    task::{Context, Poll},
    time::{Duration, Instant},
};
use tokio::time::{sleep, sleep_until, Sleep};


/// Undifferentiated sender representing a channel with not-yet-determined semantics
pub struct IntoSender<T>(pub(super) Channel<T>);

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

/// Future for sending a message into a channel
///
/// Blocks all send futures on the same channel created after this future until this future either
/// resolves or is dropped or cancelled. If this future is dropped or cancelled without resolving,
/// it is guaranteed that it will have no effect on the channel state.
pub struct SendFut<T> {
    waiter: SendWaiter<T>,
    elem: Option<T>,
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

impl<T> Drop for IntoSender<T> {
    fn drop(&mut self) {
        decrement_sender_count(&self.0)
    }
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

impl<T> Clone for NonBlockingSender<T> {
    fn clone(&self) -> Self {
        self.0.atomic_meta().sender_count.fetch_add(1, Ordering::Relaxed);
        Self(self.0.clone())
    }
}

impl<T> Drop for NonBlockingSender<T> {
    fn drop(&mut self) {
        decrement_sender_count(&self.0)
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

impl<T> Clone for BlockingSender<T> {
    fn clone(&self) -> Self {
        self.0.atomic_meta().sender_count.fetch_add(1, Ordering::Relaxed);
        Self(self.0.clone())
    }
}

impl<T> Drop for BlockingSender<T> {
    fn drop(&mut self) {
        decrement_sender_count(&self.0)
    }
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
