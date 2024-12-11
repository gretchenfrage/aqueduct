// exposed API of channels

use self::future::*;
use super::{
    error::*,
    core,
};
use std::{
    sync::atomic::Ordering::Relaxed,
    mem::take,
};


// ==== helper functions for adapting core API to exposed API ====


// cancel the channel, avoiding locking if possible.
fn cancel<T>(channel: &core::Channel<T>) {
    if channel.send_state() == core::SendState::Normal as u8
        || channel.recv_state() == core::RecvState::Normal as u8
    {
        let mut lock = channel.lock();
        lock.set_send_error(core::SendState::Cancelled);
        lock.set_recv_error(core::RecvState::Cancelled);
    }
}

// finish the channel, avoiding locking if possible.
fn finish<T>(channel: &core::Channel<T>) {
    if channel.recv_state() == core::RecvState::Normal as u8 {
        channel.lock().finish();
    }
}

// 1. increment channel send count.
// 2. clone another handle to the channel.
fn clone_sender<T>(channel: &core::Channel<T>) -> core::Channel<T> {
    channel.send_count().fetch_add(1, Relaxed);
    channel.clone()
}

// 1. decrement channel send count.
// 2. if cancel_on_drop is true, cancel the channel.
// 3. if cancel_on_drop is false, and the send count was lowered to 0, finish the channel.
fn drop_sender<T>(channel: &core::Channel<T>, cancel_on_drop: bool) {
    let prev_send_count = channel.send_count().fetch_sub(1, Relaxed);
    if cancel_on_drop {
        cancel(channel);
    } else if prev_send_count == 1 {
        finish(channel);
    }
}

// 1. increment channel recv count.
// 2. clone another handle to the channel.
fn clone_receiver<T>(channel: &core::Channel<T>) -> core::Channel<T> {
    channel.recv_count().fetch_sub(1, Relaxed);
    channel.clone()
}

// 1. decrement channel recv count.
// 2. if the recv count was lowered to 0, set the send state to no receivers, avoiding locking if
//    possible.
fn drop_receiver<T>(channel: &core::Channel<T>) {
    let prev_recv_count = channel.recv_count().fetch_sub(1, Relaxed);
    if prev_recv_count == 1 {
        if channel.send_state() == core::SendState::Normal as u8 {
            channel.lock().set_send_error(core::SendState::NoReceivers);
        }
    }
}

// convert send state byte into typed representation of optional terminal state.
fn send_error(send_state_byte: u8) -> Option<SendErrorCause> {
    if send_state_byte == core::SendState::Normal as u8 {
        None
    } else if send_state_byte == core::SendState::NoReceivers as u8 {
        Some(NoReceiversError.into())
    } else if send_state_byte == core::SendState::Cancelled as u8 {
        Some(CancelledError.into())
    } else if send_state_byte == core::SendState::ConnectionLost as u8 {
        Some(ConnectionLostError.into())
    } else if send_state_byte == core::SendState::ChannelLostInTransit as u8 {
        Some(ChannelLostInTransitError.into())
    } else {
        unreachable!("invalid send_state_byte: {}", send_state_byte);
    }
}

// convert send state byte into typed representation of optional terminal state.
fn recv_terminal_state(recv_state_byte: u8) -> Option<RecvTerminalState> {
    if recv_state_byte == core::RecvState::Normal as u8 {
        None
    } else if recv_state_byte == core::RecvState::Finished as u8 {
        Some(RecvTerminalState::Finished)
    } else if recv_state_byte == core::RecvState::Cancelled as u8 {
        Some(RecvTerminalState::Error(CancelledError.into()))
    } else if recv_state_byte == core::RecvState::ConnectionLost as u8 {
        Some(RecvTerminalState::Error(ConnectionLostError.into()))
    } else if recv_state_byte == core::RecvState::ChannelLostInTransit as u8 {
        Some(RecvTerminalState::Error(ChannelLostInTransitError.into()))
    } else {
        unreachable!("invalid recv_state_byte: {}", recv_state_byte)
    }
}


// ==== the exposed API ====


