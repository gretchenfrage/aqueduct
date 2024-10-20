
use std::{
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
    cell::UnsafeCell,
    mem::MaybeUninit,
};
use tokio::sync::Notify;


// lifecycle state values
const NOT_INIT: u8 = 0;
const NOT_LOCKED: u8 = 1;
const LOCKED: u8 = 2;
const CANCELLED: u8 = 3;
const CONN_DROPPED: u8 = 4;

#[derive(Debug, Copy, Clone)]
#[repr(u8)]
enum LifecycleState {
    NotInit,
    NotLocked,
    Locked,
    Cancelled,
    ConnectionDropped,
}

#[derive(Debug, Copy, Clone)]
#[repr(u8)]
enum NetworkState {
    Local,
    RemoteSender,
    RemoteReceiver,
}

#[derive(Debug, Copy, Clone)]
#[repr(u8)]
enum DeliveryGuarantees {
    Undifferentiated,
    OrderedReliable,
    UnorderedReliable,
    UnorderedUnreliable,
}

struct State<T> {
    send: UnsafeCell<MaybeUninit<flume::Sender<T>>>,
    recv: UnsafeCell<MaybeUninit<flume::Receiver<T>>>,
    lifecycle_state: AtomicU8,
    network_state: AtomicU8,
    delivery_guarantees: AtomicU8,
    notify_init: Notify,
    notify_deinit: Notify,
}


/// Create a networkable channel.
///
/// The [`channel`] function is used to create a connected pair of [`IntoSender`] and [`Receiver`].
/// The [`IntoSender`] can be converted into one of various kinds of senders with their own APIs
/// and guarantees, whereas the [`Receiver`] can be used directly without further conversion.
///
/// Either the [`IntoSender`] half or the [`Receiver`] half can be sent across a network boundary,
/// or they can both be used within the local process, but attempting to send both across a network
/// boundary will error, even if it is the same network boundary.
pub fn channel<T>() -> (IntoSender<T>, Receiver<T>) {
    let state_1 = Arc::new(State {
        send: UnsafeCell::new(MaybeUninit::uninit()),
        recv: UnsafeCell::new(MaybeUninit::uninit()),
        lifecycle_state: AtomicU8::new(LifecycleState::NotInit as u8),
        network_state: AtomicU8::new(NetworkState::Local as u8),
        delivery_guarantees: AtomicU8::new(DeliveryGuarantees::Undifferentiated as u8),
        notify_init: Notify::new(),
        notify_deinit: Notify::new(),
    });
    let state_2 = Arc::clone(&state_1);
    (IntoSender(state_1), Receiver(state_2))
}

/// Sender half of a networkable channel in an "undifferentiated" state.
///
/// When a channel is first created with [`channel`], the sender half is represented by
/// `IntoSender`, which is not yet useable.
///
/// To send messages into the channel from the current process, one calls a method on the
/// `IntoSender` to convert it into some more specific kind of sender with a more specific API
/// and set of guarantees.
///
/// Alternatively, one may send the `IntoSender` across a network boundary, so long as the
/// corresponding [`Receiver`] stays in the local process. Once the remote process receives the
/// `IntoSender`, it can call a method on it to convert it into a more specific kind of sender and
/// use that to send messages back to the receiver handle in the local process.
pub struct IntoSender<T>(Arc<State<T>>);

impl<T> IntoSender<T> {
    /// Become an ordered and bounded sender.
    ///
    /// - **Ordered:**
    ///
    ///   Messages received will be received in the same order they were sent, with no holes.
    /// - **Bounded:**
    ///
    ///   The sender will buffer up to `buffer_num_msgs` messages before blocking due to
    ///   backpressure. Blocking due to backpressure helps bound memory consumption if the receiver
    ///   is slow or even malicious.
    ///
    /// If networked, this will be backed by a single QUIC stream. Since the network may fail at
    /// any time, it is not possible to guarantee that all messages sent will be delivered.
    /// However, so long as the network connection is alive, a best effort will be made to transmit
    /// all sent messages to the receiver. The sequence of received messages is guaranteed to be a
    /// prefix of the sequence of sent messages.
    pub fn into_ordered_bounded(self, buffer_num_msgs: usize) -> BlockingSender<T> {
        self.init(flume::bounded(buffer_num_msgs), DeliveryGuarantees::OrderedReliable);
        BlockingSender(self.0)
    }

