//! # Aqua: An experimental network protocol based on sending streams within streams.
//!
//! #### Background: Streams within streams
//!
//! It's a common pattern to send messages within channels which contain handles to additional
//! channels. See _Actors with Tokio_ by Alice Rhyl [[1]] or _Incorrect Reification: An
//! Anti-Pattern_ by myself [[2]]. For example, the pattern of a request/response server can be
//! implemented by having an mpsc channel of request messages going to a processor thread-like
//! wherein each request contains a oneshot sender for sending back the response.
//!
//! [1]: https://ryhl.io/blog/actors-with-tokio/
//! [2]: https://phoenixkahlo.com/articles/incorrect-reification
//!
//! ```
//! use tokio::sync::{oneshot, mpsc};
//! 
//! #[tokio::main]
//! async fn main() {
//!     let (send_req, mut recv_req) = mpsc::channel::<(i32, oneshot::Sender<i32>)>(4096);
//!     tokio::spawn(async move { // spawn server-like task
//!         while let Some((input, send_res)) = recv_req.recv().await {
//!             send_res.send(input * 2); // respond to request with input times two
//!         }
//!     });
//!     let (send_res, recv_res) = oneshot::channel();
//!     send_req.send((5, send_res)).await.unwrap(); // send request
//!     assert_eq!(recv_res.await.unwrap(), 10) // get response; 5 * 2 == 10
//! }
//! ```
//!
//! #### Background: QUIC
//!
//! The QUIC transport protocol [[3]] is an increasingly adopted network protocol which, unlike
//! TCP, provides the ability to create multiple byte streams within a connection and control them
//! in fine-grained ways, and other useful features such as unreliable datagram transmission.
//! However, QUIC provides little in the way of coordinating the lifecycles of streams relative to
//! each other, leaving that to be implemented by layers above it.
//!
//! [3]: https://en.wikipedia.org/wiki/QUIC
//!
//! #### Aqua: What is Aqua?
//!
//! Aqua is an experimental network protocol designed to facilitate network programming in the same
//! pattern of sending streams within streams as one would within a single process. Network streams
//! to other endpoints can be opened up and messages can be sent down them. However, instead of
//! these messages consisting solely of their binary/text payload, they can also have various
//! sender / receiver handles for newly opened network streams in the same connection associated as
//! attachments.
//!
//! By combining this with a custom serde-like serialization system which knows not only how to
//! encode/decode the payload representation of messages but also how to talk to the aqua API to
//! attach / take attached streams, we gain the ability to seamlessly create a channel, put the 
//! sender or receiver half in a struct, and send that struct down another channel, and have this
//! work even if that channel is transmitting its data across a network boundary.
//!
//! ![aqua](/home/phoenix/repos/aqua/assets/aqua.drawio.png)
//!
//! Moreover, it's possible to use this crate's networkable channel types in a non-networked way,
//! within the same process, as if they were just normal async channels. This allows us to write
//! code which is highly generic over whether different sub-tasks are running in the same process
//! or whether they are communicating with each other over a network.
//!
//! #### Aqua: Using the API
//!
//! This library essentially has three entry-points: creating a channel, creating a server, and
//! creating a client.
//!
//! > **Channel**
//!
//! The [`channel`] function is used to create a connected pair of [`IntoSender`] and [`Receiver`].
//! The [`IntoSender`] can be converted into one of various kinds of senders with their own APIs
//! and guarantees, whereas the [`Receiver`] can be used directly without further conversion.
//!
//! Either the [`IntoSender`] half or the [`Receiver`] half can be sent across a network boundary,
//! or they can both be used within the local process, but attempting to send both across a network
//! boundary will error, even if it is the same network boundary. It is also illegal to send part
//! of a channel across a network boundary if it was already received from across a network
//! boundary, or if it otherwise originated from somewhere other than a local call to [`channel`].
//!
//! There are also one-shot channels created by the [`oneshot`] function, which are designed to
//! convey exactly one message in their lifespan (like [tokio's `oneshot` module][4]). These are
//! essentially a simple case of reliable channels.
//!
//! [4]: https://docs.rs/tokio/latest/tokio/sync/oneshot/index.html
//!
//! > **Server**
//!
//! (TODO provisional API) See [`server`].
//!
//! > **Client**
//!
//! (TODO provisional API) See [`client`].

