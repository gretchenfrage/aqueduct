// typed API for reading frames from quic streams and datagrams

use crate::{
    zero_copy::{MultiBytes, quic::QuicStreamReader},
    codec::common::*,
};
use bytes::Bytes;
use quinn::RecvStream;
use anyhow::*;


// utility internal to frame reading module:
//
// - abstracts over zero-copy layer for QUIC stream versus datagram
// - provides helper methods for reading common primitives
// - is a place to store some basic stats so certain protocol errors can be validated
struct QuicReader {
    source_type: SourceType,
    source: QuicReaderSource,
    stats: QuicReaderStats,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum SourceType {
    UniStream,
    BidiStream,
    Datagram,
}

// source field of QuicReader
enum QuicReaderSource {
    Stream(QuicStreamReader),
    Datagram(MultiBytes),
}

// stats field of QuicReader
#[derive(Default)]
struct QuicReaderStats {
    version_frame: bool,
    non_version_frame: bool,
}

impl QuicReader {
    // read bytes and copy to buf
    async fn read(&mut self, buf: &mut [u8]) -> Result<(), Error> {
        Ok(match &mut self.source {
            &mut QuicReaderSource::Stream(ref mut inner) => inner.read(buf).await?,
            &mut QuicReaderSource::Datagram(ref mut inner) => inner.read(buf)?,
        })
    }
    
    // read bytes zero-copy-ly
    async fn read_zc(&mut self, n: usize) -> Result<MultiBytes, Error> {
        Ok(match &mut self.source {
            &mut QuicReaderSource::Stream(ref mut inner) => inner.read_zc(n).await?,
            &mut QuicReaderSource::Datagram(ref mut inner) => inner.read_zc(n)?,
        })
    }

    // whether the data source ends immediately after bytes read so far
    async fn is_done(&mut self) -> Result<bool, Error> {
        Ok(match &mut self.source {
            &mut QuicReaderSource::Stream(ref mut inner) => inner.is_done().await?,
            &mut QuicReaderSource::Datagram(ref inner) => inner.len() == 0,
        })
    }

    // read a fixed-size array
    async fn read_arr<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut buf = [0; N];
        self.read(&mut buf).await?;
        Ok(buf)
    }

    // read a single byte
    async fn read_byte(&mut self) -> Result<u8> {
        Ok(self.read_arr::<1>().await?[0])
    }

    // read a var len int. if limit is Some, decrements the referenced usize by the number of bytes
    // read, or errors if doing so would underflow.
    async fn read_vli_with_limit(&mut self, mut limit: Option<&mut usize>) -> Result<u64> {
        let mut i: u64 = 0;
        for x in 0..8 {
            if let Some(limit) = limit.as_mut() {
                ensure!(**limit > 0, "expected more bytes");
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
            ensure!(**limit > 0, "expected more bytes");
            **limit -= 1;
        }
        let b = self.read_byte().await?;
        i |= (b as u64) << VLI_FINAL_SHIFT;
        Ok(i)
    }

    // read a var len int.
    async fn read_vli(&mut self) -> Result<u64> {
        Ok(self.read_vli_with_limit(None).await?)
    }

    // read a var len int, then cast to usize or error if unable.
    async fn read_vli_usize(&mut self) -> Result<usize> {
        let n = self.read_vli().await?;
        n.try_into().map_err(|_| anyhow!("overflow casting var len int to usize: {}", n))
    }

    // read a channel id.
    async fn read_chan_id(&mut self) -> Result<ChanId> {
        self.read_vli().await.map(ChanId)
    }
}

// typed API for reading a sequence of frames, with some validation done automatically.
pub(crate) struct Frames(QuicReader);

impl Frames {
    pub(crate) fn from_uni_stream(stream: RecvStream) -> Self {
        Frames(QuicReader {
            source_type: SourceType::UniStream,
            source: QuicReaderSource::Stream(QuicStreamReader::new(stream)),
            stats: Default::default(),
        })
    }

    pub(crate) fn from_bidi_stream(stream: RecvStream) -> Self {
        Frames(QuicReader {
            source_type: SourceType::BidiStream,
            source: QuicReaderSource::Stream(QuicStreamReader::new(stream)),
            stats: Default::default(),
        })
    }

    pub(crate) fn from_datagram(datagram: Bytes) -> Self {
        Frames(QuicReader {
            source_type: SourceType::Datagram,
            source: QuicReaderSource::Datagram(MultiBytes::from(datagram)),
            stats: Default::default(),
        })
    }

