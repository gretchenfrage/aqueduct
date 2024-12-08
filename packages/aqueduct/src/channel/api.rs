// exposed API of channels

use self::future::*;
use super::{
    error::*,
    core,
};
use std::sync::atomic::Ordering::Relaxed;


/// Create a channel
///
/// See [channels docs](crate::docs::ch_1_01_channels).
pub fn channel<T>() -> (IntoSender<T>, IntoReceiver<T>) {
    todo!()
}


/// Unconverted sender half of a channel
///
/// See [channels docs](crate::docs::ch_1_01_channels).
pub struct IntoSender<T>(core::Channel<T>);

impl<T> IntoSender<T> {
    /// Convert into an ordered, reliable, bounded sender
    pub fn into_ordered(self, bound: usize) -> Sender<T> {
        todo!()
    }

    /// Convert into an ordered, reliable, unbounded sender
    pub fn into_ordered_unbounded(self) -> NonBlockingSender<T> {
        todo!()
    }

    /// Convert into an unordered, reliable, bounded sender
    pub fn into_unordered(self, bound: usize) -> Sender<T> {
        todo!()
    }

    /// Convert into an unordered, reliable, unbounded sender
    pub fn into_unordered_unbounded(self) -> NonBlockingSender<T> {
        todo!()
    }

    /// Convert into an unreliable sender
    ///
    /// The send buffer is bounded, but this does not create backpressure, because overflowing the
    /// buffer is handled by dropping the oldest buffered message.
    pub fn into_unreliable(self, bound: usize) -> NonBlockingSender<T> {
        todo!()
    }

    /// Finish the channel (without even converting the sender)
    ///
    /// This causes all receivers to enter the "finished" terminal state, unless they enter some
    /// other terminal state first.
    pub fn finish(self) {
        todo!()
    }

    /// Cancel the channel (without even converting the sender)
    ///
    /// This causes all receivers to enter the [`CancelledError`] terminal state, unless they enter
    /// some other terminal state first.
    pub fn cancel(self) {
        todo!()
    }

    /// Set whether this `IntoSender` automatically cancels the channel if dropped without
    /// converting or finishing
    ///
    /// Defaults to true. If set to false, automatically finishes the channel if dropped without
    /// converting or cancelling.
    pub fn set_cancel_on_drop(&mut self, cancel_on_drop: bool) -> &mut Self {
        todo!()
    }

    /// Ownership-chaining version of [`set_cancel_on_drop`](Self::set_cancel_on_drop)
    pub fn with_cancel_on_drop(self, cancel_on_drop: bool) -> Self {
        todo!()
    }
}


/// Sender handle to a possibly networked channel, with backpressure
///
/// See [channels docs](crate::docs::ch_1_01_channels).
pub struct Sender<T>(core::Channel<T>);

impl<T> Sender<T> {
    /// Create a future to send a message on this channel
    ///
    /// See the API of [`SendFut`], as it is not only a future, but also provides additional
    /// methods, including the API for blocking on a send operation or trying to send immediately.
    pub fn send(&self, msg: T) -> SendFut<T> {
        todo!()
    }

    /// Finish this sender handle
    ///
    /// Once _all_ sender handles to a channel are finished, and all buffered messages have been
    /// received, all receivers enter the "finished" terminal state, unless they enter some other
    /// terminal state first.
    pub fn finish(self) {
        todo!()
    }

    /// Cancel the channel
    ///
    /// This causes all buffered messages to be dropped and all senders and receivers to enter the
    /// [`CancelledError`] terminal state, unless they enter some other terminal state first.
    pub fn cancel(&self) {
        todo!()
    }

    /// Set whether this `Sender` automatically cancels the channel if dropped without finishing
    ///
    /// Defaults to true. If set to false, automatically finishes the channel if dropped without
    /// cancelling.
    pub fn set_cancel_on_drop(&mut self, cancel_on_drop: bool) -> &mut Self {
        todo!()
    }

    /// Ownership-chaining version of [`set_cancel_on_drop`](Self::set_cancel_on_drop)
    pub fn with_cancel_on_drop(self, cancel_on_drop: bool) -> Self {
        todo!()
    }

    /// If the senders of this channel have entered a terminal state, get that terminal state
    ///
    /// If this returns `Some`, all senders for this channel are permanently in that terminal
    /// state, and all attempts to send will return a corresponding error.
    pub fn terminal_state(&self) -> Option<SendErrorCause> {
        todo!()
    }

    // TODO: buffered

    // TODO: debug

    // TODO: downgrade
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        todo!()
    }
}


/// Sender handle to a possibly networked channel, with no backpresure
///
/// See [channels docs](crate::docs::ch_1_01_channels).
pub struct NonBlockingSender<T>(core::Channel<T>);