pub use anyhow::Error;


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
    todo!()
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
pub struct IntoSender<T> {
    _p: std::marker::PhantomData<T>
}

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
    pub fn into_ordered_bounded(buffer_num_msgs: usize) -> BlockingSender<T> {
        todo!()
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
    pub fn into_ordered_unbounded() -> NonBlockingSender<T> {
        todo!()
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
    pub fn into_unordered_bounded(buffer_num_msgs: usize) -> BlockingSender<T> {
        todo!()
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
    pub fn into_unordered_unbounded() -> NonBlockingSender<T> {
        todo!()
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
    /// data, such as the channel being closed or aborted.
    ///
    /// [1]: https://datatracker.ietf.org/doc/html/rfc9221
    pub fn into_unreliable() -> NonBlockingSender<T> {
        todo!()
    }

    /// Abort the channel, causing the receiver half to yield an `Aborted` error.
    ///
    /// Contrast to simply dropping the `IntoSender`, which results in the receiver yielding
    /// `Ok(None)`.
    pub fn abort(self) {
        todo!()
    }
}

/// Sender half of a networkable stream that blocks for backpressure.
pub struct BlockingSender<T> {
    _p: std::marker::PhantomData<T>
}

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
    /// The sender half aborted the channel.
    Aborted,
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
    /// The sender half aborted the channel.
    Aborted,
    /// The encompassing network connection was lost before the channel closed otherwise.
    ConnectionLost,
}

impl<T> BlockingSender<T> {
    /// Send a value, waiting until backpressure allows for it.
    pub async fn send(&self, value: T) -> Result<(), SendError<T>> {
        todo!()
    }

    /// Send a value, blocking until backpressure allows for it.
    pub fn send_blocking(&self, value: T) -> Result<(), SendError<T>> {
        todo!()
    }

    /// Try to send a value now if backpressure allows for it.
    pub fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
        todo!()
    }

    /// Whether the receiver half of this channel is across a network boundary.
    pub fn is_networked(&self) -> bool {
        todo!()
    }

    /// Abort the channel, causing the receiver half to yield an `Aborted` error ASAP.
    ///
    /// This causes the sender half to give up on transmitting buffered data (if the channel is
    /// networked), and instructs the receiver half to switch to yielding `Aborted` errors
    /// immediately rather than continuing to deliver buffered values to the application. 
    ///
    /// Contrast to simply dropping all the sender handles to a channel, which results in the
    /// sender making a best effort to continue to transmit all buffered data until it is fully
    /// received (if the channel is networked), and the receiver continuing to yield all values to
    /// the application before finally yielding `Ok(None)`.
    pub fn abort(&self) {
        todo!()
    }
}

impl<T> Clone for BlockingSender<T> {
    fn clone(&self) -> Self {
        todo!()
    }
}

/// Sender half of a networkable stream that does not block for backpressure.
pub struct NonBlockingSender<T> {
    _p: std::marker::PhantomData<T>
}

impl<T> NonBlockingSender<T> {
    /// Send a value. Never blocks.
    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        todo!()
    }

    /// Abort the channel, causing the receiver half to yield an `Aborted` error ASAP.
    ///
    /// This causes the sender half to give up on transmitting buffered data (if the channel is
    /// networked), and instructs the receiver half to switch to yielding `Aborted` errors
    /// immediately rather than continuing to deliver buffered values to the application. 
    ///
    /// Contrast to simply dropping all the sender handles to a channel, which results in the
    /// sender making a best effort to continue to transmit all buffered data until it is fully
    /// received (if the channel is networked and the sender was created in a way that configures
    /// it to be reliable), and the receiver continuing to yield all values to the application
    /// before finally yielding `Ok(None)`.
    pub fn abort(&self) {
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
pub struct Receiver<T> {
    _p: std::marker::PhantomData<T>
}

/// Error returned by [`Receiver::recv`][crate::Receiver::recv] and similar.
pub enum RecvError {
    /// The sender half aborted the channel.
    Aborted,
    /// The encompassing network connection was lost before the channel closed otherwise.
    ConnectionLost,
}

/// Error returned by [`Receiver::try_recv`][crate::Receiver::try_recv].
pub enum TryRecvError {
    /// There is not currently a value available (although there may be in the future).
    Empty,
    /// The sender half aborted the channel.
    Aborted,
    /// The encompassing network connection was lost before the channel closed otherwise.
    ConnectionLost,
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


/// Create a networkable one-shot channel: a channel that conveys only a single message.
///
/// Either the [`OneshotSender`] half or the [`OneshotReceiver`] half can be sent across a network
/// boundary, or they can both be used within the local process, but attempting to send both across
/// a network boundary will error, even if it is the same network boundary.
pub fn oneshot<T>() -> (OneshotSender<T>, OneshotReceiver<T>) {
    todo!()
}

/// Sender half of a networkable oneshot channel.
///
/// One may consume this to send a message to the receiver. Alternatively, one may send this across
/// a network boundary, so long as the corresponding [`OneshotReceiver`] stays in the local
/// process. Once the remote process receives the `OneshotSender` it can use it to send a message
/// back to the receiver handle in the local process.
pub struct OneshotSender<T> {
    _p: std::marker::PhantomData<T>
}

/// Error returned by [`OneshotSender::send`][crate::OneshotSender::send].
pub struct OneshotSendError<T> {
    /// Value that failed to send.
    pub value: T,
    /// Reason why value failed to send.
    pub reason: OneshotSendErrorReason,
}

/// See [`OneshotSendError`].
pub enum OneshotSendErrorReason {
    /// The receiver half was dropped.
    Closed,
    /// The encompassing network connection was lost before the channel closed otherwise.
    ConnectionLost,
}

impl<T> OneshotSender<T> {
    /// Send a value. Never blocks.
    pub fn send(self, value: T) -> Result<(), OneshotSendError<T>> {
        todo!()
    }

    /// Whether the receiver half of this channel is across a network boundary.
    pub fn is_networked(&self) -> bool {
        todo!()
    }
}

/// Receiver half of a networkable oneshot channel.
///
/// One may use this to try to receive a message from the sender. Alternatively, one may send this
/// across a network boundary, so long as the corresponding [`OneshotSender`] stays in the local
/// process. Once the remote process receives the `OneshotReceiver` it can use it to receive a
/// message sent from the local sender half.
pub struct OneshotReceiver<T> {
    _p: std::marker::PhantomData<T>
}

/// Error returned by [`OneshotReceiver::recv`][crate::OneshotReceiver::recv] and similar.
pub enum OneshotRecvError {
    /// The sender half was dropped without sending a message.
    Closed,
    /// The encompassing network connection was lost before the channel closed otherwise.
    ConnectionLost,
}

/// Error returned by [`OneshotReceiver::try_recv`][crate::OneshotReceiver::try_recv] and similar.
pub enum OneshotTryRecvError<T> {
    /// There is not currently a value available (although there may be in the future).
    Empty(OneshotReceiver<T>),
    /// The sender half was dropped without sending a message.
    Closed,
    /// The encompassing network connection was lost before the channel closed otherwise.
    ConnectionLost,
}

impl<T> OneshotReceiver<T> {
    /// Receive the message, waiting until it is available.
    pub async fn recv(self) -> Result<T, OneshotRecvError> {
        todo!()
    }

    /// Receive the message, blocking until it is available.
    pub fn recv_blocking(self) -> Result<T, OneshotRecvError> {
        todo!()
    }

    /// Try to receive the message without waiting.
    pub fn try_recv(self) -> Result<T, OneshotTryRecvError<T>> {
        todo!()
    }

    /// Whether the sender half of this channel is across a network boundary.
    pub fn is_networked(&self) -> bool {
        todo!()
    }
}


// TODO: the API below is provisional and should be enhanced when the time is right


/// (TODO provisional API) Like `serde_json::Value` but with aqua senders and receivers.
pub enum Value {
    Null,
    Bool(bool),
    Number(serde_json::Number),
    String(String),
    Array(Vec<Value>),
    Object(std::collections::BTreeMap<String, Value>),
    Sender(IntoSender<Value>),
    Receiver(Receiver<Value>),
    OneshotSender(OneshotSender<Value>),
    OneshotReceiver(OneshotReceiver<Value>),
}

/// (TODO provisional API) Bind to the socket address. Get a receiver of all root messages received
/// from all clients all interleaved together.
pub fn server(bind_to: &str) -> Result<Receiver<Value>, Error> {
    todo!()
}

/// (TODO provisional API) Connect to the server at the socket address. Get a sender of root
/// messages to it. No authentication.
pub fn client(connect_to: &str) -> Result<IntoSender<Value>, Error> {
    todo!()
}
