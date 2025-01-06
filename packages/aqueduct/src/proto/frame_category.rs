
use crate::proto::read;
use anyhow::*;


// frame that can occur on a unidirectional stream or datagram
pub(crate) enum NonBidiFrame {
    Version(read::Version),
    Message(read::Message),
    ClosedChannelLost(read::ClosedChannelLost),
}

impl NonBidiFrame {
    pub(crate) fn try_from(frame: read::Frame) -> Result<Self> {
        Ok(match frame {
            read::Frame::Version(r) => NonBidiFrame::Version(r),
            read::Frame::Message(r) => NonBidiFrame::Message(r),
            read::Frame::ClosedChannelLost(r) => NonBidiFrame::ClosedChannelLost(r),
            r => bail!("unexpected NonBidiFrame frame: {:?}", r.frame_type()),
        })
    }
}

// frame that can occur in a channel control stream in the sender-to-receiver direction 
pub(crate) enum CtrlFrameToReceiver {
    SentUnreliable(read::SentUnreliable),
    FinishSender(read::FinishSender),
}

impl CtrlFrameToReceiver {
    pub(crate) fn try_from(frame: read::Frame) -> Result<Self> {
        Ok(match frame {
            read::Frame::SentUnreliable(r) => CtrlFrameToReceiver::SentUnreliable(r),
            read::Frame::FinishSender(r) => CtrlFrameToReceiver::FinishSender(r),
            r => bail!("unexpected CtrlFrameToReceiver frame: {:?}", r.frame_type()),
        })
    }
}

// frame that can occur in a channel control stream in the receiver-to-sender direction
pub(crate) enum CtrlFrameToSender {
    AckReliable(read::AckReliable),
    AckNackUnreliable(read::AckNackUnreliable),
    CloseReceiver(read::CloseReceiver),
}

impl CtrlFrameToSender {
    pub(crate) fn try_from(frame: read::Frame) -> Result<Self> {
        Ok(match frame {
            read::Frame::AckReliable(r) => CtrlFrameToSender::AckReliable(r),
            read::Frame::AckNackUnreliable(r) => CtrlFrameToSender::AckNackUnreliable(r),
            read::Frame::CloseReceiver(r) => CtrlFrameToSender::CloseReceiver(r),
            r => bail!("unexpected CtrlFrameToSender frame: {:?}", r.frame_type()),
        })
    }
}
