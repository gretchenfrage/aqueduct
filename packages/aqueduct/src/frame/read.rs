//! Typed API for reading frames from quic streams and datagrams.

use crate::{
    zero_copy::{
        self,
        MultiBytes,
        TooFewBytesError,
    },
    frame::common::*,
};
use std::sync::{
    Arc,
    atomic::{
        Ordering::Relaxed,
        AtomicBool,
    },
};
use bytes::Bytes;
use anyhow::{anyhow, Error, Result};


// ==== error handling ====


/// Result type for reading Aqueduct frames from a QUIC stream/datagram wherein receiving a reset
/// of the underlying stream may need to be handled gracefully.
pub type ResetResult<T> = std::result::Result<T, ResetError>;

/// Error type for reading Aqueduct frames from a QUIC stream/datagram wherein receiving a reset
/// of the underlying stream may need to be handled gracefully.
#[derive(Debug)]
pub enum ResetError {
    /// A reset was received on the underlying QUIC stream.
    Reset(ResetCode),
    /// Some unrecoverable error occurred.
    Other(Error),
}

impl ResetError {
    /// Convert to `Error` by treating QUIC stream reset in the current scenario as unrecoverable.
    fn no_reset(self, dbg_ctx: &str) -> Error {
        match self {
            ResetError::Reset(code) =>
                anyhow!("unexpected stream reset {:?} ({})", code, dbg_ctx),
            ResetError::Other(e) => e,
        }
    }
}

impl From<Error> for ResetError {
    fn from(e: Error) -> Self {
        ResetError::Other(e)
    }
}

impl From<TooFewBytesError> for ResetError {
    fn from(TooFewBytesError: TooFewBytesError) -> Self {
        Error::msg("too few bytes").into()
    }
}

impl From<zero_copy::quic::ReadError> for ResetError {
    fn from(e: zero_copy::quic::ReadError) -> Self {
        match e {
            zero_copy::quic::ReadError::Quic(quinn::ReadError::Reset(reset_code_int)) => {
                ResetCode::from_u64(reset_code_int.into_inner())
                    .map(ResetError::Reset)
                    .unwrap_or_else(|| ResetError::Other(
                        anyhow!("invalid reset code int: {}", reset_code_int)
                    ))
            },
            zero_copy::quic::ReadError::Quic(e) => ResetError::Other(e.into()),
            zero_copy::quic::ReadError::TooFewBytes => TooFewBytesError.into(),
        }
    }
}


// ==== module-internal utilities ====


/// Utility internal to frame reading module:
///
/// - abstracts over zero-copy layer for QUIC stream versus datagram
/// - provides helper methods for reading common primitives
/// - is a place to store some state used for validation
struct QuicReader {
    /// Source of the byte data.
    source: QuicReaderSource,
    /// Which side of the Aqueduct connection the local side is.
    side: Side,
    /// Whether a Version frame has been received on this connection from any stream or datagram.
    received_version: Arc<AtomicBool>,
}

/// Source field of QuicReader.
enum QuicReaderSource {
    Stream(zero_copy::quic::QuicStreamReader),
    Datagram(MultiBytes),
}

impl QuicReader {
    // read bytes and copy to buf
    async fn read(&mut self, buf: &mut [u8]) -> ResetResult<()> {
        Ok(match &mut self.source {
            &mut QuicReaderSource::Stream(ref mut inner) => inner.read(buf).await?,
            &mut QuicReaderSource::Datagram(ref mut inner) => inner.read(buf)?,
        })
    }

    // read bytes zero-copy-ly
    async fn read_zc(&mut self, n: usize) -> ResetResult<MultiBytes> {
        Ok(match &mut self.source {
            &mut QuicReaderSource::Stream(ref mut inner) => inner.read_zc(n).await?,
            &mut QuicReaderSource::Datagram(ref mut inner) => inner.read_zc(n)?,
        })
    }

    // whether the data source ends immediately after bytes read so far
    async fn is_done(&mut self) -> ResetResult<bool> {
        Ok(match &mut self.source {
            &mut QuicReaderSource::Stream(ref mut inner) => inner.is_done().await?,
            &mut QuicReaderSource::Datagram(ref inner) => inner.len() == 0,
        })
    }

