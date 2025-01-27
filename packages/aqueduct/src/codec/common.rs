// common types and code between encoding and decoding

use anyhow::*;


// side of a connection.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum Side {
    Client = 0,
    Server = 1,
}

impl Side {
    pub(crate) fn dir_to(self) -> Dir {
        match self {
            Side::Server => Dir::ToServer,
            Side::Client => Dir::ToClient,
        }
    }


    pub(crate) fn opposite(self) -> Self {
        use Side::*;
        match self {
            Client => Server,
            Server => Client,
        }
    }
}

// direction between client and server.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum Dir {
    ToServer = 0,
    ToClient = 1,
}

impl Dir {
    pub(crate) fn side_to(self) -> Side {
        match self {
            Dir::ToServer => Side::Server,
            Dir::ToClient => Side::Client,
        }
    }
}

// channel ID.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub(crate) struct ChanId(pub(crate) u64);

impl ChanId {
    pub const ENTRYPOINT: Self = ChanId(0);

    pub(crate) fn new(dir: Dir, minted_by: Side, oneshot: bool, idx: u64) -> Self {
        // panic safety: if idxs are assigned sequentially within a process, with four being
        //               assigned every nanosecond, it would take over 18 years to overflow
        assert!((idx & (0b111u64 << 61)) == 0, "chan idx overflowed");
        ChanId((dir as u64) | ((minted_by as u64) << 1) | ((oneshot as u64) << 2) | (idx << 3))
    }

    pub(crate) fn dir(self) -> Dir {
        if (self.0 & 0b001) == 0 { Dir::ToServer } else { Dir::ToClient }
    }

    pub(crate) fn minted_by(self) -> Side {
        if (self.0 & 0b010) == 0 { Side::Client } else { Side::Server }
    }

    pub(crate) fn oneshot(self) -> bool {
        (self.0 & 0b100) != 0
    }

    pub(crate) fn idx(self) -> u64 {
        (self.0 & !0b111u64) >> 3
    }
}

// aqueduct frame type, with conversion between frame type byte.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum FrameType {
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
    pub(crate) fn from_byte(b: u8) -> Result<Self> {
        use FrameType::*;
        Ok(match b {
            239 => Version,
            1 => ConnectionControl,
            2 => ChannelControl,
            3 => Message,
            4 => SentUnreliable,
            5 => AckReliable,
            6 => AckNackUnreliable,
            7 => FinishSender,
            8 => CloseReceiver,
            9 => ClosedChannelLost,
            b => bail!("invalid frame type byte: {}", b),
        })
    }
}

// constants for variable length integer coding.
pub(crate) const VLI_MASK: u8 = 0b01111111;
pub(crate) const VLI_MORE: u8 = 0b10000000;
pub(crate) const VLI_FINAL_SHIFT: u8 = 56;

// constants for version frame coding.
pub(crate) const VERSION_FRAME_MAGIC_BYTES: [u8; 7] = [80, 95, 166, 96, 15, 64, 142];
pub(crate) const VERSION_FRAME_HUMAN_TEXT: [u8; 8] = *b"AQUEDUCT";
pub(crate) const VERSION: &str = "0.0.0-AFTER";

// validate that a byte string is ASCII and cast to a str.
pub(crate) fn ascii_to_str(b: &[u8]) -> Result<&str> {
    ensure!(b.is_ascii(), "expected ASCII string");
    // safety: all valid ASCII strings are valid UTF-8 strings
    Ok(std::str::from_utf8(b).unwrap())
}

// a reset error code.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(u32)]
pub(crate) enum ResetCode {
    Cancelled = 1,
    Lost = 2,
}

impl ResetCode {
    pub(crate) fn from_u64(n: u64) -> Option<Self> {
        use ResetCode::*;
        [Cancelled, Lost].into_iter().find(|&code| code as u64 == n)
    }
}

// a QUIC connection close error code.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(u32)]
pub(crate) enum CloseCode {
    // the closing side detected a violation of the Aqueduct protocol
    ProtoError = 1,
    // the closing side's application explicitly asked the connection to close
    Explicit = 2,
    // the closing side's application-provided (de)serialization logic errored
    SerdeError = 3,
}

impl CloseCode {
    pub(crate) fn from_u64(n: u64) -> Option<Self> {
        use CloseCode::*;
        [ProtoError, Explicit, SerdeError].into_iter().find(|&code| code as u64 == n)
    }
}
