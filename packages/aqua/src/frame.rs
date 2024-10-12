// internal encoding/decoding of frames on the wire

use crate::zero_copy::{
    MultiBytes,
    quic::QuicMultiBytesReader,
};
use smallvec::SmallVec;
use anyhow::*;


// newtype for a channel id
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChannelId(pub u64);

#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(u8)]
pub enum Direction {
    ToServer = 0,
    ToClient = 1,
}

#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(u8)]
pub enum Side {
    Client = 0,
    Server = 1,
}

#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(u8)]
pub enum Plurality {
    Multiple = 0,
    Oneshot = 1,
}

impl ChannelId {
    pub fn new(
        direction: Direction,
        initiator: Side,
        plurality: Plurality,
        index: u64,
    ) -> Self {
        assert!((index & !(0b111 << 61)) == 0, "ChannelId idx beyond 63-bit uint");
        ChannelId((direction as u64) | ((initiator as u64) << 1) | ((plurality as u64) << 2) | (index << 3))
    }
    
    pub fn direction(self) -> Direction {
        if (self.0 & 0b001) == 0 { Direction::ToServer } else { Direction::ToClient }
    }
    
    pub fn initiator(self) -> Side {
        if (self.0 & 0b010) == 0 { Side::Client } else { Side::Server }
    }
    
    pub fn plurality(self) -> Plurality {
        if (self.0 & 0b100) == 0 { Plurality::Multiple } else { Plurality::Oneshot }
    }
}

// structured representation of a stream frame
pub enum StreamFrame {
    Message(MessageFrame),
}

// structured representation of a stream frame of type Message
pub struct MessageFrame {
    pub dst: ChannelId,
    pub attachments: SmallVec<[ChannelId; 4]>,
    pub payload: MultiBytes,
}

impl StreamFrame {
    // read from a QUIC stream
    pub async fn read(stream: &mut QuicMultiBytesReader, our_side: Side) -> Result<Self, Error> {
        match stream.read_u8().await? {
            1 => MessageFrame::read(stream, our_side).await.map(StreamFrame::Message),
            _ => Err(Error::msg("unknown stream frame discriminant")),
        }
    }
}

impl MessageFrame {
    // read from a QUIC stream, after the stream frame discriminant
    pub async fn read(stream: &mut QuicMultiBytesReader, our_side: Side) -> Result<Self, Error> {
        let dst = ChannelId(stream.read_var_len_u64().await?);
        let mut attachments = SmallVec::new();
        loop {
            let n = stream.read_var_len_u64().await?;
            if n == 0 {
                break;
            } else {
                let id = ChannelId(n);
                ensure!(id.initiator() != our_side, "received attachment has invalid initiator");
                attachments.push(ChannelId(n));
            }
        }
        let payload_len = stream.read_var_len_usize().await?;
        let payload = stream.read_zc(payload_len).await?;
        Ok(MessageFrame { dst, attachments, payload })
    }
}
