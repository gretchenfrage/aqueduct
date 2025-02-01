//! Typed API for writing frames to quic streams and datagrams.

use crate::{
    zero_copy::MultiBytes,
    frame::common::*,
};
use quinn::{SendStream, Connection, SendDatagramError};
use anyhow::*;


// ==== internal Writer utility ====


// utility internal to this module that wraps around MultiBytes and adds some helper functions.
#[derive(Default, Clone)]
struct Writer(MultiBytes);

impl Writer {
    // write bytes.
    fn write(&mut self, bytes: &[u8]) {
        self.0.write(bytes)
    }

    // write bytes zero-copy-ly.
    fn write_zc<B: Into<MultiBytes>>(&mut self, bytes: B) {
        self.0.write_zc(bytes);
    }

    // whether no bytes have been written
    fn is_empty(&self) -> bool {
        self.0.len() == 0
    }

    // write a var len int.
    fn write_vli(&mut self, mut i: u64) {
        for _ in 0..8 {
            let mut b = (i as u8 & VLI_MASK) as u8;
            i >>= 7;
            b |= ((i != 0) as u8) << 7;
            self.write(&[b]);
            if i == 0 {
                return;
            }
        }
        debug_assert!(i != 0 && (i >> 8) == 0);
        self.write(&[i as u8]);
    }

    // write a var len int, provided as a usize.
    fn write_vli_usize(&mut self, i: usize) {
        self.write_vli(i as u64);
    }

    // write a channel id.
    fn write_chan_id(&mut self, chan_id: ChanId) {
        self.write_vli(chan_id.0);
    }

    // write a variable length-prefixed byte array (zero-copy-ly).
    fn write_vlba<B: Into<MultiBytes>>(&mut self, bytes: B) {
        let bytes = bytes.into();
        self.write_vli_usize(bytes.len());
        self.write_zc(bytes);
    }

    // send data written to `self` on the provided QUIC stream (zero-copy-ly).
    async fn send_stream(self, stream: &mut SendStream) -> Result<()> {
        for fragment in self.0.fragments() {
            stream.write_chunk(fragment).await?;
        }
        Ok(())
    }

    // open new unidirectional QUIC stream, send written data on it, finish stream.
    async fn send_new_stream(self, conn: &Connection) -> Result<()> {
        let mut stream = conn.open_uni().await?;
        self.send_stream(&mut stream).await?;
        stream.finish().unwrap();
        Ok(())
    }

    // send written data in a QUIC datagram, or fall back to send_new_stream if too large.
    async fn send_datagram(self, conn: &Connection) -> Result<()> {
        let max_datagram_size = conn.max_datagram_size()
            .ok_or_else(|| anyhow!("datagrams disabled"))?;
        if self.0.len() > max_datagram_size {
            self.send_new_stream(conn).await
        } else {
            let bytes = self.0.defragment();
            if let Err(e) = conn.send_datagram(bytes.clone()) {
                if e == SendDatagramError::TooLarge {
                    let mut stream = conn.open_uni().await?;
                    stream.write_chunk(bytes).await?;
                    stream.finish().unwrap();
                    Ok(())
                } else {
                    Err(e.into())
                }
            } else {
                Ok(())
            }
        }
    }
}

impl Into<MultiBytes> for Writer {
    fn into(self) -> MultiBytes {
        self.0.into()
    }
}


// ==== API for writing frames ====


/// Typed API for writing a sequence of Aqueduct frames to a QUIC stream or datagram.
#[derive(Default)]
pub struct Frames(Writer);

impl Frames {
    /// Send bytes written to `self` on an existing QUIC stream.
    pub async fn send_stream(self, stream: &mut SendStream) -> Result<()> {
        self.0.send_stream(stream).await
    }

    /// Open a new unidirectional QUIC stream, send bytes written to `self` on it, then finish the
    /// stream.
    pub async fn send_new_stream(self, conn: &Connection) -> Result<()> {
        self.0.send_new_stream(conn).await
    }

    /// Send bytes written to `self` in a QUIC datagram, or fall back to `send_new_stream` if too
    /// large.
    pub async fn send_datagram(self, conn: &Connection) -> Result<()> {
        self.0.send_datagram(conn).await
    }

    /// Write this frame type to `self`.
    pub fn version(&mut self) {
        self.0.write(&[FrameType::Version as u8]);
        self.0.write(&VERSION_FRAME_MAGIC_BYTES);
        self.0.write(&VERSION_FRAME_HUMAN_TEXT);
        self.0.write_vlba(VERSION);
    }