/// Create a channel
///
/// See [channels docs](crate::docs::ch_1_01_channels).
pub fn channel<T>() -> (IntoSender<T>, IntoReceiver<T>) {
    let channel_1 = core::Channel::new();
    let channel_2 = channel_1.clone();
    let send = IntoSender { channel: channel_1, cancel_on_drop: true };
    let recv = IntoReceiver(channel_2);
    (send, recv)
}

/// Unconverted sender half of a channel
///
/// See [channels docs](crate::docs::ch_1_01_channels).
// TODO: comment in the middle of IntoSender and IntoReceiver docs explaining it better
pub struct IntoSender<T> {
    channel: core::Channel<T>,
    // we don't actually let the user change this. however, removing it would require unsafe code.
    cancel_on_drop: bool,
}

impl<T> IntoSender<T> {
    /// Convert into an ordered, reliable, bounded sender
    pub fn into_ordered(mut self, bound: usize) -> Sender<T> {
        self.channel.lock().set_bound(bound);
        Sender {
            channel: clone_sender(&self.channel),
            cancel_on_drop: take(&mut self.cancel_on_drop),
        }
    }

    /// Convert into an ordered, reliable, unbounded sender
    pub fn into_ordered_unbounded(mut self) -> NonBlockingSender<T> {
        NonBlockingSender {
            channel: clone_sender(&self.channel),
            cancel_on_drop: take(&mut self.cancel_on_drop),
            bound: None,
        }
    }

    /// Convert into an unordered, reliable, bounded sender
    pub fn into_unordered(mut self, bound: usize) -> Sender<T> {
        self.channel.lock().set_bound(bound);
        self.channel.send_count().fetch_add(1, Relaxed);
        Sender {
            channel: clone_sender(&self.channel),
            cancel_on_drop: take(&mut self.cancel_on_drop),
        }
    }

    /// Convert into an unordered, reliable, unbounded sender
    pub fn into_unordered_unbounded(mut self) -> NonBlockingSender<T> {
        self.channel.send_count().fetch_add(1, Relaxed);
        NonBlockingSender {
            channel: clone_sender(&self.channel),
            cancel_on_drop: take(&mut self.cancel_on_drop),
            bound: None,
        }
    }

    /// Convert into an unreliable sender
    ///
    /// The send buffer may be bounded, but this does not create backpressure, because overflowing
    /// the buffer is handled by dropping the oldest buffered message.
    pub fn into_unreliable(mut self, bound: Option<usize>) -> NonBlockingSender<T> {
        self.channel.send_count().fetch_add(1, Relaxed);
        NonBlockingSender {
            channel: clone_sender(&self.channel),
            cancel_on_drop: take(&mut self.cancel_on_drop),
            bound,
        }
    }

    /// Finish the channel (without even converting the sender)
    ///
    /// This causes all receivers to enter the "finished" terminal state, unless they enter some
    /// other terminal state first.
    pub fn finish(mut self) {
        self.cancel_on_drop = false;
        self.channel.lock().finish();
    }

    /// Cancel the channel (without even converting the sender)
    ///
    /// This causes all receivers to enter the [`CancelledError`] terminal state, unless they enter
    /// some other terminal state first.
    pub fn cancel(mut self) {
        self.cancel_on_drop = true;
        drop(self);
    }
}

impl<T> Drop for IntoSender<T> {
    fn drop(&mut self) {
        drop_sender(&self.channel, self.cancel_on_drop);
    }
}


/// Sender handle to a possibly networked channel, with backpressure
///
/// See [channels docs](crate::docs::ch_1_01_channels).
pub struct Sender<T> {
    channel: core::Channel<T>,
    cancel_on_drop: bool,
}

impl<T> Sender<T> {
    /// Create a future to send a message on this channel
    ///
    /// See the API of [`SendFut`], as it is not only a future, but also provides additional
    /// methods, including the API for blocking on a send operation or trying to send immediately.
    pub fn send(&self, msg: T) -> SendFut<T> {
        let send_state = self.channel.send_state();
        if send_state != core::SendState::Normal as u8 {
            return SendFut {
                fut: core::Send::cheap(send_state, msg),
                cancel_on_drop: self.cancel_on_drop,
            };
        }
        let mut lock = self.channel.lock();
        let send_state = self.channel.send_state();
        if send_state != core::SendState::Normal as u8 {
            return SendFut {
                fut: core::Send::cheap(send_state, msg),
                cancel_on_drop: self.cancel_on_drop,
            };
        }
        self.channel.send_count().fetch_add(1, Relaxed);
        SendFut {
            fut: lock.send(msg),
            cancel_on_drop: self.cancel_on_drop,
        }
    }

