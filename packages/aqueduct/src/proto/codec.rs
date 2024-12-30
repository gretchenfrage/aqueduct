// encoding/decoding things on the wire

use crate::zero_copy::{
    quic::QuicStreamReader,
    MultiBytes,
    Cursor,
    MultiBytesWriter,
};
use bytes::Bytes;
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

pub(crate) enum QuicReader<'a> {
    Stream(QuicStreamReader),
    // TODO: adapt this to be able to just be a MultiBytes directly instead
    Datagram(Cursor<'a>),
}

impl<'a> QuicReader<'a> {
    pub(crate) fn stream(stream: quinn::RecvStream) -> Self {
        QuicReader::Stream(QuicStreamReader::new(stream))
    }

    pub(crate) fn datagram(datagram: &'a MultiBytes) -> Self {
        QuicReader::Datagram(datagram.cursor())
    }

    async fn read(&mut self, buf: &mut [u8]) -> Result<()> {
        Ok(match self {
            &mut QuicReader::Stream(ref mut inner) => inner.read(buf).await?,
            &mut QuicReader::Datagram(ref mut inner) => inner.read(buf)?,
        })
    }

    async fn read_byte(&mut self) -> Result<u8> {
        Ok(match self {
            &mut QuicReader::Stream(ref mut inner) => inner.read_byte().await?,
            &mut QuicReader::Datagram(ref mut inner) => inner.read_byte()?,
        })
    }

    async fn read_zc(&mut self, mut n: usize) -> Result<MultiBytes> {
        Ok(match self {
            &mut QuicReader::Stream(ref mut inner) => inner.read_zc(n).await?,
            &mut QuicReader::Datagram(ref mut inner) => inner.read_zc(n)?,
        })
    }

    async fn is_done(&mut self) -> Result<bool> {
        Ok(match self {
            &mut QuicReader::Stream(ref mut inner) => inner.is_done().await?,
            &mut QuicReader::Datagram(ref mut inner) => inner.remaining_len() == 0,
        })
    }

    async fn read_vli_with_limit(&mut self, mut limit: Option<&mut usize>) -> Result<u64> {
        let mut i: u64 = 0;
        for x in 0..8 {
            if let Some(limit) = limit.as_mut() {
                ensure!(**limit >= 0, "expected more bytes");
                **limit -= 1;
            }
            let b = self.read_byte().await?;
            i |= ((b & VLI_MASK) as u64) << (x * 7);
            if (b & VLI_MORE) == 0 {
                return Ok(i);
            }
        }
        debug_assert!((i & (0xffu64 << VLI_FINAL_SHIFT)) == 0);
        if let Some(limit) = limit.as_mut() {
            ensure!(**limit >= 0, "expected more bytes");
            **limit -= 1;
        }
        let b = self.read_byte().await?;
        i |= (b as u64) << VLI_FINAL_SHIFT;
        Ok(i)
    }

    async fn read_vli(&mut self) -> Result<u64> {
        Ok(self.read_vli_with_limit(None).await?)
    }

    async fn read_vli_usize(&mut self) -> Result<usize> {
        let n = self.read_vli().await?;
        n.try_into().map_err(|_| anyhow!("overflow casting var len int to usize: {}", n))
    }

    async fn read_chan_id(&mut self) -> Result<ChanId> {
        self.read_vli().await.map(ChanId)
    }
}

const VLI_MASK: u8 = 0b01111111;
const VLI_MORE: u8 = 0b10000000;
const VLI_FINAL_SHIFT: u8 = 56;

/*

    // encode a variable length integer
    pub(crate) fn encode_vli(mut i: u64, w: &mut MultiBytesWriter) {
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
    pub(crate) fn encode_vlba(b: MultiBytes, w: &mut MultiBytesWriter) {
        encode_vli(b.len().try_into().expect("usize overflowed u64"), w);
        w.write_zc(b);
    }

    // decode a variable length integer from already-in-memory bytes
    pub(crate) fn decode_vli(r: &mut Cursor) -> Result<u64> {        
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
    pub(crate) fn decode_vlba(r: &mut Cursor) -> Result<MultiBytes> {
        let len = decode_vli(r)?.try_into().map_err(|_| anyhow!("u64 overflowed usize"))?;
        Ok(r.read_zc(len)?)
    }

    // asynchronously read and decode a variable length integer from a QUIC stream
    pub(crate) async fn decode_vli_async(r: &mut QuicReader) -> Result<u64> {        
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
    pub(crate) async fn decode_vlba_async(r: &mut QuicReader) -> Result<MultiBytes> {
        let len = decode_vli_async(r).await?.try_into().map_err(|_| anyhow!("u64 overflowed usize"))?;
        Ok(r.read_zc(len).await?)
    }

    pub(crate) enum FrameType {
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
    }

    impl FrameType {
        pub(crate) async fn read(r: &mut QuicReader) -> Result<Self> {
            use FrameType::*;
            Ok(match r.read_byte().await? {
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
    */

