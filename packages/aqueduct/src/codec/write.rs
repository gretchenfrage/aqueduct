// frame encoding

use crate::{
    zero_copy::{
        MultiBytes,
        MultiBytesWriter,
    },
    codec::common::*,
};
use quinn::{SendStream, Connection, SendDatagramError};
use anyhow::*;


// utility internal to frame writing module:
//
// - wraps around MultiBytes
// - adds helper methods for writing common primitives
// - adds helper methods for zero-copy writing to QUIC
#[derive(Default, Clone)]
struct Writer(MultiBytesWriter);

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

    // send written data on the provided QUIC stream (zero-copy-ly).
    async fn send_stream(mut self, stream: &mut SendStream) -> Result<()> {
        let mut bytes = self.0.build();
        let mut pages = bytes.pages_mut();
        while !pages.is_empty() {
            let written = stream.write_chunks(pages).await?;
            pages = &mut pages[written.chunks..];
        }
        Ok(())
    }

    // open new unidirectional QUIC stream, send written data on it, finish stream.
    async fn send_new_stream(mut self, conn: &Connection) -> Result<()> {
        let mut stream = conn.open_uni().await?;
        self.send_stream(&mut stream).await?;
        stream.finish().unwrap();
        Ok(())
    }

    // send written data in a QUIC datagram, or fall back to send_new_stream if too large.
    async fn send_datagram(mut self, conn: &Connection) -> Result<()> {
        let max_datagram_size = conn.max_datagram_size()
            .ok_or_else(|| anyhow!("datagrams disabled"))?;
        if self.0.len() > max_datagram_size {
            self.send_new_stream(conn).await
        } else {
            let bytes = self.0.build().defragment();
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
        self.0.build()
    }
}


// typed API for writing a sequence of frames.
#[derive(Default)]
pub(crate) struct Frames(Writer);

impl Frames {
    // send written data on QUIC stream.
    pub(crate) async fn send_stream(mut self, stream: &mut SendStream) -> Result<()> {
        self.0.send_stream(stream).await
    }

    // open new unidirectional QUIC stream, send written data on it, finish stream.
    pub(crate) async fn send_new_stream(mut self, conn: &Connection) -> Result<()> {
        self.0.send_new_stream(conn).await
    }

    // send written data in a QUIC datagram, or fall back to send_new_stream if too large.
    pub(crate) async fn send_datagram(mut self, conn: &Connection) -> Result<()> {
        self.0.send_datagram(conn).await
    }

    // write a Version frame.
    pub(crate) fn version(&mut self) {
        self.0.write(&[FrameType::Version as u8]);
        self.0.write(&VERSION_FRAME_MAGIC_BYTES);
        self.0.write(&VERSION_FRAME_HUMAN_TEXT);
        self.0.write_vlba(VERSION);
    }

    // write a ConnectionControl frame.
    pub(crate) fn connection_control(&mut self) {
        // TODO
        self.0.write(&[FrameType::ConnectionControl as u8]);
        self.0.write(&[0]);
    }

    // write a ChannelControl frame.
    pub(crate) fn channel_control(&mut self, chan_id: ChanId) {
        self.0.write(&[FrameType::ChannelControl as u8]);
        self.0.write_chan_id(chan_id);
    }

    // write a Message frame.
    pub(crate) fn message(
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
}

// typed API for writing a sequence of message attachments.
#[derive(Default)]
pub(crate) struct Attachments(Writer);

impl Attachments {
    // write an attachment
    pub(crate) fn attachment(&mut self, chan_id: ChanId) {
        self.0.write_chan_id(chan_id);
    }
}