    /// Become an ordered and unbounded sender.
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
        self.init(flume::unbounded(), DeliveryGuarantees::OrderedReliable);
        NonBlockingSender(self.0)
    }

    /// Become an unordered (but reliable) and bounded sender.
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
    ///   The sender will buffer up to `buffer_num_msgs` messages before blocking due to
    ///   backpressure. Blocking due to backpressure helps bound memory consumption if the receiver
    ///   is slow or even malicious.
    ///
    /// If networked, this will be backed by a different QUIC stream for each message. Since the
    /// network may fail at any time, it is not possible to guarantee that all messages sent will
    /// be delivered. However, so long as the network connection is alive, a best effort will be
    /// made to transmit all sent messages to the receiver.
    ///
    /// [1]: https://en.wikipedia.org/wiki/Head-of-line_blocking
    pub fn into_unordered_bounded(self, buffer_num_msgs: usize) -> BlockingSender<T> {
        self.init(flume::bounded(buffer_num_msgs), DeliveryGuarantees::UnorderedReliable);
        BlockingSender(self.0)
    }

    /// Become an unordered (but reliable) and unbounded sender.
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
        self.init(flume::unbounded(), DeliveryGuarantees::UnorderedReliable);
        NonBlockingSender(self.0)
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
    /// Sent messages will be transmitted as soon as possible to minimize delay. Overall, this
    /// exists for situations where a message ceases to be useful if it arrives late.
    ///
    /// If networked, this will be backed by QUIC's [Unreliable Datagram Extension][1]--although if
    /// sent messages are too large to fit in a datagram after serialization, they may be sent in
    /// one-off QUIC streams instead. QUIC streams may also be used to reliably convey some control
    /// data, such as the channel being closed or cancelled.
    ///
    /// [1]: https://datatracker.ietf.org/doc/html/rfc9221
    pub fn into_unreliable() -> NonBlockingSender<T> {
        todo!()
    }
    
    fn init(
        &self,
        send_recv: (flume::Sender<T>, flume::Receiver<T>),
        delivery_guarantees: DeliveryGuarantees,
    ) {
        let (send, recv) = send_recv;
        unsafe {
            MaybeUninit::write(&mut *self.0.send.get(), send);
            MaybeUninit::write(&mut *self.0.recv.get(), recv);
        }
        self.0.lifecycle_state.store(LifecycleState::NotLocked as u8, Ordering::Release);
        self.0.delivery_guarantees.store(delivery_guarantees as u8, Ordering::Relaxed);
    }
}

/// Sender half of a networkable stream that blocks for backpressure.
pub struct BlockingSender<T>(Arc<State<T>>);

/// Error returned by [`BlockingSender::send`][crate::BlockingSender::send] and similar.
pub struct SendError<T> {
    /// Value that failed to send.
    pub value: T,
    /// Reason why value failed to send.
    pub reason: SendErrorReason,
}

/// See [`SendError`].
pub enum SendErrorReason {
    /// The receiver half was dropped.
    Closed,
    /// The sender half cancelled the channel.
    Cancelled,
    /// The encompassing network connection was lost before the channel closed otherwise.
    ConnectionLost,
}

/// Error returned by [`BlockingSender::try_send`][crate::BlockingSender::send].
pub struct TrySendError<T> {
    /// Value that failed to send.
    pub value: T,
    /// Reason why value failed to send.
    pub reason: TrySendErrorReason,
}

/// See [`TrySendError`].
pub enum TrySendErrorReason {
    /// Backpressure did not permit a value to be sent at this time.
    Full,
    /// The receiver half was dropped.
    Closed,
    /// The sender half cancelled the channel.
    Cancelled,
    /// The encompassing network connection was lost before the channel closed otherwise.
    ConnectionLost,
}

impl<T> BlockingSender<T> {
    /// Send a value, waiting until backpressure allows for it.
    pub async fn send(&self, _value: T) -> Result<(), SendError<T>> {
        todo!()
    }

    /// Send a value, blocking until backpressure allows for it.
    pub fn send_blocking(&self, _value: T) -> Result<(), SendError<T>> {
        todo!()
    }

    /// Try to send a value now if backpressure allows for it.
    pub fn try_send(&self, _value: T) -> Result<(), TrySendError<T>> {
        todo!()
    }