    // read a single byte
    async fn read_byte(&mut self) -> ResetResult<u8> {
        let mut buf = [0];
        self.read(&mut buf).await?;
        let [b] = buf;
        Ok(b)
    }

    // read a frame type byte
    async fn read_frame_type(&mut self) -> ResetResult<FrameType> {
        let b = self.read_byte().await?;
        FrameType::from_byte(b).ok_or_else(|| anyhow!("invalid frame type byte: {}", b).into())
    }

    // read a var len int. if limit is Some, decrements the referenced usize by the number of bytes
    // read, or errors if doing so would underflow.
    async fn read_vli_with_limit(&mut self, mut limit: Option<&mut usize>) -> ResetResult<u64> {
        let mut i: u64 = 0;
        for x in 0..8 {
            if let Some(limit) = limit.as_mut() {
                if **limit == 0 {
                    return Err(anyhow!("too few bytes").into());
                }
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
            if **limit == 0 {
                return Err(anyhow!("too few bytes").into());
            }
            **limit -= 1;
        }
        let b = self.read_byte().await?;
        i |= (b as u64) << VLI_FINAL_SHIFT;
        Ok(i)
    }

    // read a var len int.
    async fn read_vli(&mut self) -> ResetResult<u64> {
        Ok(self.read_vli_with_limit(None).await?)
    }

    // read a var len int, then cast to usize or error if unable.
    async fn read_vli_usize(&mut self) -> ResetResult<usize> {
        let n = self.read_vli().await?;
        n.try_into().map_err(|_| anyhow!("overflow casting var len int to usize: {}", n).into())
    }

    // read a channel id.
    async fn read_chan_id(&mut self) -> ResetResult<ChanId> {
        self.read_vli().await.map(ChanId)
    }

    // read and validate the contents of a Version frame. if successful, set the `received_version`
    // connection-global atomic to true.
    async fn read_version_frame(&mut self) -> ResetResult<()> {
        for b in VERSION_FRAME_MAGIC_BYTES {
            if self.read_byte().await? != b {
                return Err(anyhow!("wrong Version frame magic byte sequence").into());
            }
        }
        for b in VERSION_FRAME_HUMAN_TEXT {
            if self.read_byte().await? != b {
                return Err(anyhow!("wrong Version frame human text").into());
            }
        }
        let version_number_length = self.read_vli_usize().await?;
        let mut buf_space = [0; 0xff];
        if version_number_length > buf_space.len() {
            return Err(anyhow!("unreasonably long Version frame version number").into());
        }
        let buf = &mut buf_space[..version_number_length];
        self.read(buf).await?;
        let version_num = ascii_to_str(buf).ok_or(Error::msg("non-ASCII version number"))?;
        if version_num != VERSION {
            return
                Err(anyhow!("mismatching Version frame version number: {:?}", version_num).into());
        }
        self.received_version.store(true, Relaxed);
        Ok(())
    }

    // return Err unless the `received_version` connection-global atomic is set to true.
    fn ensure_received_version(&mut self, dbg_ctx: &str) -> Result<()> {
        if self.received_version.load(Relaxed) {
            Ok(())
        } else {
            Err(anyhow!(
                "received non-Version frame before receiving Version frame in connection ({})",
                dbg_ctx,
            ).into())
        }
    }
}

// utility internal to the frame reading module for reading pos-neg range data.
struct PosNegRangeReader {
    reader: QuicReader,
    remaining_byte_len: usize,
    zero_delta_validation_state: ZeroDeltaValidationState,
}

// state for validating pos-neg range data deltas for not containing invalid zeroes.
#[derive(Copy, Clone, PartialEq)]
enum ZeroDeltaValidationState {
    HasNotReadAny,
    HasReadOnlyInitialZeroDelta,
    HasReadNonZeroDelta,
}

impl PosNegRangeReader {
    // construct with the length prefix having already been read.
    fn new(reader: QuicReader, byte_len: usize) -> Self {
        PosNegRangeReader {
            reader,
            remaining_byte_len: byte_len,
            zero_delta_validation_state: ZeroDeltaValidationState::HasNotReadAny,
        }
    }

