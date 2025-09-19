//! Typed API for reading frames from quic streams and datagrams.

use crate::{frame::common::*, quic_zc};
use anyhow::anyhow;
use bytes::Bytes;
use multibytes::*;

// ==== error handling ====

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    /// A reset was received on the underlying QUIC stream.
    Reset,
    /// Some unrecoverable error occurred.
    Other(anyhow::Error),
}

impl From<anyhow::Error> for Error {
    fn from(e: anyhow::Error) -> Self {
        Error::Other(e)
    }
}

impl From<TooFewBytesError> for Error {
    fn from(TooFewBytesError: TooFewBytesError) -> Self {
        anyhow::Error::msg("too few bytes").into()
    }
}

impl From<quic_zc::Error> for Error {
    fn from(e: quic_zc::Error) -> Self {
        match e {
            quic_zc::Error::Quic(quinn::ReadError::Reset(_)) => Error::Reset,
            quic_zc::Error::Quic(e) => Error::Other(e.into()),
            quic_zc::Error::TooFewBytes => TooFewBytesError.into(),
        }
    }
}

// ==== module-internal utilities ====

// - abstracts over zero-copy layer for QUIC stream versus datagram
// - provides helper methods for reading common primitives
enum Reader {
    Stream(quic_zc::QuicStreamReader),
    Datagram(MultiBytes),
}

impl Reader {
    // read bytes and copy to buf
    async fn read(&mut self, buf: &mut [u8]) -> Result<()> {
        Ok(match self {
            &mut Reader::Stream(ref mut inner) => inner.read(buf).await?,
            &mut Reader::Datagram(ref mut inner) => inner.read(buf)?,
        })
    }

    // read bytes zero-copy-ly
    async fn read_zc(&mut self, n: usize) -> Result<MultiBytes> {
        Ok(match self {
            &mut Reader::Stream(ref mut inner) => inner.read_zc(n).await?,
            &mut Reader::Datagram(ref mut inner) => inner.read_zc(n)?,
        })
    }

    // whether the data source ends immediately after bytes read so far
    async fn is_done(&mut self) -> Result<bool> {
        Ok(match self {
            &mut Reader::Stream(ref mut inner) => inner.is_done().await?,
            &mut Reader::Datagram(ref inner) => inner.len() == 0,
        })
    }

    // read a single byte
    async fn read_byte(&mut self) -> Result<u8> {
        let mut buf = [0];
        self.read(&mut buf).await?;
        let [b] = buf;
        Ok(b)
    }

    // read a var len int. if limit is Some, decrements the referenced usize by the number of bytes
    // read, or errors if doing so would underflow.
    async fn read_varint_with_limit(&mut self, mut limit: Option<&mut usize>) -> Result<u64> {
        let mut i: u64 = 0;
        for x in 0..8 {
            if let Some(limit) = limit.as_mut() {
                if **limit == 0 {
                    return Err(TooFewBytesError.into());
                }
                **limit -= 1;
            }
            let b = self.read_byte().await?;
            i |= ((b & VARINT_MASK) as u64) << (x * 7);
            if (b & VARINT_MORE) == 0 {
                return Ok(i);
            }
        }
        debug_assert!((i & (0xffu64 << VARINT_FINAL_SHIFT)) == 0);
        if let Some(limit) = limit.as_mut() {
            if **limit == 0 {
                return Err(TooFewBytesError.into());
            }
            **limit -= 1;
        }
        let b = self.read_byte().await?;
        i |= (b as u64) << VARINT_FINAL_SHIFT;
        Ok(i)
    }

    // read a var len int.
    async fn read_varint(&mut self) -> Result<u64> {
        Ok(self.read_varint_with_limit(None).await?)
    }

    // read a var len int, then cast to usize or error if unable.
    async fn read_varint_usize_with_limit(&mut self, limit: Option<&mut usize>) -> Result<usize> {
        let n = self.read_varint_with_limit(limit).await?;
        n.try_into()
            .map_err(|_| anyhow!("overflow casting var len int to usize: {}", n).into())
    }

    async fn read_varint_usize(&mut self) -> Result<usize> {
        self.read_varint_usize_with_limit(None).await
    }

    // read a varbytes (varint encoding length, followed by bytes). if limit is Some, decrements
    // the referenced usize by the number of bytes read, or errors if doing so would underflow.
    async fn read_varbytes_with_limit(
        &mut self,
        mut limit: Option<&mut usize>,
    ) -> Result<MultiBytes> {
        let len = self
            .read_varint_usize_with_limit(limit.as_mut().map(|l| &mut **l))
            .await?;
        if let Some(limit) = limit.as_mut() {
            let Some(new_limit) = limit.checked_sub(len) else {
                return Err(TooFewBytesError.into());
            };
            **limit = new_limit;
        }
        self.read_zc(len).await
    }
}

