//! Aqueduct encoding/decoding shared types and functions.

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct Side(pub bool);

impl Side {
    pub const CLIENT: Self = Side(false);
    pub const SERVER: Self = Side(true);
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanId(pub u64);

impl ChanId {
    pub const ENTRYPOINT: Self = ChanId(0);

    /// panics if channel index excessively high.
    pub fn new(creator: Side, sender: Side, oneshot: bool, idx: u64) -> Self {
        // panic safety: if idxs are assigned sequentially within a process, with four being
        //               assigned every nanosecond, it would take over 18 years to overflow
        assert!((idx & (0b111u64 << 61)) == 0, "chan idx overflowed");
        ChanId((creator.0 as u64) | ((sender.0 as u64) << 1) | ((oneshot as u64) << 2) | (idx << 3))
    }

    pub fn creator(self) -> Side {
        Side(self.0 & 0b001 != 0)
    }

    pub fn sender(self) -> Side {
        Side(self.0 & 0b010 != 0)
    }

    pub fn is_oneshot(self) -> bool {
        self.0 & 0b100 != 0
    }

    pub fn idx(self) -> u64 {
        (self.0 & !0b111u64) >> 3
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(u8)]
pub enum FrameTag {
    Version = 239,
    AckVersion = 1,
    ConnectionHeaders = 2,
    RouteTo = 3,
    Message = 4,
    SentUnreliable = 5,
    FinishSender = 6,
    CancelSender = 7,
    AckReliable = 8,
    AckNackUnreliable = 9,
    CloseReceiver = 10,
    ForgetChannel = 11,
}

impl FrameTag {
    pub fn from_byte(b: u8) -> Option<Self> {
        use FrameTag::*;
        [
            Version,
            AckVersion,
            ConnectionHeaders,
            RouteTo,
            Message,
            SentUnreliable,
            FinishSender,
            CancelSender,
            AckReliable,
            AckNackUnreliable,
            CloseReceiver,
            ForgetChannel,
        ]
        .into_iter()
        .find(|&frame_type| frame_type as u8 == b)
    }
}

// constants for variable length integer coding.
pub const VARINT_MASK: u8 = 0b01111111;
pub const VARINT_MORE: u8 = 0b10000000;
pub const VARINT_FINAL_SHIFT: u8 = 56;

// constants for version frame coding.
pub const VERSION_FRAME_MAGIC_BYTES: [u8; 7] = [80, 95, 166, 96, 15, 64, 142];
pub const VERSION_FRAME_HUMAN_TEXT: [u8; 8] = *b"AQUEDUCT";
pub const VERSION: &str = "0.0.0-AFTER";

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