    // read the next delta, if there is one.
    async fn next_delta(&mut self) -> ResetResult<Option<u64>> {
        if self.remaining_byte_len == 0 {
            if self.zero_delta_validation_state ==
                ZeroDeltaValidationState::HasReadOnlyInitialZeroDelta
            {
                Err(anyhow!("pos-neg range data contained only initial zero delta").into())
            } else {
                Ok(None)
            }
        } else {
            let delta = self.reader.read_vli_with_limit(Some(&mut self.remaining_byte_len)).await?;
            if delta == 0 {
                if self.zero_delta_validation_state == ZeroDeltaValidationState::HasNotReadAny {
                    self.zero_delta_validation_state =
                        ZeroDeltaValidationState::HasReadOnlyInitialZeroDelta;
                } else {
                    return Err(anyhow!("pos-neg range data contained invalid zero delta").into());
                }
            } else {
                self.zero_delta_validation_state = ZeroDeltaValidationState::HasReadNonZeroDelta;
            }
            Ok(Some(delta))
        }
    }

    // assert done, and convert to inner QuicReader
    fn done(self) -> QuicReader {
        assert!(
            self.remaining_byte_len == 0,
            "PosNegRangeReader not done (Aqueduct implementation bug)",
        );
        self.reader
    }
}


// ==== the actual API for reading frames ====


/// Begin reading Aqueduct frames from a QUIC unidirectional stream.
pub async fn uni_stream(
    stream: quinn::RecvStream,
    side: Side,
    received_version: Arc<AtomicBool>,
) -> ResetResult<UniFrames> {
    UniFrames::begin_frame(QuicReader {
        source: QuicReaderSource::Stream(zero_copy::quic::QuicStreamReader::new(stream)),
        side,
        received_version,
    }).await
}

/// Begin reading Aqueduct frames from a QUIC datagram.
pub async fn datagram(
    datagram: Bytes,
    side: Side,
    received_version: Arc<AtomicBool>,
) -> Result<UniFrames> {
    // this function could be made not async, but it's not worthwhile to do so.
    UniFrames::begin_frame(QuicReader {
        source: QuicReaderSource::Datagram(datagram.into()),
        side,
        received_version,
    }).await.map_err(|e| match e {
        ResetError::Reset(_) => unreachable!("a datagram cannot experience stream reset"),
        ResetError::Other(e) => e,
    })
}

/// Typed API for starting to read Aqueduct frames from a QUIC unidirectional stream or datagram.
#[must_use]
pub enum UniFrames {
    /// Stream/datagram contains (maybe a Version frame, then) one or more Message frames.
    Message(Message),
    /// Stream/datagram contains (maybe a Version frame, then) one ClosedChannelLost frame.
    ClosedChannelLost(ClosedChannelLost),
}

impl UniFrames {
    /// Read a frame type byte and construct self based on it.
    async fn begin_frame(mut reader: QuicReader) -> ResetResult<Self> {
        let mut ft = reader.read_frame_type().await?;

        if ft == FrameType::Version {
            reader.read_version_frame().await?;
            ft = reader.read_frame_type().await?;
        } else {
            reader.ensure_received_version("UniFrames")?;
        }           

        Ok(match ft {
            FrameType::Message => UniFrames::Message(Message(reader)),
            FrameType::ClosedChannelLost =>
                UniFrames::ClosedChannelLost(ClosedChannelLost(reader)),
            ft => return Err(anyhow!("unexpected frame type {:?} in UniFrames context", ft).into()),
        })
    }
}

/// Part of typed API for reading a Message frame.
#[must_use]
pub struct Message(QuicReader);

impl Message {
    /// Whether this Message frame was sent reliably.
    pub fn reliable(&self) -> bool {
        matches!(&self.0.source, &QuicReaderSource::Stream(_))
    }