impl<T> NonBlockingSender<T> {
    /// Send a message on this channel
    ///
    /// Errors are "sticky": If this returns an error, that error has become the terminal state for
    /// all of this channel's senders, and any further send operation will return the same error.
    pub fn send(&self, msg: T) -> Result<(), SendError<T, SendErrorCause>> {
        todo!()
    }

    /// Finish this sender handle
    ///
    /// Once _all_ sender handles to a channel are finished, and all buffered messages have been
    /// received, all receivers enter the "finished" terminal state, unless they enter some other
    /// terminal state first.
    pub fn finish(self) {
        todo!()
    }

    /// Cancel the channel
    ///
    /// This causes all buffered messages to be dropped and all senders and receivers to enter the
    /// [`CancelledError`] terminal state, unless they enter some other terminal state first.
    pub fn cancel(&self) {
        todo!()
    }

    /// Set whether this `Sender` automatically cancels the channel if dropped without finishing
    ///
    /// Defaults to true. If set to false, automatically finishes the channel if dropped without
    /// cancelling.
    pub fn set_cancel_on_drop(&mut self, cancel_on_drop: bool) -> &mut Self {
        todo!()
    }

    /// Ownership-chaining version of [`set_cancel_on_drop`](Self::set_cancel_on_drop)
    pub fn with_cancel_on_drop(self, cancel_on_drop: bool) -> Self {
        todo!()
    }

    /// If the senders of this channel have entered a terminal state, get that terminal state
    ///
    /// If this returns `Some`, all senders for this channel are permanently in that terminal
    /// state, and all attempts to send will return a corresponding error.
    pub fn terminal_state(&self) -> Option<SendErrorCause> {
        todo!()
    }

    // TODO: buffered

    // TODO: debug

    // TODO: downgrade
}

impl<T> Clone for NonBlockingSender<T> {
    fn clone(&self) -> Self {
        todo!()
    }
}

/// Unconverted receiver half of a channel
///
/// See [channels docs](crate::docs::ch_1_01_channels).
pub struct IntoReceiver<T>(core::Channel<T>);

impl<T> IntoReceiver<T> {
    /// Convert into a receiver
    pub fn into_receiver(self) -> Receiver<T> {
        todo!()
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
        todo!()
    }

    /// If the receivers of this channel have entered a terminal state, get that terminal state
    ///
    /// If this returns `Some`, all receivers for this channel are permanently in that terminal
    /// state, and all attempts to send will return a corresponding error.
    pub fn terminal_state(&self) -> Option<SendErrorCause> {
        todo!()
    }

    // TODO: debug

    // TODO: downgrade
}

impl<T> Clone for Receiver<T> {
    fn clone(&self) -> Self {
        todo!()
    }
}

fn send_error(send_state_byte: u8) -> Option<SendErrorCause> {
    if send_state_byte == SendState::Normal as u8 {
        None
    } else if send_state_byte == SendState::NoReceivers as u8 {
        Some(NoReceiversError.into())
    } else if send_state_byte == SendState::Cancelled as u8 {
        Some(CancelledError.into())
    } else if send_state_byte == SendState::ConnectionLost as u8 {
        Some(ConnectionLostError.into())
    } else if send_state_byte == SendState::ChannelLostInTransit as u8 {
        Some(ChannelLostInTransitError.into())
    } else {
        unreachable!("invalid send_state_byte: {}", send_state_byte);
    }
}

// future types for channels.
pub(crate) mod future {
    use super::*;
    use crate::channel::{
        polling::{Timeout, poll},
        core::*,
    };
    use std::{
        task::{Poll, Context},
        future::Future,
        pin::{Pin, pin},
        time::{Duration, Instant},
    };

    /// Future for sending into a [`Sender`](super::Sender)
    pub struct SendFut<T>(SendFutInner<T>);

    pub(crate) enum SendFutInner<T> {
        // may actually involve locking the channel state.
        Core(core::Send<T>),
        // optimization when a terminal state can be read atomically.
        Cheap(SendError<T, SendErrorCause>),
        // already resolved or cancelled.
        Spent,
    }

    impl<T> Future for SendFut<T> {
        type Output = Result<(), SendError<T, SendErrorCause>>;

        fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
            let inner = &mut self.get_mut().0;

            // optimization
            if let &mut Core(ref mut core) = inner {
                if let Some(if core.channel().send_state
            }

            todo!()

            /*
            pin!(&mut self.get_mut().0)
                .poll(cx)
                .map(|result| result
                    .map_err(|(send_state_byte, msg)| SendError {
                        msg,
                        cause: send_error(send_state_byte).unwrap(),
                    }))*/
        }
    }

    /// Future for receiving from a [`Receiver`](super::Receiver)
    pub struct RecvFut<T>(core::Recv<T>);
}
