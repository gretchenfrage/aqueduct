// encoding/decoding things on the wire

use crate::zero_copy::{*, quic::*};
use bytes::*;
use anyhow::*;


// side of a connection
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum Side {
    Client = 0,
    Server = 1,
}

// direction between client and server
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum Dir {
    ToServer = 0,
    ToClient = 1,
}

// channel ID
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

const VLI_MASK: u8 = 0b01111111;
const VLI_MORE: u8 = 0b10000000;
const VLI_FINAL_SHIFT: u8 = 56;

// encode a variable length integer
fn encode_vli(mut i: u64, w: &mut MultiBytesWriter) {
    for _ in 0..8 {
        let mut b = (i as u8 & VLI_MASK) as u8;
        i >>= 7;
        b |= ((i != 0) as u8) << 7;
        w.write(&[b]);
        if i == 0 {
            return;
        }
    }
    debug_assert!(i != 0 && (i >> 8) == 0);
    w.write(&[i as u8]);
}

// encode a variable length byte array
fn encode_vlba(b: MultiBytes, w: &mut MultiBytesWriter) {
    encode_vli(b.len().try_into().expect("usize overflowed u64"), w);
    w.write_zc(b);
}

// decode a variable length integer from already-in-memory bytes
fn decode_vli(r: &mut Cursor) -> Result<u64> {        
    let mut i: u64 = 0;
    for x in 0..8 {
        let b = r.read_byte()?;
        i |= ((b & VLI_MASK) as u64) << (x * 7);
        if (b & VLI_MORE) == 0 {
            return Ok(i);
        }
    }
    debug_assert!((i & (0xffu64 << VLI_FINAL_SHIFT)) == 0);
    let b = r.read_byte()?;
    i |= (b as u64) << VLI_FINAL_SHIFT;
    Ok(i)
}

// decode a variable length byte array from already-in-memory bytes
fn decode_vlba(r: &mut Cursor) -> Result<MultiBytes> {
    let len = decode_vli(r)?.try_into().map_err(|_| anyhow!("u64 overflowed usize"))?;
    Ok(r.read_zc(len)?)
}

// asynchronously read and decode a variable length integer from a QUIC stream
async fn decode_vli_async(r: &mut QuicMultiBytesReader) -> Result<u64> {        
    let mut i: u64 = 0;
    for x in 0..8 {
        let b = r.read_byte().await?;
        i |= ((b & VLI_MASK) as u64) << (x * 7);
        if (b & VLI_MORE) == 0 {
            return Ok(i);
        }
    }
    debug_assert!((i & (0xffu64 << VLI_FINAL_SHIFT)) == 0);
    let b = r.read_byte().await?;
    i |= (b as u64) << VLI_FINAL_SHIFT;
    Ok(i)
}

// asynchronously read and decode a variable length byte array from a QUIC stream
async fn decode_vlba_async(r: &mut QuicMultiBytesReader) -> Result<MultiBytes> {
    let len = decode_vli_async(r).await?.try_into().map_err(|_| anyhow!("u64 overflowed usize"))?;
    Ok(r.read_zc(len).await?)
}

// aqueduct frame that could appear on a datagram / stream that has not yet been established as a
// channel control stream
pub(crate) enum NoCtxFrame {
    Version,
    ConnCtrl {
        headers: MultiBytes,
    },
    ChanCtrl {
        chan_id: ChanId,
    },
    Message {
        sent_on: ChanId,
        message_num: u64,
        attachments: MultiBytes,
        payload: MultiBytes,
    },
    ClosedChanLost {
        chan_id: ChanId,
    },
}

const VERSION_FRAME_MAGIC_BYTES: [u8; 8] = [239, 80, 95, 166, 96, 15, 64, 142];
const VERSION_FRAME_HUMAN_TEXT: [u8; 8] = *b"AQUEDUCT";
const VERSION_FRAME_VERSION: &[u8] = b"0.0.0-AFTER";