    /// Read the `sent_on` field.
    pub async fn sent_on(mut self) -> ResetResult<(Message2, ChanIdLocalReceiver)> {
        let o = self.0.read_chan_id().await?;
        let SortByDir::LocalReceiver(o) = o.sort_by_dir(self.0.side) else {
            return Err(anyhow!(
                "received Message frame with sent_on field in wrong direction"
            ).into());
        };
        Ok((Message2(self.0), o))
    }
}

/// Part of typed API for reading a Message frame.
#[must_use]
pub struct Message2(QuicReader);

impl Message2 {
    /// Read the `message_num` field.
    pub async fn message_num(mut self) -> ResetResult<(Message3, u64)> {
        let o = self.0.read_vli().await?;
        Ok((Message3(self.0), o))
    }
}

/// Part of typed API for reading a Message frame.
#[must_use]
pub struct Message3(QuicReader);

impl Message3 {
    /// Read the byte length of the `attachments` field (may not be the number of elements).
    pub async fn attachments_len(mut self) -> ResetResult<(Message4, usize)> {
        let len = self.0.read_vli_usize().await?;
        Ok((Message4(self.0, len), len))
    }
}

/// Part of typed API for reading a Message frame.
#[must_use]
pub struct Message4(QuicReader, usize);

impl Message4 {
    /// Read the next element of the `attachments` field, or return `None` if there are no more.
    pub async fn next_attachment(&mut self) -> ResetResult<Option<ChanIdRemotelyMinted>> {
        if self.1 == 0 {
            return Ok(None);
        }
        let o = ChanId(self.0.read_vli_with_limit(Some(&mut self.1)).await?);
        let SortByMintedBy::RemotelyMinted(o) = o.sort_by_minted_by(self.0.side) else {
            return Err(anyhow!(
                "received Message frame with attached channel minted by wrong side"
            ).into());
        };
        Ok(Some(o))
    }

