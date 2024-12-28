// protocol implementation

mod codec;

use self::codec::*;
use crate::channel::*;
use dashmap::DashMap;
use quinn::*;


struct Conn {
    senders: DashMap<ChanId, NetSender>,
    receivers: DashMap<ChanId, NetReceiver>,
}

struct NetSender {
    closed: bool,
    remote_reachable: bool,
}

struct NetReceiver {
    closed: bool,
    remote_reachable: bool,
}