// ==== the actual API for reading frames ====

/// Typed API for reading a sequence of Aqueduct frames from a QUIC stream or datagram.
pub struct Frames(Reader);

impl Frames {
    pub fn from_stream(stream: quinn::RecvStream) -> Self {
        Frames(Reader::Stream(quic_zc::QuicStreamReader::new(stream)))
    }

    pub fn from_datagram(datagram: Bytes) -> Self {
        Frames(Reader::Datagram(datagram.into()))
    }

    // begin reading a frame.
    pub async fn frame(mut self) -> Result<Option<Frame>> {
        if self.0.is_done().await? {
            return Ok(None);
        }

        let tag_byte = self.0.read_byte().await?;
        let tag = FrameTag::from_byte(self.0.read_byte().await?)
            .ok_or_else(|| anyhow!("unknown frame tag byte: {}", tag_byte))?;
        Ok(Some(match tag {
            FrameTag::Version => Frame::Version(self.version().await?),
            FrameTag::AckVersion => Frame::AckVersion(self),
            FrameTag::ConnectionHeaders => {
                Frame::ConnectionHeaders(self.connection_headers().await?)
            }
            FrameTag::RouteTo => Frame::RouteTo(self.route_to().await?),
            FrameTag::Message => Frame::Message(self.message().await?),
            FrameTag::SentUnreliable => Frame::SentUnreliable(self.sent_unreliable().await?),
            FrameTag::FinishSender => Frame::FinishSender(self.finish_sender().await?),
            FrameTag::CancelSender => Frame::CancelSender(self),
            FrameTag::AckReliable => Frame::AckReliable(self.ack_nack_ranges().await?),
            FrameTag::AckNackUnreliable => Frame::AckNackUnreliable(self.ack_nack_ranges().await?),
            FrameTag::CloseReceiver => Frame::CloseReceiver(self),
            FrameTag::ForgetChannel => Frame::ForgetChannel(self.forget_channel().await?),
        }))
    }

    async fn version(mut self) -> Result<Self> {
        for b in VERSION_FRAME_MAGIC_BYTES {
            if self.0.read_byte().await? != b {
                return Err(anyhow!("wrong VERSION frame magic byte sequence").into());
            }
        }
        for b in VERSION_FRAME_HUMAN_TEXT {
            if self.0.read_byte().await? != b {
                return Err(anyhow!("wrong VERSION frame human text").into());
            }
        }
        let version_number_length = self.0.read_varint_usize().await?;
        let mut buf_space = [0; 0xff];
        if version_number_length > buf_space.len() {
            return Err(anyhow!("unreasonably long VERSION frame version number").into());
        }
        let buf = &mut buf_space[..version_number_length];
        self.0.read(buf).await?;
        let version_num =
            ascii_to_str(buf).ok_or(anyhow::Error::msg("non-ASCII version number"))?;
        if version_num != VERSION {
            return Err(anyhow!("unknown VERSION frame version number: {:?}", version_num).into());
        }
        Ok(self)
    }

    async fn connection_headers(mut self) -> Result<ConnectionHeaders> {
        let headers_bytes = self.0.read_varint_usize().await?;
        Ok(ConnectionHeaders(self.0, headers_bytes))
    }

    async fn route_to(mut self) -> Result<RouteTo> {
        let chan_id = ChanId(self.0.read_varint().await?);
        Ok(RouteTo {
            chan_id,
            next: self,
        })
    }

    async fn message(mut self) -> Result<Message> {
        let message_num = self.0.read_varint().await?;
        let message_headers_bytes = self.0.read_varint_usize().await?;
        Ok(Message {
            message_num,
            message_headers: MessageHeaders(self.0, message_headers_bytes),
        })
    }

    async fn sent_unreliable(mut self) -> Result<SentUnreliable> {
        let count = self.0.read_varint().await?;
        Ok(SentUnreliable { count, next: self })
    }

    async fn finish_sender(mut self) -> Result<FinishSender> {
        let sent_reliable = self.0.read_varint().await?;
        Ok(FinishSender {
            sent_reliable,
            next: self,
        })
    }

    async fn ack_nack_ranges(mut self) -> Result<AckNackRanges> {
        let bytes = self.0.read_varint_usize().await?;
        Ok(AckNackRanges(self.0, bytes))
    }

    async fn forget_channel(mut self) -> Result<ForgetChannel> {
        let chan_id = ChanId(self.0.read_varint().await?);
        Ok(ForgetChannel {
            chan_id,
            next: self,
        })
    }
}

pub enum Frame {
    Version(Frames),
    AckVersion(Frames),
    ConnectionHeaders(ConnectionHeaders),
    RouteTo(RouteTo),
    Message(Message),
    SentUnreliable(SentUnreliable),
    FinishSender(FinishSender),
    CancelSender(Frames),
    AckReliable(AckNackRanges),
    AckNackUnreliable(AckNackRanges),
    CloseReceiver(Frames),
    ForgetChannel(ForgetChannel),
}