    /// Stop reading the `attachments` field. Panics if there are un-read attachments.
    pub fn done(self) -> Message5 {
        assert!(self.1 == 0, "Message4.done without reading all attachments");
        Message5(self.0)
    }
}

/// Part of typed API for reading a Message frame.
#[must_use]
pub struct Message5(QuicReader);

impl Message5 {
    /// Read the byte length of the `payload` field.
    pub async fn payload_len(mut self) -> ResetResult<(Message6, usize)> {
        let len = self.0.read_vli_usize().await?;
        Ok((Message6(self.0, len), len))
    }
}

/// Part of typed API for reading a Message frame.
#[must_use]
pub struct Message6(QuicReader, usize);

impl Message6 {
    /// Read the entire contents of the `payload` field.
    pub async fn payload(mut self) -> ResetResult<(NextMessage, MultiBytes)> {
        let o = self.0.read_zc(self.1).await?;
        Ok((NextMessage(self.0), o))
    }
}

/// Typed API for possibly reading more Message frames after at least one Message frame has already
/// been read from the underlying byte sequence.
#[must_use]
pub struct NextMessage(QuicReader);

impl NextMessage {
    /// Begin reading the next Message frame, or return `None` if the underlying byte sequence
    /// finishes gracefully after the previous Message frame.
    pub async fn next_message(mut self) -> ResetResult<Option<Message>> {
        Ok(if self.0.is_done().await? {
            None
        } else {
            let ft = self.0.read_frame_type().await?;
            if ft != FrameType::Message {
                return Err(anyhow!("unexpected frame type {:?} in NextMessage context", ft).into());
            }
            Some(Message(self.0))
        })
    }
}

/// Typed API for reading a ClosedChannelLost frame.
#[must_use]
pub struct ClosedChannelLost(QuicReader);

impl ClosedChannelLost {
    /// Read the `channel_id` field.
    pub async fn chan_id(mut self) -> Result<(ExpectFinishNoReset, ChanIdRemotelyMinted)> {
        let o = self.0.read_chan_id().await.map_err(|e| e.no_reset("after ClosedChannelLost"))?;
        let SortByMintedBy::RemotelyMinted(o) = o.sort_by_minted_by(self.0.side) else {
            return Err(anyhow!("received ClosedChannelLost frame with locally minted channel id"));
        };
        Ok((ExpectFinishNoReset(self.0, "after ClosedChannelLost"), o))
    }
}

/// Begin reading Aqueduct frames from a QUIC bidirectional stream.
pub async fn bidi_stream(
    stream: quinn::RecvStream,
    side: Side,
    received_version: Arc<AtomicBool>,
) -> ResetResult<BidiFrames> {
    let mut reader = QuicReader {
        source: QuicReaderSource::Stream(zero_copy::quic::QuicStreamReader::new(stream)),
        side,
        received_version,
    };

    let mut ft = reader.read_frame_type().await?;

    if ft == FrameType::Version {
        reader.read_version_frame().await?;
        ft = reader.read_frame_type().await?;
    } else {
        if ft == FrameType::ConnectionControl {
            return Err(anyhow!("ConnectionControl frame without preceding Version frame").into());
        }
        reader.ensure_received_version("BidiFrames")?;
    }

    Ok(match ft {
        FrameType::ConnectionControl => BidiFrames::ConnectionControl(ConnectionControl(reader)),
        FrameType::ChannelControl => BidiFrames::ChannelControl(ChannelControl(reader)),
        ft => return Err(anyhow!("unexpected frame type {:?} in BidiFrames context", ft).into()),
    })
}

/// Typed API for starting to read Aqueduct frames from a QUIC bidirectional stream.
#[must_use]
pub enum BidiFrames {
    /// Stream contains a Version frame, then a ConnectionControl frame, making the stream
    /// the connection control stream.
    ConnectionControl(ConnectionControl),
    /// Stream contains (maybe a Version frame, then) a ChannelControl frame, making the stream a
    /// channel control stream.
    ChannelControl(ChannelControl),
}

/// Typed API for reading a ConnectionControl frame.
#[must_use]
pub struct ConnectionControl(QuicReader);

impl ConnectionControl {
    /// Skip past the `headers` field in its entirety.
    ///
    /// TODO: Actually utilize headers.
    pub async fn skip_headers_inner(mut self) -> Result<ExpectFinishNoReset> {
        let len = self.0.read_vli_usize().await.map_err(|e| e.no_reset("after ConnectionControl"))?;
        self.0.read_zc(len).await.map_err(|e| e.no_reset("after ConnectionControl"))?;
        Ok(ExpectFinishNoReset(self.0, "after ConnectionControl"))
    }
}

/// Typed API for reading a ChannelControl frame.
#[must_use]
pub struct ChannelControl(QuicReader);

impl ChannelControl {
    /// Read the `channel_id` field.
    pub async fn chan_id(mut self) -> ResetResult<ChanCtrlFrames> {
        let o = self.0.read_chan_id().await?;
        let SortByMintedBy::LocallyMinted(o) = o.sort_by_minted_by(self.0.side) else {
            return Err(anyhow!(
                "received ChannelControl frame with remotely minted channel ID"
            ).into());
        };
        Ok(match o.sort_by_dir(self.0.side) {
            SortByDir::LocalSender(o) =>
                ChanCtrlFrames::Sender(SenderChanCtrlFrames(self.0), o),
            SortByDir::LocalReceiver(o) =>
                ChanCtrlFrames::Receiver(ReceiverChanCtrlFrames(self.0), o),
        })
    }
}

/// Typed API for reading Aqueduct frames from a channel control stream.
#[must_use]
pub enum ChanCtrlFrames {
    /// The local side is the sender side of the channel.
    Sender(SenderChanCtrlFrames, ChanIdLocalSenderLocallyMinted),
    /// The local side is the receiver side of the channel.
    Receiver(ReceiverChanCtrlFrames, ChanIdLocalReceiverLocallyMinted),
}

/// Typed API for reading Aqueduct frames from a channel control stream for which the local side is
/// the sender side of the channel.
#[must_use]
pub struct SenderChanCtrlFrames(QuicReader);

impl SenderChanCtrlFrames {
    /// Begin reading the next frame.
    pub async fn next_frame(mut self) -> ResetResult<SenderChanCtrlFrame> {
        Ok(match self.0.read_frame_type().await? {
            FrameType::AckReliable =>
                SenderChanCtrlFrame::AckReliable(AckReliable(self.0)),
            FrameType::AckNackUnreliable =>
                SenderChanCtrlFrame::AckNackUnreliable(AckNackUnreliable(self.0)),
            FrameType::CloseReceiver =>
                SenderChanCtrlFrame::CloseReceiver(CloseReceiver(self.0)),
            ft =>
                return Err(
                    anyhow!("unexpected frame type {:?} in SenderChanCtrlFrames context", ft).into()
                ),
        })
    }
}

/// Typed API for starting to read an Aqueduct frame from a channel control stream for which the
/// local side is the sender side of the channel.
#[must_use]
pub enum SenderChanCtrlFrame {
    /// The frame currently being read is an AckReliable frame.
    AckReliable(AckReliable),
    /// The frame currently being read is an AckNackUnreliable frame.
    AckNackUnreliable(AckNackUnreliable),
    /// The frame currently being read is a CloseReceiver frame.
    CloseReceiver(CloseReceiver),
}

/// Part of typed API for reading an AckReliable frame.
#[must_use]
pub struct AckReliable(QuicReader);

impl AckReliable {
    /// Read the byte length of the `acks` field (may not be the number of elements).
    pub async fn acks_len(mut self) -> ResetResult<(AckReliable2, usize)> {
        let o = self.0.read_vli_usize().await?;
        Ok((AckReliable2(PosNegRangeReader::new(self.0, o)), o))
    }
}

/// Part of typed API for reading an AckReliable frame.
#[must_use]
pub struct AckReliable2(PosNegRangeReader);

impl AckReliable2 {
    /// Read the next pos-neg range delta of the `acks` field, or return `None` if there are no
    /// more.
    ///
    /// The first range represents acks, the second range represents not-yet-acks, and then
    /// they alternate back and forth.
    pub async fn next_delta(&mut self) -> ResetResult<Option<u64>> {
        self.0.next_delta().await
    }

