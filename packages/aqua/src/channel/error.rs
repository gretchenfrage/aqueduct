//! File to contain channel's error types

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
}

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
