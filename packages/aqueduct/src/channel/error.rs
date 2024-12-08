// channel error types.


// ==== base error types ====


/// Error for trying to send into a channel for which all receivers have been dropped
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct NoReceiversError;

/// Error for trying to use a channel which a sender has cancelled
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct CancelledError;

/// Error for trying to use a networked channel for which the encompassing network connection has
/// been lost
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ConnectionLostError;

/// Error for trying to use a channel which was sent through another networked channel in a message
/// that will never be delivered
///
/// This may occur in situations such as:
///
/// - Half of this channel was sent through another networked, unreliable channel, and the message
///   it was sent in was lost due to congestion.
/// - Half of this channel was sent through another networked channel, and then that channel was
///   cancelled before the message it was sent in was received by the remote side.
///
/// This is not designed to cover cases where the relevant encompassing network connection failed
/// as a whole--see [`ConnectionLostError`].
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ChannelLostInTransitError;

/// Error for attempting to use a channel with no or limited blocking, and the operation not
/// completing immediately or by the specified deadline 
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct WouldBlockError;


// ==== compound error types ====


/// Error for trying to send into a channel
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct SendError<T, E> {
    /// The message that could not be sent
    pub msg: T,
    /// The reason the message could not be sent
    pub cause: E,
}

macro_rules! compound_from {
    ($compound:ident {$(
        $variant:ident($inner:ty),
    )*})=>{$(
        impl From<$inner> for $compound {
            fn from(inner: $inner) -> Self {
                Self::$variant(inner)
            }
        }
    )*};
}

/// Terminal error state for trying to send into a channel
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum SendErrorCause {
    /// All receivers handles have been dropped
    NoReceivers(NoReceiversError),
    /// This or another sender handle was used to cancel the channel
    Cancelled(CancelledError),
    /// The encompassing network connection was lost
    ConnectionLost(ConnectionLostError),
    /// The receiver half of the channel was sent through another networked channel in a message
    /// that will never be delivered
    ChannelLostInTransit(ChannelLostInTransitError),
}

compound_from!(SendErrorCause {
    NoReceivers(NoReceiversError),
    Cancelled(CancelledError),
    ConnectionLost(ConnectionLostError),
    ChannelLostInTransit(ChannelLostInTransitError),
});

/// Error for trying to send into a channel with no or limited blocking
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum TrySendErrorCause {
    /// The senders of the channel have entered some terminal error state
    Terminal(SendErrorCause),
    /// The operation could not be resolved immediately or by the specified deadline
    WouldBlock(WouldBlockError),
}

compound_from!(TrySendErrorCause {
    Terminal(SendErrorCause),
    WouldBlock(WouldBlockError),
});

/// Terminal error state for trying to receive from a channel
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum RecvError {
    /// A sender handle was used to cancel the channel
    Cancelled(CancelledError),
    /// The encompassing network connection was lost
    ConnectionLost(ConnectionLostError),
    /// The sender half of the channel was sent through another networked channel in a message that
    /// will never be delivered
    ChannelLostInTransit(ChannelLostInTransitError),
}

compound_from!(RecvError {
    Cancelled(CancelledError),
    ConnectionLost(ConnectionLostError),
    ChannelLostInTransit(ChannelLostInTransitError),
});

/// Error for trying to receive from a channel with no or limited blocking
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum TryRecvError {
    /// The receivers of the channel have entered some terminal error state
    Terminal(RecvError),
    /// The operation could not be resolved immediately or by the specified deadline
    WouldBlock(WouldBlockError),
}

compound_from!(TryRecvError {
    Terminal(RecvError),
    WouldBlock(WouldBlockError),
});

/// Terminal state for trying to receive from a channel
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum RecvTerminalState {
    /// The "finished" state, where all senders have finished and no more buffered messages remain
    Finished,
    /// A terminal error state
    Error(RecvError),
}

compound_from!(RecvTerminalState {
    Error(RecvError),
});