    /// Finish this sender handle
    ///
    /// Once _all_ sender handles to a channel are finished, and all buffered messages have been
    /// received, all receivers enter the "finished" terminal state, unless they enter some other
    /// terminal state first.
    pub fn finish(mut self) {
        self.cancel_on_drop = false;
        drop(self);
    }

    /// Cancel the channel
    ///
    /// This causes all buffered messages to be dropped and all senders and receivers to enter the
    /// [`CancelledError`] terminal state, unless they enter some other terminal state first.
    pub fn cancel(&self) {
        cancel(&self.channel);
    }

    /// Set whether this `Sender` automatically cancels the channel if dropped without finishing
    ///
    /// Defaults to true. If set to false, automatically finishes this sender handle if dropped
    /// without cancelling. When a sender is cloned, the cloned sender inherits the original
    /// sender's `cancel_on_drop` property--however, `cancel_on_drop` is only a property of
    /// individual sender handles, and changing it for one does not change it for all senders in
    /// the channel.
    pub fn set_cancel_on_drop(&mut self, cancel_on_drop: bool) -> &mut Self {
        self.cancel_on_drop = cancel_on_drop;
        self
    }

    /// Ownership-chaining version of [`set_cancel_on_drop`](Self::set_cancel_on_drop)
    pub fn with_cancel_on_drop(mut self, cancel_on_drop: bool) -> Self {
        self.cancel_on_drop = cancel_on_drop;
        self
    }

    /// If the senders of this channel have entered a terminal state, get that terminal state
    ///
    /// If this returns `Some`, all senders for this channel are permanently in that terminal
    /// state, and all attempts to send will return a corresponding error.
    pub fn terminal_state(&self) -> Option<SendErrorCause> {
        send_error(self.channel.send_state())
    }

    // TODO: buffered, bound

    // TODO: debug

    // TODO: downgrade
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Sender {
            channel: clone_sender(&self.channel),
            cancel_on_drop: self.cancel_on_drop,
        }
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        drop_sender(&self.channel, self.cancel_on_drop);
    }
}


/// Sender handle to a possibly networked channel, with no backpresure
///
/// See [channels docs](crate::docs::ch_1_01_channels).
pub struct NonBlockingSender<T> {
    channel: core::Channel<T>,
    cancel_on_drop: bool,
    bound: Option<usize>,
}

impl<T> NonBlockingSender<T> {
    /// Send a message on this channel
    ///
    /// Errors are "sticky": If this returns an error, that error has become the terminal state for
    /// all of this channel's senders, and any further send operation will return the same error.
    pub fn send(&self, msg: T) -> Result<(), SendError<T>> {
        if let Some(cause) = send_error(self.channel.send_state()) {
            return Err(SendError { msg, cause });
        }
        let mut lock = self.channel.lock();
        if let Some(cause) = send_error(self.channel.send_state()) {
            return Err(SendError { msg, cause });
        }
        if let Some(bound) = self.bound {
            debug_assert!(lock.elems().len() <= bound);
            if lock.elems().len() == bound {
                lock.elems().pop();
            }
        }
        lock.enqueue(msg);
        Ok(())
    }

    /// Finish this sender handle
    ///
    /// Once _all_ sender handles to a channel are finished, and all buffered messages have been
    /// received, all receivers enter the "finished" terminal state, unless they enter some other
    /// terminal state first.
    pub fn finish(mut self) {
        self.cancel_on_drop = false;
        drop(self);
    }

    /// Cancel the channel
    ///
    /// This causes all buffered messages to be dropped and all senders and receivers to enter the
    /// [`CancelledError`] terminal state, unless they enter some other terminal state first.
    pub fn cancel(&self) {
        cancel(&self.channel);
    }

