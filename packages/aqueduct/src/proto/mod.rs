
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



struct Connection {
    side: Side,
    chan_id_mint: ChanIdMint,
    quic_conn: quinn::Connection,
    must_send_version: Arc<AtomicBool>,
    has_received_version: Arc<AtomicBool>,
    receivers: DashMap<ChanIdLocalReceiver, Receiver>,
}

struct Receiver {
    send_ctrl_task_msg: TokioUnboundedSender<ReceiverCtrlTaskMsg>,
    recv_ctrl_task_msg: AtomicTake<TokioUnboundedReceiver<ReceiverCtrlTaskMsg>>,
    gateway: TokioMutex<ReceiverGateway>,
}

enum ReceiverCtrlTaskMsg {

}

enum ReceiverGateway {
    Buffer(Vec<ReceivedMessage>),
    Object(Box<dyn FnMut(ReceivedMessage) -> Result<()>>),
}

struct ReceivedMessage {

}