    /// Write this frame type to `self`.
    pub fn connection_control(&mut self) {
        // TODO
        self.0.write(&[FrameType::ConnectionControl as u8]);
        self.0.write(&[0]);
    }

    /// Write this frame type to `self`.
    pub fn channel_control(&mut self, chan_id: ChanId) {
        self.0.write(&[FrameType::ChannelControl as u8]);
        self.0.write_chan_id(chan_id);
    }

    /// Write this frame type to `self`.
    pub fn message(
        &mut self,
        sent_on: ChanId,
        message_num: u64,
        attachments: Attachments,
        payload: MultiBytes,
    ) {
        self.0.write(&[FrameType::Message as u8]);
        self.0.write_chan_id(sent_on);
        self.0.write_vli(message_num);
        self.0.write_vlba(attachments.0);
        self.0.write_vlba(payload);
    }

    /// Write this frame type to `self`.
    pub fn sent_unreliable(&mut self, delta: u64) {
        self.0.write(&[FrameType::SentUnreliable as u8]);
        self.0.write_vli(delta);
    }

    /// Write this frame type to `self`.
    pub fn ack_reliable(&mut self, acks: PosNegRanges) {
        self.0.write(&[FrameType::AckReliable as u8]);
        self.0.write_vlba(acks.finalize());
    }

    /// Write this frame type to `self`.
    pub fn ack_nack_unreliable(&mut self, ack_nacks: PosNegRanges) {
        self.0.write(&[FrameType::AckNackUnreliable as u8]);
        self.0.write_vlba(ack_nacks.finalize());
    }

    /// Write this frame type to `self`.
    pub fn finish_sender(&mut self, reliable_count: u64) {
        self.0.write(&[FrameType::FinishSender as u8]);
        self.0.write_vli(reliable_count);
    }

    /// Write this frame type to `self`.
    pub fn close_receiver(&mut self, reliable_ack_nacks: PosNegRanges) {
        self.0.write(&[FrameType::CloseReceiver as u8]);
        self.0.write_vlba(reliable_ack_nacks.written.0);
    }

    /// Write this frame type to `self`.
    pub fn closed_channel_lost(&mut self, chan_id: ChanId) {
        self.0.write(&[FrameType::ClosedChannelLost as u8]);
        self.0.write_chan_id(chan_id);
    }
}

/// Typed API for encoding a sequence of Aqueduct message attachments to be included within an
/// Aqueduct frame.
#[derive(Default)]
pub struct Attachments(Writer);

impl Attachments {
    /// Encode an attachment.
    pub fn attachment(&mut self, chan_id: ChanId) {
        self.0.write_chan_id(chan_id);
    }
}

/// Typed API for encoding an Aqueduct pos-neg range sequence be included within an Aqueduct frame.
///
/// Automatically:
///
/// - Filters filters out empty ranges.
/// - Merges adjacent pos/neg ranges.
/// - Inserts an initial empty ack range if necessary.
///
/// As such, this can _mostly_ be used in arbitrary ways without panicking. However, if the user
/// attempts to pass `self` to a function to encode a frame, and `self` does not include _any_
/// non-empty ranges, that will panic.
#[derive(Default)]
pub struct PosNegRanges {
    written: Writer,
    pending: Option<(bool, u64)>,
}

impl PosNegRanges {
    /// Write a positive delta.
    pub fn pos_delta(&mut self, delta: u64) {
        if delta == 0 { return; }
        match &mut self.pending {
            &mut None => {
                self.pending = Some((true, delta));
            }
            &mut Some((true, ref mut pending_delta)) => {
                *pending_delta += delta;
            }
            &mut Some((false, pending_delta)) => {
                debug_assert!(!self.written.is_empty());
                self.written.write_vli(pending_delta);
                self.pending = Some((true, delta));
            }
        }
    }

    /// Write a negative delta.
    pub fn neg_delta(&mut self, delta: u64) {
        if delta == 0 { return; }
        match &mut self.pending {
            &mut None => {
                if self.written.is_empty() {
                    // initial empty ack
                    self.written.write_vli(0);
                }
                self.pending = Some((false, delta));
            }
            &mut Some((false, ref mut pending_delta)) => {
                debug_assert!(!self.written.is_empty());
                *pending_delta += delta;
            }
            &mut Some((true, pending_delta)) => {
                self.written.write_vli(pending_delta);
                self.pending = Some((true, delta));
            }
        }
    }

    fn finalize(mut self) -> MultiBytes {
        if let Some((_, pending_delta)) = self.pending {
            self.written.write_vli(pending_delta);
        }
        assert!(!self.written.is_empty(), "writing empty PosNegRanges (this is a bug)");
        self.written.into()
    }
}
