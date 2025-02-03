
struct Connection {
    side: Side,
    quic_conn: quinn::Connection,
    send_version: Arc<AtomicBool>,
    receive_version: Arc<AtomicBool>,
    receivers: DashMap<ChanIdLocalReceiver, Receiver>,
}

struct Receiver {
    send_ctrl_task_msg: TokioUnboundedSender<ReceiverCtrlTaskMsg>,
    recv_ctrl_task_msg: AtomicTake<TokioUnboundedReceiver<ReceiverCtrlTaskMsg>>>,
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