impl NoCtxFrame {
    fn encode(self, w: &mut MultiBytesWriter) {
        match self {
            NoCtxFrame::Version => {
                w.write(&VERSION_FRAME_MAGIC_BYTES);
                w.write(&VERSION_FRAME_HUMAN_TEXT);
                encode_vlba(Bytes::from(VERSION_FRAME_VERSION).into(), w);
            }
            NoCtxFrame::ConnCtrl { headers } => {
                w.write(&[1]);
                encode_vlba(headers.into(), w);
            }
            NoCtxFrame::ChanCtrl { chan_id } => {
                w.write(&[2]);
                encode_vli(chan_id.0, w);
            }
            NoCtxFrame::Message { sent_on, message_num, attachments, payload } => {
                w.write(&[3]);
                encode_vli(sent_on.0, w);
                encode_vli(message_num, w);
                encode_vlba(attachments, w);
                encode_vlba(payload, w);
            }
            NoCtxFrame::ClosedChanLost { chan_id } => {
                w.write(&[9]);
                encode_vli(chan_id.0, w);
            }
        }
    }

    async fn decode(r: &mut QuicMultiBytesReader) -> Result<Self> {
        Ok(match r.read_byte().await? {
            b if b == VERSION_FRAME_MAGIC_BYTES[0] => {
                let mut buf = [0; 15];
                r.read(&mut buf).await?;
                ensure!(
                    &buf[..7] == &VERSION_FRAME_MAGIC_BYTES[1..],
                    "version frame magic bytes wrong",
                );
                ensure!(
                    &buf[7..] == &VERSION_FRAME_HUMAN_TEXT,
                    "version frame human text wrong",
                );
                let version = decode_vlba_async(r).await?;
                ensure!(
                    version.len() == VERSION_FRAME_VERSION.len() && {
                        let mut buf = [0; VERSION_FRAME_VERSION.len()];
                        version.cursor().read(&mut buf).unwrap();
                        &buf == VERSION_FRAME_VERSION
                    },
                    // TODO: better string-like debugging
                    "unknown version: {:?}", version,
                );
                NoCtxFrame::Version
            },
            1 => NoCtxFrame::ConnCtrl {
                headers: decode_vlba_async(r).await?,
            },
            2 => NoCtxFrame::ChanCtrl {
                chan_id: ChanId(decode_vli_async(r).await?),
            },
            3 => NoCtxFrame::Message {
                sent_on: ChanId(decode_vli_async(r).await?),
                message_num: decode_vli_async(r).await?,
                attachments: decode_vlba_async(r).await?,
                payload: decode_vlba_async(r).await?,
            },
            4 => bail!("SentUnreliable frame not in chan ctrl stream"),
            5 => bail!("AckReliable frame not in chan ctrl stream"),
            6 => bail!("AckNackUnreliable frame not in chan ctrl stream"),
            7 => bail!("FinishSender frame not in chan ctrl stream"),
            8 => bail!("CloseReceiver frame not in chan ctrl stream"),
            9 => NoCtxFrame::ClosedChanLost {
                chan_id: ChanId(decode_vli_async(r).await?),
            },
            b => bail!("invalid frame type byte: {}", b),
        })
    }

