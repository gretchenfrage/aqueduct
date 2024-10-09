
pub struct ChannelId(pub u64);

pub struct OneshotChannelId(pub u64);

pub enum AttachmentType {
    Sender,
    Receiver,
    OneshotSender,
    OneshotReceiver,
}

pub enum Attachment {
    Sender(ChannelId),
    Receiver(ChannelId),
    OneshotSender(OneshotChannelId),
    OneshotReceiver(OneshotChannelId),
}

pub enum StreamFrame {
    ClientHello {
        header: Bytes,
    },
    Message {
        destination: ChannelId,
        payload: Bytes,
        attachments: Vec<Attachment>,
    },
    BeginControlStream,
}

pub enum ControlStreamFrame {
    ThingAttached {

    }
}

pub enum HowMsgSent {
    QuicStream {
        zero_rtt: bool,
        quic_stream_id: u64,
    },
    QuicDatagram {
        
    }
}