    /// Whether the receiver half of this channel is across a network boundary.
    pub fn is_networked(&self) -> bool {
        todo!()
    }

    /// Cancel the channel, causing the receiver half to yield an `Cancelled` error ASAP.
    ///
    /// This causes the sender half to give up on transmitting buffered data (if the channel is
    /// networked), and instructs the receiver half to switch to yielding `Cancelled` errors
    /// immediately rather than continuing to deliver buffered values to the application.
    ///
    /// Contrast to simply dropping all the sender handles to a channel, which results in the
    /// sender making a best effort to continue to transmit all buffered data until it is fully
    /// received (if the channel is networked), and the receiver continuing to yield all values to
    /// the application before finally yielding `Ok(None)`.
    pub fn cancel(&self) {
        todo!()
    }
}

impl<T> Clone for BlockingSender<T> {
    fn clone(&self) -> Self {
        todo!()
    }
}

/// Sender half of a networkable stream that does not block for backpressure.
pub struct NonBlockingSender<T>(Arc<State<T>>);

impl<T> NonBlockingSender<T> {
    /// Send a value. Never blocks.
    pub fn send(&self, _value: T) -> Result<(), SendError<T>> {
        todo!()
    }

    /// Cancel the channel, causing the receiver half to yield an `Cancelled` error ASAP.
    ///
    /// This causes the sender half to give up on transmitting buffered data (if the channel is
    /// networked), and instructs the receiver half to switch to yielding `Cancelled` errors
    /// immediately rather than continuing to deliver buffered values to the application.
    ///
    /// Contrast to simply dropping all the sender handles to a channel, which results in the
    /// sender making a best effort to continue to transmit all buffered data until it is fully
    /// received (if the channel is networked and the sender was created in a way that configures
    /// it to be reliable), and the receiver continuing to yield all values to the application
    /// before finally yielding `Ok(None)`.
    pub fn cancel(&self) {
        todo!()
    }
}

impl<T> Clone for NonBlockingSender<T> {
    fn clone(&self) -> Self {
        todo!()
    }
}

/// Receiver half of a networkable channel.
///
/// This type abstracts over multiple different possible kinds of sending strategies and
/// guarantees, such as whether messages sent have guaranteed ordering, guaranteed delivery,
/// bounded buffering, etc. Those details are determined unilaterally by the sender, and do not
/// affect the receiver API.
///
/// A `Receiver` can be sent across a network boundary so long as it originated from a call to
/// `channel` in the current process and the corresponding sender stays in the local process. Once
/// the remote process receives the `Receiver`, it can use it to receive messages sent from the
/// local sender half.
pub struct Receiver<T>(Arc<State<T>>);

/// Error returned by [`Receiver::recv`][crate::Receiver::recv] and similar.
pub enum RecvError {
    /// The sender half cancelled the channel.
    Cancelled,
    /// The encompassing network connection was lost before the channel closed otherwise.
    Disconnected,
}

/// Error returned by [`Receiver::try_recv`][crate::Receiver::try_recv].
pub enum TryRecvError {
    /// There is not currently a value available (although there may be in the future).
    Empty,
    /// The sender half cancelled the channel.
    Cancelled,
    /// The encompassing network connection was lost before the channel closed otherwise.
    Disconnected,
}

impl<T> Receiver<T> {
    /// Receive the next value on this channel, waiting until it is available.
    ///
    /// Yields `None` values if all sender handles were dropped and all messages on the channel
    /// were successfully received--like a fused iterator.
    pub async fn recv(&mut self) -> Result<Option<T>, RecvError> {
        todo!()
    }

    /// Receive the next value on this channel, blocking until it is available.
    ///
    /// Yields `None` values if all sender handles were dropped and all messages on the channel
    /// were successfully received--like a fused iterator.
    pub fn recv_blocking(&mut self) -> Result<Option<T>, RecvError> {
        todo!()
    }

    /// Try to receive the next value on this channel without waiting.
    ///
    /// Yields `None` values if all sender handles were dropped and all messages on the channel
    /// were successfully received--like a fused iterator.
    pub fn try_recv(&mut self) -> Result<Option<T>, TryRecvError> {
        todo!()
    }

    /// Whether the sender half of this channel is across a network boundary.
    pub fn is_networked(&self) -> bool {
        todo!()
    }
}
