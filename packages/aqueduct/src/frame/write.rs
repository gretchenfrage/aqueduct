//! Typed API for writing frames to quic streams and datagrams.

use crate::frame::common::*;
use anyhow::*;
use multibytes::*;
use quinn::{Connection, SendDatagramError, SendStream};
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};

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

    // write a var len int.
    fn write_varint(&mut self, mut i: u64) {
        for _ in 0..8 {
            let mut b = (i as u8 & VARINT_MASK) as u8;
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

    // write a variable length-prefixed byte array (zero-copy-ly).
    fn write_varbytes<B: Into<MultiBytes>>(&mut self, bytes: B) {
        let bytes = bytes.into();
        self.write_varint(bytes.len() as u64);
        self.write_zc(bytes);
    }

    // send data written to `self` on the provided QUIC stream (zero-copy-ly).
    async fn send_on_stream(self, stream: &mut SendStream) -> Result<()> {
        for fragment in self.0.fragments() {
            stream.write_chunk(fragment).await?;
        }
        Ok(())
    }

    // open new unidirectional QUIC stream, send written data on it, finish stream.
    async fn send_on_new_stream(self, conn: &Connection) -> Result<()> {
        let mut stream = conn.open_uni().await?;
        self.send_on_stream(&mut stream).await?;
        stream.finish().unwrap();
        Ok(())
    }

    // send written data in a QUIC datagram, or fall back to send_new_stream if too large.
    async fn send_on_datagram(self, conn: &Connection) -> Result<()> {
        let max_datagram_size = conn
            .max_datagram_size()
            .ok_or_else(|| anyhow!("datagrams disabled"))?;
        if self.0.len() > max_datagram_size {
            self.send_on_new_stream(conn).await
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

/// Typed API for writing a sequence of Aqueduct frames to a QUIC stream or datagram.
#[derive(Default)]
pub struct Frames(Writer);

macro_rules! tag_only_frame {
    ($method:ident $variant:ident) => {
        pub fn $method(&mut self) {
            self.0.write(&[FrameTag::$variant as u8]);
        }
    };
}

impl Frames {
    /// Construct, encoding a Version frame if must_send_version is true, but otherwise empty.
    pub fn new_with_version(remote_acked_version: &AtomicBool) -> Self {
        let mut frames = Frames::default();
        if !remote_acked_version.load(Relaxed) {
            frames.version();
        }
        frames
    }

    /// Send bytes written to `self` on an existing QUIC stream.
    pub async fn send_on_stream(self, stream: &mut SendStream) -> Result<()> {
        self.0.send_on_stream(stream).await
    }

    /// Open a new unidirectional QUIC stream, send bytes written to `self` on it, then finish the
    /// stream.
    pub async fn send_on_new_stream(self, conn: &Connection) -> Result<()> {
        self.0.send_on_new_stream(conn).await
    }

    /// Send bytes written to `self` in a QUIC datagram, or fall back to `send_on_new_stream` if
    /// too large.
    pub async fn send_on_datagram(self, conn: &Connection) -> Result<()> {
        self.0.send_on_datagram(conn).await
    }

    pub fn version(&mut self) {
        self.0.write(&[FrameTag::Version as u8]);
        self.0.write(&VERSION_FRAME_MAGIC_BYTES);
        self.0.write(&VERSION_FRAME_HUMAN_TEXT);
        self.0.write_varbytes(VERSION);
    }

    tag_only_frame!(ack_version AckVersion);

    pub fn connection_headers(&mut self, connection_headers: Headers) {
        self.0.write(&[FrameTag::ConnectionHeaders as u8]);
        self.0.write_varbytes(connection_headers.0.0);
    }

    pub fn route_to(&mut self, chan_id: ChanId) {
        self.0.write(&[FrameTag::RouteTo as u8]);
        self.0.write_varint(chan_id.0);
    }

    pub fn message(
        &mut self,
        message_num: u64,
        message_headers: Headers,
        attachments: Attachments,
        payload: MultiBytes,
    ) {
        self.0.write(&[FrameTag::Message as u8]);
        self.0.write_varint(message_num);
        self.0.write_varbytes(message_headers.0.0);
        self.0.write_varbytes(attachments.0.0);
        self.0.write_varbytes(payload);
    }

    pub fn sent_unreliable(&mut self, count: u64) {
        self.0.write(&[FrameTag::SentUnreliable as u8]);
        self.0.write_varint(count);
    }

    pub fn finish_sender(&mut self, sent_reliable: u64) {
        self.0.write(&[FrameTag::FinishSender as u8]);
        self.0.write_varint(sent_reliable);
    }

    tag_only_frame!(cancel_sender CancelSender);
    tag_only_frame!(ack_reliable AckReliable);

    pub fn ack_nack_unreliable(&mut self, deltas: Deltas) {
        self.0.write(&[FrameTag::AckNackUnreliable as u8]);
        self.0.write_varbytes(deltas.0.0);
    }

    tag_only_frame!(close_receiver CloseReceiver);

    pub fn forget_channel(&mut self, chan_id: ChanId) {
        self.0.write(&[FrameTag::ForgetChannel as u8]);
        self.0.write_varint(chan_id.0);
    }
}

#[derive(Default)]
pub struct Headers(Writer);

impl Headers {
    pub fn header(&mut self, key: impl Into<MultiBytes>, val: impl Into<MultiBytes>) {
        self.0.write_varbytes(key);
        self.0.write_varbytes(val);
    }
}

#[derive(Default)]
pub struct Attachments(Writer);

impl Attachments {
    pub fn attachment(&mut self, chan_id: ChanId, chan_headers: Headers) {
        self.0.write_varint(chan_id.0);
        self.0.write_varbytes(chan_headers.0.0);
    }
}

#[derive(Default)]
pub struct Deltas(Writer);

impl Deltas {
    pub fn delta(&mut self, n: u64) {
        self.0.write_varint(n);
    }
}