    /// Set whether this `Sender` automatically cancels the channel if dropped without finishing
    ///
    /// Defaults to true. If set to false, automatically finishes this sender handle if dropped
    /// without cancelling. When a sender is cloned, the cloned sender inherits the original
    /// sender's `cancel_on_drop` property--however, `cancel_on_drop` is only a property of
    /// individual sender handles, and changing it for one does not change it for all senders in
    /// the channel.
    pub fn set_cancel_on_drop(&mut self, cancel_on_drop: bool) -> &mut Self {
        self.cancel_on_drop = cancel_on_drop;
        self
    }

    /// Ownership-chaining version of [`set_cancel_on_drop`](Self::set_cancel_on_drop)
    pub fn with_cancel_on_drop(mut self, cancel_on_drop: bool) -> Self {
        self.cancel_on_drop = cancel_on_drop;
        self
    }

    /// If the senders of this channel have entered a terminal state, get that terminal state
    ///
    /// If this returns `Some`, all senders for this channel are permanently in that terminal
    /// state, and all attempts to send will return a corresponding error.
    pub fn terminal_state(&self) -> Option<SendErrorCause> {
        send_error(self.channel.send_state())
    }

    // TODO: buffered, bound

    // TODO: debug

    // TODO: downgrade
}

impl<T> Clone for NonBlockingSender<T> {
    fn clone(&self) -> Self {
        NonBlockingSender {
            channel: clone_sender(&self.channel),
            cancel_on_drop: self.cancel_on_drop,
            bound: self.bound,
        }
    }
}

impl<T> Drop for NonBlockingSender<T> {
    fn drop(&mut self) {
        drop_sender(&self.channel, self.cancel_on_drop);
    }
}


/// Unconverted receiver half of a channel
///
/// See [channels docs](crate::docs::ch_1_01_channels).
pub struct IntoReceiver<T>(core::Channel<T>);

impl<T> IntoReceiver<T> {
    /// Convert into a receiver
    pub fn into_receiver(self) -> Receiver<T> {
        Receiver(clone_receiver(&self.0))
    }
}

impl<T> Drop for IntoReceiver<T> {
    fn drop(&mut self) {
        drop_receiver(&self.0);
    }
}


/// Receiver handle to a possibly networked channel
///
/// See [channels docs](crate::docs::ch_1_01_channels).
pub struct Receiver<T>(core::Channel<T>);

impl<T> Receiver<T> {
    /// Create a future to receive a message from this channel
    ///
    /// See the API of [`RecvFut`], as it is not only a future, but also provides additional
    /// methods, including the API for blocking on a recv operation or trying to recv immediately.
    pub fn recv(&self) -> RecvFut<T> {
        let recv_state = self.0.recv_state();
        if recv_state != core::RecvState::Normal as u8 {
            return RecvFut(core::Recv::cheap(recv_state));
        }
        let mut lock = self.0.lock();
        let recv_state = self.0.recv_state();
        if recv_state != core::RecvState::Normal as u8 {
            return RecvFut(core::Recv::cheap(recv_state));
        }
        self.0.recv_count().fetch_add(1, Relaxed);
        RecvFut(lock.recv())
    }

    /// If the receivers of this channel have entered a terminal state, get that terminal state
    ///
    /// If this returns `Some`, all receivers for this channel are permanently in that terminal
    /// state, and all attempts to send will return a corresponding error.
    pub fn terminal_state(&self) -> Option<RecvTerminalState> {
        recv_terminal_state(self.0.recv_state())
    }

    // TODO: debug

    // TODO: downgrade
}

impl<T> Clone for Receiver<T> {
    fn clone(&self) -> Self {
        Receiver(clone_receiver(&self.0))
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        drop_receiver(&self.0);
    }
}


// future types for channels.
pub(crate) mod future {
    use super::*;
    use crate::channel::{
        polling::{Timeout, poll},
    };
    use std::{
        task::{Poll, Context},
        future::Future,
        pin::{Pin, pin},
        time::{Duration, Instant},
    };