pub(crate) struct FramesReader<'a>(QuicReader<'a>);

impl<'a> FramesReader<'a> {
    pub(crate) fn new(r: QuicReader<'a>) -> Self {
        FramesReader(r)
    }

    pub(crate) async fn read_frame(mut self) -> Result<FrameReader<'a>> {
        Ok(match self.0.read_byte().await? {
            239 => FrameReader::Version(FrameReaderVersion(self.0)),
            1 => FrameReader::ConnectionControl(FrameReaderConnectionControl(self.0)),
            2 => FrameReader::ChannelControl(FrameReaderChannelControl(self.0)),
            3 => FrameReader::Message(FrameReaderMessage(self.0)),
            4 => FrameReader::SentUnreliable(FrameReaderSentUnreliable(self.0)),
            5 => FrameReader::AckReliable(FrameReaderAckReliable(self.0)),
            6 => FrameReader::AckNackUnreliable(FrameReaderAckNackUnreliable(self.0)),
            7 => FrameReader::FinishSender(FrameReaderFinishSender(self.0)),
            8 => FrameReader::CloseReceiver(FrameReaderCloseReceiver(self.0)),
            9 => FrameReader::ClosedChannelLost(FrameReaderClosedChannelLost(self.0)),
            b => bail!("invalid frame type byte: {}", b),
        })
    }
}

pub(crate) enum FrameReader<'a> {
    Version(FrameReaderVersion<'a>),
    ConnectionControl(FrameReaderConnectionControl<'a>),
    ChannelControl(FrameReaderChannelControl<'a>),
    Message(FrameReaderMessage<'a>),
    SentUnreliable(FrameReaderSentUnreliable<'a>),
    AckReliable(FrameReaderAckReliable<'a>),
    AckNackUnreliable(FrameReaderAckNackUnreliable<'a>),
    FinishSender(FrameReaderFinishSender<'a>),
    CloseReceiver(FrameReaderCloseReceiver<'a>),
    ClosedChannelLost(FrameReaderClosedChannelLost<'a>),
}

pub(crate) struct FrameReaderVersion<'a>(QuicReader<'a>);

impl<'a> FrameReaderVersion<'a> {
    pub(crate) async fn read_validate(mut self) -> Result<FramesReader<'a>> {
        let mut buf = [0; 15];
        self.0.read(&mut buf).await?;
        ensure!(&buf[..7] == &[80, 95, 166, 96, 15, 64, 142], "wrong version frame magic bytes");
        ensure!(&buf[7..] == b"AQUEDUCT", "wrong version frame human text");
        let version_len = self.0.read_vli().await?;
        ensure!(version_len <= 64, "unreasonably long version length");
        let version = self.0.read_zc(version_len as usize).await?;
        ensure!(version == b"0.0.0-AFTER"[..], "unknown version: {:?}", version);
        Ok(FramesReader(self.0))
    }
}

pub(crate) struct FrameReaderConnectionControl<'a>(QuicReader<'a>);

impl<'a> FrameReaderConnectionControl<'a> {
    pub(crate) async fn read_headers_len(mut self) -> Result<(FrameReaderConnectionControlPart2<'a>, usize)> {
        let len = self.0.read_vli_usize().await?;
        Ok((FrameReaderConnectionControlPart2(self.0, len), len))
    }
}

pub(crate) struct FrameReaderConnectionControlPart2<'a>(QuicReader<'a>, usize);

impl<'a> FrameReaderConnectionControlPart2<'a> {
    pub(crate) async fn read_headers(mut self) -> Result<(FramesReader<'a>, MultiBytes)> {
        let o = self.0.read_zc(self.1).await?;
        Ok((FramesReader(self.0), o))
    }
}

pub(crate) struct FrameReaderChannelControl<'a>(QuicReader<'a>);

impl<'a> FrameReaderChannelControl<'a> {
    pub(crate) async fn read_chan_id(mut self) -> Result<(FramesReader<'a>, ChanId)> {
        let o = self.0.read_chan_id().await?;
        Ok((FramesReader(self.0), o))
    }
}

pub(crate) struct FrameReaderMessage<'a>(QuicReader<'a>);

impl<'a> FrameReaderMessage<'a> {
    pub(crate) async fn read_chan_id(mut self) -> Result<(FrameReaderMessagePart2<'a>, ChanId)> {
        let o = self.0.read_chan_id().await?;
        Ok((FrameReaderMessagePart2(self.0), o))
    }
}

pub(crate) struct FrameReaderMessagePart2<'a>(QuicReader<'a>);

impl<'a> FrameReaderMessagePart2<'a> {
    pub(crate) async fn read_message_num(mut self) -> Result<(FrameReaderMessagePart3<'a>, u64)> {
        let o = self.0.read_vli().await?;
        Ok((FrameReaderMessagePart3(self.0), o))
    }
}

pub(crate) struct FrameReaderMessagePart3<'a>(QuicReader<'a>);

impl<'a> FrameReaderMessagePart3<'a> {
    pub(crate) async fn read_attachments_len(mut self) -> Result<(FrameReaderMessagePart4<'a>, usize)> {
        let len = self.0.read_vli_usize().await?;
        Ok((FrameReaderMessagePart4(self.0, len), len))
    }
}

pub(crate) struct FrameReaderMessagePart4<'a>(QuicReader<'a>, usize);