    /// Stop reading the `acks` field. Panics if there are un-read deltas.
    pub fn done(self) -> SenderChanCtrlFrames {
        SenderChanCtrlFrames(self.0.done())
    }
}

/// Part of typed API for reading an AckNackUnreliable frame.
#[must_use]
pub struct AckNackUnreliable(QuicReader);

impl AckNackUnreliable {
    /// Read the byte length of the `ack_nacks` field (may not be the number of elements).
    pub async fn ack_nacks_len(mut self) -> ResetResult<(AckNackUnreliable2, usize)> {
        let o = self.0.read_vli_usize().await?;
        Ok((AckNackUnreliable2(PosNegRangeReader::new(self.0, o)), o))
    }
}

/// Part of typed API for reading an AckNackUnreliable frame.
#[must_use]
pub struct AckNackUnreliable2(PosNegRangeReader);

impl AckNackUnreliable2 {
    /// Read the next pos-neg range delta of the `ack_nacks` field, or return `None` if there are
    /// no more.
    ///
    /// The first range represents acks, the second range represents nacks, and then they alternate
    /// back and forth.
    pub async fn next_delta(&mut self) -> ResetResult<Option<u64>> {
        self.0.next_delta().await
    }

    /// Stop reading the `ack_nacks` field. Panics if there are un-read deltas.
    pub fn done(self) -> SenderChanCtrlFrames {
        SenderChanCtrlFrames(self.0.done())
    }
}

/// Part of typed API for reading a CloseReceiver frame.
#[must_use]
pub struct CloseReceiver(QuicReader);

impl CloseReceiver {
    /// Read the byte length of the `final_reliable_ack_nacks` field (may not be the number of
    /// elements).
    pub async fn final_reliable_ack_nacks_len(mut self) -> ResetResult<(CloseReceiver2, usize)> {
        let o = self.0.read_vli_usize().await?;
        Ok((CloseReceiver2(PosNegRangeReader::new(self.0, o)), o))
    }
}

/// Part of typed API for reading an CloseReceiver frame.
#[must_use]
pub struct CloseReceiver2(PosNegRangeReader);

impl CloseReceiver2 {
    /// Read the next pos-neg range delta of the `final_reliable_ack_nacks` field, or return
    /// `None` if there are no more.
    ///
    /// The first range represents acks, the second range represents nacks, and then they alternate
    /// back and forth.
    pub async fn next_delta(&mut self) -> ResetResult<Option<u64>> {
        self.0.next_delta().await
    }

