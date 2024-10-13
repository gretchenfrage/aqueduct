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
//! (TODO)
//!
//! > **Client**
//!
//! (TODO)

pub extern crate bytes;

pub mod zero_copy;

mod frame;
mod channel;
mod stuff;
//mod frame;

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
    pub fn send(self, _value: T) -> Result<(), OneshotSendError<T>> {
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

/*
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
*/