//! Aqueduct encoding/decoding shared types and functions. 

pub use super::typed_chan_id::*;


/// Side of a connection.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(u8)]
pub enum Side {
    Client = 0,
    Server = 1,
}

impl Side {
    /// Get the direction that is flowing to this side.
    pub fn dir_to(self) -> Dir {
        match self {
            Side::Server => Dir::ToServer,
            Side::Client => Dir::ToClient,
        }
    }

    /// Get the opposite side.
    pub fn opposite(self) -> Self {
        use Side::*;
        match self {
            Client => Server,
            Server => Client,
        }
    }
}


/// Direction between client and server.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(u8)]
pub enum Dir {
    ToServer = 0,
    ToClient = 1,
}

impl Dir {
    /// Get the side that this direction is flowing to.
    pub fn side_to(self) -> Side {
        match self {
            Dir::ToServer => Side::Server,
            Dir::ToClient => Side::Client,
        }
    }
}


/// Channel ID.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanId(pub u64);

impl ChanId {
    /// The entrypoint channel ID.
    pub const ENTRYPOINT: Self = ChanId(0);

    /// Construct from parts. Panic if channel index excessively high.
    pub fn new(dir: Dir, minted_by: Side, oneshot: bool, idx: u64) -> Self {
        // panic safety: if idxs are assigned sequentially within a process, with four being
        //               assigned every nanosecond, it would take over 18 years to overflow
        assert!((idx & (0b111u64 << 61)) == 0, "chan idx overflowed");
        ChanId((dir as u64) | ((minted_by as u64) << 1) | ((oneshot as u64) << 2) | (idx << 3))
    }

    /// Get direction part.
    pub fn dir(self) -> Dir {
        if (self.0 & 0b001) == 0 { Dir::ToServer } else { Dir::ToClient }
    }

    /// Get minted-by part.
    pub fn minted_by(self) -> Side {
        if (self.0 & 0b010) == 0 { Side::Client } else { Side::Server }
    }

    /// Get is-oneshot part.
    pub fn is_oneshot(self) -> bool {
        (self.0 & 0b100) != 0
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        (self.0 & !0b111u64) >> 3
    }
}


/// Aqueduct frame type.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(u8)]
pub enum FrameType {
    Version = 237,
    ConnectionControl = 1,
    ChannelControl = 2,
    Message = 3,
    SentUnreliable = 4,
    AckReliable = 5,
    AckNackUnreliable = 6,
    FinishSender = 7,
    CloseReceiver = 8,
    ClosedChannelLost = 9,
}

impl FrameType {
    /// Convert from frame type byte.
    pub fn from_byte(b: u8) -> Option<Self> {
        use FrameType::*;
        [
            Version,
            ConnectionControl,
            ChannelControl,
            Message,
            SentUnreliable,
            AckReliable,
            AckNackUnreliable,
            FinishSender,
            CloseReceiver,
            ClosedChannelLost,
        ].into_iter().find(|&frame_type| frame_type as u8 == b)
    }
}


// constants for variable length integer coding.
pub const VLI_MASK: u8 = 0b01111111;
pub const VLI_MORE: u8 = 0b10000000;
pub const VLI_FINAL_SHIFT: u8 = 56;


// constants for version frame coding.
pub const VERSION_FRAME_MAGIC_BYTES: [u8; 7] = [80, 95, 166, 96, 15, 64, 142];
pub const VERSION_FRAME_HUMAN_TEXT: [u8; 8] = *b"AQUEDUCT";
pub const VERSION: &str = "0.0.0-AFTER";


/// An Aqueduct QUIC stream reset error code.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(u32)]
pub enum ResetCode {
    Cancelled = 1,
    Lost = 2,
}

impl ResetCode {
    /// Convert from integer representation.
    pub fn from_u64(n: u64) -> Option<Self> {
        use ResetCode::*;
        [Cancelled, Lost].into_iter().find(|&code| code as u64 == n)
    }
}


/// An Aqueduct QUIC connection close error code.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(u32)]
pub enum CloseCode {
    /// The closing side detected a violation of the Aqueduct protocol
    ProtoError = 1,
    /// The closing side's application explicitly asked the connection to close
    Explicit = 2,
    /// The closing side's application-provided (de)serialization logic errored
    SerdeError = 3,
}

impl CloseCode {
    /// Convert from integer representation.
    pub fn from_u64(n: u64) -> Option<Self> {
        use CloseCode::*;
        [ProtoError, Explicit, SerdeError].into_iter().find(|&code| code as u64 == n)
    }
}


/// Validate that a byte string is ASCII and cast to a str.
pub fn ascii_to_str(b: &[u8]) -> Option<&str> {
    if b.is_ascii() {
        // unwrap safety: all valid ASCII strings are valid UTF-8 strings
        Some(std::str::from_utf8(b).unwrap())
    } else {
        None
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum MessageNum {
    Reliable(u64),
    Unreliable(u64),
}