pub struct ConnectionHeaders(Reader, usize);

impl ConnectionHeaders {
    pub fn remaining_bytes(&self) -> usize {
        self.1
    }

    pub async fn header(&mut self) -> Result<(MultiBytes, MultiBytes)> {
        Ok((
            self.0.read_varbytes_with_limit(Some(&mut self.1)).await?,
            self.0.read_varbytes_with_limit(Some(&mut self.1)).await?,
        ))
    }

    pub fn done(self) -> Frames {
        assert_eq!(self.remaining_bytes(), 0, "not done");
        Frames(self.0)
    }
}

pub struct RouteTo {
    pub chan_id: ChanId,
    pub next: Frames,
}

pub struct Message {
    pub message_num: u64,
    pub message_headers: MessageHeaders,
}

pub struct MessageHeaders(Reader, usize);

impl MessageHeaders {
    pub fn remaining_bytes(&self) -> usize {
        self.1
    }

    pub async fn header(&mut self) -> Result<(MultiBytes, MultiBytes)> {
        Ok((
            self.0.read_varbytes_with_limit(Some(&mut self.1)).await?,
            self.0.read_varbytes_with_limit(Some(&mut self.1)).await?,
        ))
    }

    pub async fn done(mut self) -> Result<MessageAttachments> {
        assert_eq!(self.remaining_bytes(), 0, "not done");
        let attachments_bytes = self.0.read_varint_usize().await?;
        Ok(MessageAttachments(self.0, attachments_bytes))
    }
}

pub struct MessageAttachments(Reader, usize);

impl MessageAttachments {
    pub fn remaining_bytes(&self) -> usize {
        self.1
    }

    pub async fn attachment(mut self) -> Result<MessageAttachment> {
        let channel = ChanId(self.0.read_varint_with_limit(Some(&mut self.1)).await?);
        let headers_bytes = self
            .0
            .read_varint_usize_with_limit(Some(&mut self.1))
            .await?;

        let Some(attachments_remaining_bytes) = self.1.checked_sub(headers_bytes) else {
            return Err(TooFewBytesError.into());
        };

        Ok(MessageAttachment {
            channel,
            channel_headers: ChannelHeaders {
                reader: self.0,
                headers_remaining_bytes: headers_bytes,
                attachments_remaining_bytes,
            },
        })
    }

    pub async fn done(mut self) -> Result<MessagePayload> {
        assert_eq!(self.remaining_bytes(), 0, "not done");
        let payload_bytes = self.0.read_varint_usize().await?;
        Ok(MessagePayload(self.0, payload_bytes))
    }
}

pub struct MessageAttachment {
    pub channel: ChanId,
    pub channel_headers: ChannelHeaders,
}

pub struct ChannelHeaders {
    reader: Reader,
    headers_remaining_bytes: usize,
    attachments_remaining_bytes: usize,
}

impl ChannelHeaders {
    pub fn remaining_bytes(&self) -> usize {
        self.headers_remaining_bytes
    }

    pub async fn header(&mut self) -> Result<(MultiBytes, MultiBytes)> {
        Ok((
            self.reader
                .read_varbytes_with_limit(Some(&mut self.headers_remaining_bytes))
                .await?,
            self.reader
                .read_varbytes_with_limit(Some(&mut self.headers_remaining_bytes))
                .await?,
        ))
    }

    pub fn done(self) -> MessageAttachments {
        assert_eq!(self.remaining_bytes(), 0, "not done");
        MessageAttachments(self.reader, self.attachments_remaining_bytes)
    }
}

pub struct MessagePayload(Reader, usize);

impl MessagePayload {
    pub fn bytes(&self) -> usize {
        self.1
    }

    pub async fn read(mut self) -> Result<(MultiBytes, Frames)> {
        let payload = self.0.read_zc(self.1).await?;
        Ok((payload, Frames(self.0)))
    }
}

pub struct SentUnreliable {
    pub count: u64,
    pub next: Frames,
}

pub struct FinishSender {
    pub sent_reliable: u64,
    pub next: Frames,
}

pub struct AckNackRanges(Reader, usize);

impl AckNackRanges {
    pub fn remaining_bytes(&self) -> usize {
        self.1
    }

    pub async fn delta(&mut self) -> Result<u64> {
        self.0.read_varint_with_limit(Some(&mut self.1)).await
    }

    pub fn done(self) -> Frames {
        assert_eq!(self.remaining_bytes(), 0, "not done");
        Frames(self.0)
    }
}

pub struct ForgetChannel {
    pub chan_id: ChanId,
    pub next: Frames,
}
