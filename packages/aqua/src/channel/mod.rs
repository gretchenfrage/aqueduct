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
mod error;
mod send;
mod recv;

use self::inner::Channel;
use std::sync::atomic::{AtomicU64, Ordering};
use seg_queue::SegQueue;

pub use self::{
    send::{IntoSender, NonBlockingSender, BlockingSender, SendFut, SendTimeoutFut},
    recv::{Receiver, RecvFut, RecvTimeoutFut},
    error::{SendError, SendErrorReason, TrySendError, TrySendErrorReason, RecvError, TryRecvError},
};


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



// TODO: make receiver not clonable any more? or make IntoReceiver? idk
// TODO: better convenience methods on error types
// TODO: sender/receiver count increment/decrement for futures
// TODO: attachee lost for cancellation too

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

// TODO: move this into the sender module

/// decrement the channel's sender count, and if this causes it to reach zero, notify the next
/// receive future, unless the channel is already in an errored state
fn decrement_sender_count<T>(channel: &Channel<T>) {
    if channel.atomic_meta().sender_count.fetch_sub(1, Ordering::Relaxed) == 1 {
        channel.lock_mutate(|_, meta| (false, meta.error.is_none()));
    }
}