    /// Stop reading the `final_reliable_ack_nacks` field. Panics if there are un-read deltas.
    pub fn done(self) -> ExpectFinishOrReset {
        ExpectFinishOrReset(self.0.done(), "after CloseReceiver")
    }
}

/// Typed API for reading Aqueduct frames from a channel control stream for which the local side is
/// the receiver side of the channel.
#[must_use]
pub struct ReceiverChanCtrlFrames(QuicReader);

impl ReceiverChanCtrlFrames {
    /// Begin reading the next frame.
    pub async fn next_frame(mut self) -> ResetResult<ReceiverChanCtrlFrame> {
        Ok(match self.0.read_frame_type().await? {
            FrameType::SentUnreliable =>
                ReceiverChanCtrlFrame::SentUnreliable(SentUnreliable(self.0)),
            FrameType::FinishSender =>
                ReceiverChanCtrlFrame::FinishSender(FinishSender(self.0)),
            ft =>
                return Err(anyhow!(
                    "unexpected frame type {:?} in ReceiverChanCtrlFrames context", ft
                ).into()),
        })
    }
}

/// Typed API for starting to read an Aqueduct frame from a channel control stream for which the
/// local side is the receiver side of the channel.
#[must_use]
pub enum ReceiverChanCtrlFrame {
    /// The frame currently being read is an SentUnreliable frame.
    SentUnreliable(SentUnreliable),
    /// The frame currently being read is an FinishSender frame.
    FinishSender(FinishSender),
}

/// Typed API for reading a SentUnreliable frame.
#[must_use]
pub struct SentUnreliable(QuicReader);

impl SentUnreliable {
    /// Read the `delta` field.
    pub async fn delta(mut self) -> ResetResult<(ReceiverChanCtrlFrames, u64)> {
        let o = self.0.read_vli().await?;
        if o == 0 {
            return Err(anyhow!("SentUnreliable delta of 0").into())
        }
        Ok((ReceiverChanCtrlFrames(self.0), o))
    }
}

/// Typed API for reading a FinishSender frame.
#[must_use]
pub struct FinishSender(QuicReader);

impl FinishSender {
    /// Read the `reliable_count` field. 
    pub async fn reliable_count(mut self) -> ResetResult<(ExpectFinishOrReset, u64)> {
        let o = self.0.read_vli().await?;
        Ok((ExpectFinishOrReset(self.0, "after FinishSender"), o))
    }
}

/// Typed API for expecting a sequence of Aqueduct frames to gracefully finish after previously
/// received frames, wherein receiving a reset of the underlying stream may still need to be
/// handled gracefully.
#[must_use]
pub struct ExpectFinishOrReset(QuicReader, &'static str);

impl ExpectFinishOrReset {
    /// Wait to observe the graceful finishing of the underlying QUIC stream/datagram.
    pub async fn finish(mut self) -> ResetResult<()> {
        if self.0.is_done().await? {
            Ok(())
        } else {
            Err(anyhow!("expected end of stream/datagram ({})", self.1).into())
        }
    }
}

/// Typed API for expecting a sequence of Aqueduct frames to gracefully finish after previously
/// received frames.
#[must_use]
pub struct ExpectFinishNoReset(QuicReader, &'static str);

impl ExpectFinishNoReset {
    /// Wait to observe the graceful finishing of the underlying QUIC stream/datagram.
    pub async fn finish(mut self) -> Result<()> {
        if self.0.is_done().await.map_err(|e| e.no_reset(self.1))? {
            Ok(())
        } else {
            Err(anyhow!("expected end of stream/datagram ({})", self.1).into())
        }
    }
}