    fn decode_datagram(r: &mut Cursor) -> Result<Self> {
        Ok(match r.read_byte()? {
            b if b == VERSION_FRAME_MAGIC_BYTES[0] => {
                let mut buf = [0; 15];
                r.read(&mut buf)?;
                ensure!(
                    &buf[..7] == &VERSION_FRAME_MAGIC_BYTES[1..],
                    "version frame magic bytes wrong",
                );
                ensure!(
                    &buf[7..] == &VERSION_FRAME_HUMAN_TEXT,
                    "version frame human text wrong",
                );
                let version = decode_vlba(r)?;
                ensure!(
                    // TODO: simplify everything in this match clause
                    version.len() == VERSION_FRAME_VERSION.len() && {
                        let mut buf = [0; VERSION_FRAME_VERSION.len()];
                        version.cursor().read(&mut buf).unwrap();
                        &buf == VERSION_FRAME_VERSION
                    },
                    // TODO: better string-like debugging
                    "unknown version: {:?}", version,
                );
                NoCtxFrame::Version
            },
            1 => bail!("ConnectionControl frame in datagram"),
            2 => bail!("ChannelControl frame in datagram"),
            3 => NoCtxFrame::Message {
                sent_on: ChanId(decode_vli(r)?),
                message_num: decode_vli(r)?,
                attachments: decode_vlba(r)?,
                payload: decode_vlba(r)?,
            },
            4 => bail!("SentUnreliable frame not in chan ctrl stream"),
            5 => bail!("AckReliable frame not in chan ctrl stream"),
            6 => bail!("AckNackUnreliable frame not in chan ctrl stream"),
            7 => bail!("FinishSender frame not in chan ctrl stream"),
            8 => bail!("CloseReceiver frame not in chan ctrl stream"),
            9 => bail!("ClosedChannelLost frame in datagram"),
            b => bail!("invalid frame type byte: {}", b),
        })
    }
}

// aqueduct frame that could appear on an established channel control stream
pub(crate) enum ChanCtrlFrame {
    SentUnreliable {
        num_sent_diff: u64,
    },
    AckReliable {
        acks: MultiBytes,
    },
    AckNackUnreliable {
        acknacks: MultiBytes,
    },
    FinishSender {
        num_sent: u64,
    },
    CloseReceiver {
        reliable_acknacks: MultiBytes,
    },
}

impl ChanCtrlFrame {
    fn encode(self, w: &mut MultiBytesWriter) {
        match self {
            ChanCtrlFrame::SentUnreliable { num_sent_diff } => {
                w.write(&[4]);
                encode_vli(num_sent_diff, w);
            },
            ChanCtrlFrame::AckReliable { acks } => {
                w.write(&[5]);
                encode_vlba(acks, w);
            },
            ChanCtrlFrame::AckNackUnreliable { acknacks } => {
                w.write(&[6]);
                encode_vlba(acknacks, w);
            },
            ChanCtrlFrame::FinishSender { num_sent } => {
                w.write(&[7]);
                encode_vli(num_sent, w);
            },
            ChanCtrlFrame::CloseReceiver { reliable_acknacks } => {
                w.write(&[8]);
                encode_vlba(reliable_acknacks, w);
            },
        }
    }

    async fn decode(r: &mut QuicMultiBytesReader) -> Result<Self> {
        Ok(match r.read_byte().await? {
            b if b == VERSION_FRAME_MAGIC_BYTES[0] => bail!("Version frame in chan ctrl stream"),
            1 => bail!("ConnectionControl frame in chan ctrl stream"),
            2 => bail!("ChannelControl frame in chan ctrl stream"),
            3 => bail!("Message frame in chan ctrl stream"),
            4 => ChanCtrlFrame::SentUnreliable {
                num_sent_diff: decode_vli_async(r).await?,
            },
            5 => ChanCtrlFrame::AckReliable {
                acks: decode_vlba_async(r).await?,
            },
            6 => ChanCtrlFrame::AckNackUnreliable {
                acknacks: decode_vlba_async(r).await?,
            },
            7 => ChanCtrlFrame::FinishSender {
                num_sent: decode_vli_async(r).await?,
            },
            8 => ChanCtrlFrame::CloseReceiver {
                reliable_acknacks: decode_vlba_async(r).await?
            },
            9 => bail!("ClosedChannelLost frame in chan ctrl stream"),
            b => bail!("invalid frame type byte: {}", b),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entrypoint_sanity() {
        assert!(ChanId::ENTRYPOINT.dir() == Dir::ToServer);
        assert!(ChanId::ENTRYPOINT.minted_by() == Side::Client);
        assert!(ChanId::ENTRYPOINT.oneshot() == false);
        assert_eq!(ChanId::ENTRYPOINT.idx(), 0);

        assert_eq!(ChanId::new(Dir::ToServer, Side::Client, false, 0), ChanId::ENTRYPOINT);
    }
}