impl<'a> FrameReaderMessagePart4<'a> {
    pub(crate) async fn read_attachments(mut self) -> Result<(FrameReaderMessagePart5<'a>, MultiBytes)> {
        let o = self.0.read_zc(self.1).await?;
        Ok((FrameReaderMessagePart5(self.0), o))
    }
}

pub(crate) struct FrameReaderMessagePart5<'a>(QuicReader<'a>);

impl<'a> FrameReaderMessagePart5<'a> {
    pub(crate) async fn read_payload_len(mut self) -> Result<(FrameReaderMessagePart6<'a>, usize)> {
        let len = self.0.read_vli_usize().await?;
        Ok((FrameReaderMessagePart6(self.0, len), len))
    }
}

pub(crate) struct FrameReaderMessagePart6<'a>(QuicReader<'a>, usize);

impl<'a> FrameReaderMessagePart6<'a> {
    pub(crate) async fn read_payload(mut self) -> Result<(FramesReader<'a>, MultiBytes)> {
        let o = self.0.read_zc(self.1).await?;
        Ok((FramesReader(self.0), o))
    }
}


pub(crate) struct FrameReaderSentUnreliable<'a>(QuicReader<'a>);

impl<'a> FrameReaderSentUnreliable<'a> {
    pub(crate) async fn read_sent_unreliable_diff(mut self) -> Result<(FramesReader<'a>, u64)> {
        let o = self.0.read_vli().await?;
        Ok((FramesReader(self.0), o))
    }
}

pub(crate) struct FrameReaderAckReliable<'a>(QuicReader<'a>);

/*

impl<'a> FrameReaderAckReliable<'a> {
    pub(crate) async fn read_acks_beginning(mut self) -> Result<FrameReaderAckReliableAck<'a>> {
        let len = self.0.read_vli_usize().await?;
        Ok(FrameReaderAckReliableAck { r: self.0, remaining_len: len })
    }
}

pub(crate) struct FrameReaderAckReliableAck<'a> {
    r: QuicReader<'a>,
    remaining_len: usize,
}

impl<'a> FrameReaderAckReliableAck<'a> {
    pub(crate) async fn read_ack(
        mut self,
    ) -> Result<(FrameReaderAckReliableNotYetAckOrEnd<'a>, u64)> {
        let o = self.r.read_vli_with_limit(Some(&mut self.remaining_len)).await?;
        let next = if self.remaining_len == 0 {
            FrameReaderAckReliableNotYetAckOrEnd::End(FramesReader(self.r))
        } else {
            FrameReaderAckReliableNotYetAckOrEnd::NotYetAck(self)
        };
    }
}

pub(crate) enum FrameReaderAckReliableAckOrEnd<'a> {
    Ack(FrameReaderAckReliableAck<'a>),
    End(FramesReader<'a>),
}

pub(crate) struct FrameReaderAckReliableNotYetAck<'a> {
    r: QuicReader<'a>,
    remaining_len: usize,
}

pub(crate) enum FrameReaderAckReliableNotYetAckOrEnd<'a> {
    NotYetAck(FrameReaderAckReliableNotYetAck<'a>),
    End(FramesReader<'a>),
}
*/
pub(crate) struct FrameReaderAckNackUnreliable<'a>(QuicReader<'a>);

impl<'a> FrameReaderAckNackUnreliable<'a> {

}

pub(crate) struct FrameReaderFinishSender<'a>(QuicReader<'a>);

impl<'a> FrameReaderFinishSender<'a> {

}

pub(crate) struct FrameReaderCloseReceiver<'a>(QuicReader<'a>);

impl<'a> FrameReaderCloseReceiver<'a> {

}

pub(crate) struct FrameReaderClosedChannelLost<'a>(QuicReader<'a>);

impl<'a> FrameReaderClosedChannelLost<'a> {

}



/*
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

pub(crate) const VERSION_FRAME_MAGIC_BYTES: [u8; 8] = [239, 80, 95, 166, 96, 15, 64, 142];
pub(crate) const VERSION_FRAME_HUMAN_TEXT: [u8; 8] = *b"AQUEDUCT";
pub(crate) const VERSION_FRAME_VERSION: &[u8] = b"0.0.0-AFTER";


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

    async fn decode(r: &mut QuicReader) -> Result<Self> {
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

    async fn decode(r: &mut QuicReader) -> Result<Self> {
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
}*/

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
