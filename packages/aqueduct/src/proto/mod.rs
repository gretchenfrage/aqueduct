
mod chan_id_mint;

use self::chan_id_mint::ChanIdMint;
use crate::{
    util::atomic_take::AtomicTake,
    frame::common::*,
};
use std::sync::{
    atomic::AtomicBool,
    Arc,
};
use dashmap::DashMap;
use tokio::sync::{
    mpsc::{
        UnboundedSender as TokioUnboundedSender,
        UnboundedReceiver as TokioUnboundedReceiver,
    },
    Mutex as TokioMutex,
};
use anyhow::Result;


/// Top-level shared state of an Aqueduct connection.
struct Connection {
    /// The underlying QUIC connection.
    quic_conn: quinn::Connection,
    /// Whether we're the client or the server.
    side: Side,
    /// For minting new channel IDs.
    chan_id_mint: ChanIdMint,
    /// Whether we must (still) begin all outgoing QUIC streams/datagrams with a Version frame.
    must_send_version: Arc<AtomicBool>,
    /// Whether we have (as of yet) received an entire valid Version frame from the remote side.
    has_received_version: Arc<AtomicBool>,
    /// Receiver state for networked Aqueduct channels.
    receivers: DashMap<ChanIdLocalReceiver, Receiver>,
}

/// Receiver state for a networked Aqueduct channel.
struct Receiver {
    /// Message sender to the local side's channel control task.
    send_ctrl_task_msg: TokioUnboundedSender<ReceiverCtrlTaskMsg>,
    /// Message receiver of messages to the local side's channel control task, if that task does
    /// not yet exist. The local side's channel control task is spawned when the local side
    /// experiences the establishment of the channel control stream:
    ///
    /// - For Aqueduct channels created by the remote side, the local side creates the channel
    ///   control stream at the same time as it creates this Receiver, and thus the channel control
    ///   task is spawned immediately and this AtomicTake is initialized as None.
    /// - For Aqueduct channels create by the local side, the remote side creates the channel
    ///   control stream, and thus there is an asynchronous delay between the local side creating
    ///   this Receiver and the local side observing the establishment of the channel control
    ///   stream. As such, this AtomicTake is initialized as Some, and then its value is taken
    ///   later to be owned by the local side's channel control task.
    recv_ctrl_task_msg: AtomicTake<TokioUnboundedReceiver<ReceiverCtrlTaskMsg>>,
    /// Gateway for giving received messages to application deserialization logic. 
    gateway: TokioMutex<ReceiverGateway>,
}

/// Message to a receiver-side channel control task.
enum ReceiverCtrlTaskMsg {

}

/// Gateway for giving received messages to application deserialization logic.
enum ReceiverGateway {
    /// The application has not yet taken the receiver. The gateway holds a buffer of received
    /// but not yet application-level deserialized messages.
    Buffer(Vec<ReceivedMessage>),
    /// The application has taken the receiver. The dyn object will invoke application-level
    /// deserialization logic on the message, then relay it to the application's Aqueduct channel
    /// handle, or return error on failure or panic in the application-level deserialization logic.
    Object(Box<dyn FnMut(ReceivedMessage) -> Result<()>>),
}

/// Message within a channel that has been fully received on the Aqueduct protocol level, but not
/// yet deserialized at the application application.
struct ReceivedMessage {

}
