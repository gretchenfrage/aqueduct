//! File to contain channel's receiver API

use super::{
    inner::{Channel, RecvWaiter},
    pollster2::{
        DropWakers,
        block_on,
        block_on_timeout,
        dont_block_on,
    },
    seg_queue::SegQueue,
    error::{SendErrorReason, RecvError, TryRecvError},
    Semantics,
};
use std::{
    future::Future,
    pin::Pin,
    sync::atomic::Ordering,
    task::{Context, Poll},
    time::{Duration, Instant},
};
use tokio::time::{sleep, Sleep};

/// A receiver handle to a channel
///
/// Receive operations take effect strictly in the order their futures are created, and also all
/// receiving futures are strictly cancel-safe. This means that receive operation futures or
/// threads parked on a blocking receive operation form a queue, such that when an async receive
/// method is called and returns a future, the future will block all receive futures created
/// afterwards or blocking receive method calls initiated afterwards from resolving with anything
/// other than perhaps a timeout until previous futures are either resolved, dropped, or cancelled
/// by calling their [`cancel`][self::RecvFut::cancel] method.
pub struct Receiver<T>(pub(super) Channel<T>);

/// Future for receiving a message from a channel
///
/// Resolves to `Ok(None)` if the channel finished gracefully and all messages in it have been
/// taken.
///
/// Blocks all send futures on the same channel created after this future until this future either
/// resolves or is dropped or cancelled. If this future is dropped or cancelled without resolving,
/// it is guaranteed that it will have no effect on the channel state.
pub struct RecvFut<T>(RecvWaiter<T>);

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

impl<T> Clone for Receiver<T> {
    fn clone(&self) -> Self {
        self.0.atomic_meta().receiver_count.fetch_add(1, Ordering::Relaxed);
        Self(self.0.clone())
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