    /// Future for sending into a [`Sender`]
    ///
    /// The message will not be sent until this future resolves (a call to `poll` returns
    /// `Poll::Ready`). If this future has not yet resolved, the send operation may be aborted and
    /// its message retrieved by calling [`rescind`](Self::rescind) (or by dropping).
    ///
    /// Only the most recently created send future for a channel that has not yet resolved,
    /// rescinded, or dropped may successfully resolve. Thus, if one creates a send future and
    /// holds it for an extended period, it may block send futures created after it.
    ///
    /// Errors are "sticky": If this resolves to an error, that error has become the terminal state
    /// for all of this channel's senders, and any further send operation will return the same
    /// error.
    ///
    /// For purposes of reference-counting senders, this future counts as a sender so long as it
    /// still has the potential to resolve. Thus, receivers cannot enter the "finished" state until
    /// all send futures for the channel are dropped, resolved, or rescinded.
    pub struct SendFut<T> {
        pub(super) fut: core::Send<T>,
        pub(super) cancel_on_drop: bool,
    }

    fn map_send_result<T>(
        result: (Result<(), (u8, T)>, Option<core::Channel<T>>)
    ) -> Result<(), SendError<T>> {
        let (result, channel) = result;
        if let Some(channel) = channel {
            drop_sender(&channel, false);
        }
        result
            .map_err(|(send_state_byte, msg)| SendError {
                msg,
                cause: send_error(send_state_byte).unwrap(),
            })
    }

    fn map_try_send_result<T>(
        result: Result<(Result<(), (u8, T)>, Option<core::Channel<T>>), ()>,
    ) -> Result<(), TrySendError<T>> {
        match result {
            Ok(send_result) => map_send_result(send_result).map_err(TrySendError::from),
            Err(()) => Err(WouldBlockError.into()),
        }
    }

    impl<T> Future for SendFut<T> {
        type Output = Result<(), SendError<T>>;

        fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
            let this = self.get_mut();
            // for implementation of FusedFuture
            if this.is_terminated() {
                return Poll::Pending;
            }
            pin!(&mut this.fut)
                .poll(cx)
                .map(map_send_result)
        }
    }

    impl<T> SendFut<T> {
        /// If the message has not yet been sent, abort the send operation and rescind the message,
        /// and possibly cancel the channel
        ///
        /// If this future has not yet resolved or rescinded, this returns `Some`. Moreover, if
        /// this returns `Some` and `cancel` was passed as true, the channel will be cancelled.
        pub fn rescind(&mut self, cancel: bool) -> Option<T> {
            self.fut.cancel()
                .map(|(msg, channel)| {
                    if let Some(channel) = channel {
                        drop_sender(&channel, cancel);
                    }
                    msg
                })
        }

        /// Shorthand for calling [`rescind`](Self::rescind) with `cancel` being true
        ///
        /// If this future has not yet resolved or rescinded, this returns `Some` and cancels the
        /// channel.
        pub fn rescind_cancel(&mut self) -> Option<T> {
            self.rescind(true)
        }

        /// Shorthand for calling [`rescind`](Self::rescind) with `cancel` being false
        ///
        /// If this future has not yet resolved or rescinded, this returns `Some`, and prevents
        /// this future from cancelling the channel when dropped.
        pub fn rescind_finish(&mut self) -> Option<T> {
            self.rescind(false)
        }

        /// Try to resolve this future immediately without blocking
        ///
        /// Calling this method counts as polling this future, and if this method returns anything
        /// other than [`WouldBlockError`], that counts as this future resolving. This method will
        /// panic if this future has already resolved or rescinded.
        pub fn try_now(&mut self) -> Result<(), TrySendError<T>> {
            assert!(!self.is_terminated(), "SendFut.block called after terminated");
            map_try_send_result(poll(&mut self.fut, Timeout::NonBlocking))
        }

        /// Block until this future resolves
        ///
        /// Calling this method counts as polling this future, and when this method returns, that
        /// counts as this future resolving. This method will panic if this future has already
        /// resolved or rescinded.
        pub fn block(&mut self) -> Result<(), SendError<T>> {
            assert!(!self.is_terminated(), "SendFut.block called after terminated");
            let result = poll(&mut self.fut, Timeout::Never)
                .ok().expect("poll timed out with Timeout::Never");
            map_send_result(result)
        }

        /// Block until this future resolves or a timeout elapses
        ///
        /// Calling this method counts as polling this future, and if this method returns anything
        /// other than [`WouldBlockError`], that counts as this future resolving. This method will
        /// panic if this future has already resolved or rescinded.
        pub fn block_timeout(&mut self, timeout: Duration) -> Result<(), TrySendError<T>> {
            assert!(!self.is_terminated(), "SendFut.block called after terminated");
            self.block_deadline(Instant::now() + timeout)
        }

        /// Block until this future resolves or the deadline is reached
        ///
        /// Calling this method counts as polling this future, and if this method returns anything
        /// other than [`WouldBlockError`], that counts as this future resolving. This method will
        /// panic if this future has already resolved or rescinded.
        pub fn block_deadline(&mut self, deadline: Instant) -> Result<(), TrySendError<T>> {
            assert!(!self.is_terminated(), "SendFut.block called after terminated");
            map_try_send_result(poll(&mut self.fut, Timeout::At(deadline)))
        }

        /// Whether this future has already resolved or rescinded
        pub fn is_terminated(&self) -> bool {
            self.fut.is_terminated()
        }

        /// Set whether this `SendFut` automatically cancels the channel if dropped without
        /// resolving or rescinded.
        ///
        /// When a `SendFut` is created, it inherits the [`Sender::cancel_on_drop`][1] property--
        /// however, `cancel_on_drop` is only a property of the individual `SendFut`, and changing
        /// it for this future does not change it for all senders in the channel.
        ///
        /// [1]: Sender::set_cancel_on_drop
        pub fn set_cancel_on_drop(&mut self, cancel_on_drop: bool) -> &mut Self {
            self.cancel_on_drop = cancel_on_drop;
            self
        }

        /// Ownership-chaining version of [`set_cancel_on_drop`](Self::set_cancel_on_drop)
        pub fn with_cancel_on_drop(mut self, cancel_on_drop: bool) -> Self {
            self.cancel_on_drop = cancel_on_drop;
            self
        }
    }

    #[cfg(feature = "futures")]
    impl<T> futures::future::FusedFuture for SendFut<T> {
        fn is_terminated(&self) -> bool {
            Self::is_terminated(self)
        }
    }

    impl<T> Drop for SendFut<T> {
        fn drop(&mut self) {
            // to make sure we trigger drop_sender if necessary
            self.rescind(self.cancel_on_drop);
        }
    }


    /// Future for receiving from a [`Receiver`]
    ///
    /// Resolves to `Ok(None)` to represent the "finished" state--a graceful termination. Once all
    /// sender handles to the channel have finished, and all buffered messages have been received,
    /// the channel may enter the "finished" state.
    ///
    /// A message will not be dequeued from the channel until this future resolves (a call to
    /// `poll` returns `Poll::Ready`). If this future has not yet resolved, the receive operation
    /// may be aborted by calling [`abort`](Self::abort) (or by dropping).
    ///
    /// Only the most recently created receive future for a channel that has not yet resolved,
    /// aborted, or dropped may successfully resolve. Thus, if one creates a receive future and
    /// holds it for an extended period, it may block receive futures created after it.
    ///
    /// Errors, as well the "finished" state represented by `Ok(None)`, are "sticky": If this
    /// resolves to such a value, that value has become the terminal state for all of this
    /// channel's receivers, and any further receive operation will return the same value.
    ///
    /// For purposes of reference-counting receivers, this future counts as a receiver so long as
    /// it still has the potential to resolve. Thus, senders cannot enter the [`NoReceiversError`]
    /// state until all receive futures for the channel are dropped, resolved, or aborted.
    pub struct RecvFut<T>(pub(super) core::Recv<T>);

    fn map_recv_result<T>(
        result: (Result<T, u8>, Option<core::Channel<T>>)
    ) -> Result<Option<T>, RecvError> {
        let (result, channel) = result;
        if let Some(channel) = channel {
            drop_receiver(&channel);
        }
        match result {
            Ok(msg) => Ok(Some(msg)),
            Err(recv_state_byte) => match recv_terminal_state(recv_state_byte).unwrap() {
                RecvTerminalState::Finished => Ok(None),
                RecvTerminalState::Error(error) => Err(error),
            }
        }
    }

    fn map_try_recv_result<T>(
        result: Result<(Result<T, u8>, Option<core::Channel<T>>), ()>
    ) -> Result<Option<T>, TryRecvError> {
        match result {
            Ok(recv_result) => map_recv_result(recv_result).map_err(TryRecvError::from),
            Err(()) => Err(WouldBlockError.into()),
        }
    }

    impl<T> Future for RecvFut<T> {
        type Output = Result<Option<T>, RecvError>;

        fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
            let this = self.get_mut();
            // for implementation of FusedFuture
            if this.is_terminated() {
                return Poll::Pending;
            }
            pin!(&mut this.0)
                .poll(cx)
                .map(map_recv_result)
        }
    }

    impl<T> RecvFut<T> {
        /// Try to abort this receive operation
        ///
        /// This aborts the receive operation, unless this future has already resolved or aborted,
        /// in which case this does nothing. This method never panics.
        pub fn abort(&mut self) {
            if let Some(channel) = self.0.cancel() {
                drop_receiver(&channel);
            }
        }

        /// Block until this future resolves
        ///
        /// Calling this method counts as polling this future, and when this method returns, that
        /// counts as this future resolving. This method will panic if this future has already
        /// resolved or aborted.
        pub fn block(&mut self) -> Result<Option<T>, RecvError> {
            assert!(!self.is_terminated(), "RecvFut.block called after terminated");
            let result = poll(&mut self.0, Timeout::Never)
                .ok().expect("poll timed out with Timeout::Never");
            map_recv_result(result)
        }

        /// Try to resolve this future immediately without blocking
        ///
        /// Calling this method counts as polling this future, and if this method returns anything
        /// other than [`WouldBlockError`], that counts as this future resolving. This method will
        /// panic if this future has already resolved or aborted.
        pub fn try_now(&mut self) -> Result<Option<T>, TryRecvError> {
            assert!(!self.is_terminated(), "RecvFut.block called after terminated");
            map_try_recv_result(poll(&mut self.0, Timeout::NonBlocking))
        }

        /// Block until this future resolves or a timeout elapses
        ///
        /// Calling this method counts as polling this future, and if this method returns anything
        /// other than [`WouldBlockError`], that counts as this future resolving. This method will
        /// panic if this future has already resolved or aborted.
        pub fn block_timeout(&mut self, timeout: Duration) -> Result<Option<T>, TryRecvError> {
            assert!(!self.is_terminated(), "RecvFut.block called after terminated");
            self.block_deadline(Instant::now() + timeout)
        }

        /// Block until this future resolves or the deadline is reached
        ///
        /// Calling this method counts as polling this future, and if this method returns anything
        /// other than [`WouldBlockError`], that counts as this future resolving. This method will
        /// panic if this future has already resolved or aborted.
        pub fn block_deadline(&mut self, deadline: Instant) -> Result<Option<T>, TryRecvError> {
            assert!(!self.is_terminated(), "RecvFut.block called after terminated");
            map_try_recv_result(poll(&mut self.0, Timeout::At(deadline)))
        }

        /// Whether this future has already resolved or aborted
        pub fn is_terminated(&self) -> bool {
            self.0.is_terminated()
        }
    }

    #[cfg(feature = "futures")]
    impl<T> futures::future::FusedFuture for RecvFut<T> {
        fn is_terminated(&self) -> bool {
            Self::is_terminated(self)
        }
    }

    impl<T> Drop for RecvFut<T> {
        fn drop(&mut self) {
            // to make sure we trigger drop_receiver if necessary
            self.abort();
        }
    }
}


// ==== tests ====


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_1000_test() {
        use std::{
            time::Duration,
            thread,
        };

        let (send, recv) = channel();
        let send = send.into_ordered(500);
        let recv = recv.into_receiver();

        let join_1 = thread::spawn(move || {
            for i in 1..=1000 {
                send.send(i).block_timeout(Duration::from_millis(10)).unwrap();
                if i < 1000 && i % 100 == 0 {
                    thread::sleep(Duration::from_millis(50));
                }
            }
            send.finish();
        });
        let join_2 = thread::spawn(move || {
            for i in 1..=1000 {
                let j = recv.recv().block_timeout(Duration::from_millis(60)).unwrap().unwrap();
                assert_eq!(i, j);
            }
            assert!(recv.recv().block_timeout(Duration::from_millis(10)).unwrap().is_none());
        });
        join_1.join().unwrap();
        join_2.join().unwrap();
    }

    // TODO: stochastic testing
}