    pub(crate) async fn frame(mut self) -> Result<Option<Frame>> {
        if self.0.is_done().await? {
            ensure!(
                self.0.stats.non_version_frame,
                "stream/datagram contained no non-Version frames",
            );
            return Ok(None);
        }
        let frame_type = FrameType::from_byte(self.0.read_byte().await?)?;
        
        // misc validation and stats updating goes here:
        if frame_type == FrameType::Version {
            ensure!(
                !(self.0.stats.version_frame || self.0.stats.non_version_frame),
                "Version frame not first frame",
            );
            self.0.stats.version_frame = true;
        } else {
            self.0.stats.non_version_frame = true;
        }
        
        Ok(Some(match frame_type {
            FrameType::Version => Frame::Version(Version(self.0)),
            FrameType::ConnectionControl => Frame::ConnectionControl(ConnectionControl(self.0)),
            FrameType::ChannelControl => Frame::ChannelControl(ChannelControl(self.0)),
            FrameType::Message => Frame::Message(Message(self.0)),
            FrameType::SentUnreliable => Frame::SentUnreliable(SentUnreliable(self.0)),
            FrameType::AckReliable => Frame::AckReliable(AckReliable(self.0)),
            FrameType::AckNackUnreliable => Frame::AckNackUnreliable(AckNackUnreliable(self.0)),
            FrameType::FinishSender => Frame::FinishSender(FinishSender(self.0)),
            FrameType::CloseReceiver => Frame::CloseReceiver(CloseReceiver(self.0)),
            FrameType::ClosedChannelLost => Frame::ClosedChannelLost(ClosedChannelLost(self.0)),
        }))
    }
}

// typed API for reading one frame in a sequence.
pub(crate) enum Frame {
    Version(Version),
    ConnectionControl(ConnectionControl),
    ChannelControl(ChannelControl),
    Message(Message),
    SentUnreliable(SentUnreliable),
    AckReliable(AckReliable),
    AckNackUnreliable(AckNackUnreliable),
    FinishSender(FinishSender),
    CloseReceiver(CloseReceiver),
    ClosedChannelLost(ClosedChannelLost),
}

// typed API for reading this frame type.
pub(crate) struct Version(QuicReader);

impl Version {
    pub(crate) async fn validate(mut self) -> Result<Frames> {
        ensure!(self.0.read_arr::<7>().await? == VERSION_FRAME_MAGIC_BYTES, "wrong magic bytes");
        ensure!(self.0.read_arr::<8>().await? == VERSION_FRAME_HUMAN_TEXT, "wrong human text");
        let mut version_buf = [0; 64];
        let version_len = self.0.read_vli_usize().await?;
        ensure!(version_len <= version_buf.len(), "unreasonably long version string");
        self.0.read(&mut version_buf[..version_len]).await?;
        let version = ascii_to_str(&version_buf[..version_len])?;
        ensure!(version == VERSION, "unknown version string: {:?}", version);
        Ok(Frames(self.0))
    }
}

// typed API for reading this frame type.
pub(crate) struct ConnectionControl(QuicReader);

impl ConnectionControl {
    // TODO
    pub(crate) async fn skip_headers(mut self) -> Result<Frames> {
        let len = self.0.read_vli_usize().await?;
        self.0.read_zc(len).await?;
        Ok(Frames(self.0))
    }
}

// typed API for reading this frame type.
pub(crate) struct ChannelControl(QuicReader);

impl ChannelControl {
    pub(crate) async fn chan_id(mut self) -> Result<(Frames, ChanId)> {
        let o = self.0.read_chan_id().await?;
        Ok((Frames(self.0), o))
    }
}

// typed API for reading this frame type.
pub(crate) struct Message(QuicReader);

impl Message {
    pub(crate) async fn sent_on(mut self) -> Result<(Message2, ChanId)> {
        let o = self.0.read_chan_id().await?;
        Ok((Message2(self.0), o))
    }
}

// typed API for reading this frame type.
pub(crate) struct Message2(QuicReader);

impl Message2 {
    pub(crate) async fn message_num(mut self) -> Result<(Message3, u64)> {
        let o = self.0.read_vli().await?;
        Ok((Message3(self.0), o))
    }
}

// typed API for reading this frame type.
pub(crate) struct Message3(QuicReader);

impl Message3 {
    pub(crate) async fn attachments_len(mut self) -> Result<(Message4, usize)> {
        let len = self.0.read_vli_usize().await?;
        Ok((Message4(self.0, len), len))
    }
}

// typed API for reading this frame type.
pub(crate) struct Message4(QuicReader, usize);

impl Message4 {
    pub(crate) async fn next_attachment(&mut self) -> Result<Option<ChanId>> {
        if self.1 == 0 {
            return Ok(None);
        }
        let o = self.0.read_vli_with_limit(Some(&mut self.1)).await?;
        Ok(Some(ChanId(o)))
    }

    pub(crate) fn done(self) -> Message5 {
        assert!(self.1 == 0, "Message4.done without reading all attachments");
        Message5(self.0)
    }
}

// typed API for reading this frame type.
pub(crate) struct Message5(QuicReader);

impl Message5 {
    pub(crate) async fn payload_len(mut self) -> Result<(Message6, usize)> {
        let len = self.0.read_vli_usize().await?;
        Ok((Message6(self.0, len), len))
    }
}

// typed API for reading this frame type.
pub(crate) struct Message6(QuicReader, usize);

impl Message6 {
    pub(crate) async fn payload(mut self) -> Result<(Frames, MultiBytes)> {
        let o = self.0.read_zc(self.1).await?;
        Ok((Frames(self.0), o))
    }
}

// typed API for reading this frame type.
pub(crate) struct SentUnreliable(QuicReader);

impl SentUnreliable {

}

// typed API for reading this frame type.
pub(crate) struct AckReliable(QuicReader);

impl AckReliable {

}

// typed API for reading this frame type.
pub(crate) struct AckNackUnreliable(QuicReader);

impl AckNackUnreliable {

}

// typed API for reading this frame type.
pub(crate) struct FinishSender(QuicReader);

impl FinishSender {

}

// typed API for reading this frame type.
pub(crate) struct CloseReceiver(QuicReader);

impl CloseReceiver {

}

// typed API for reading this frame type.
pub(crate) struct ClosedChannelLost(QuicReader);

impl ClosedChannelLost {
    pub(crate) async fn chan_id(mut self) -> Result<(Frames, ChanId)> {
        let o = self.0.read_chan_id().await?;
        Ok((Frames(self.0), o))
    }
}
