
use crate::{
    channel::*,
    codec::{
        common::*,
        read,
        write,
    },
};
use std::sync::Arc;
use bytes::Bytes;
use dashmap::{
    mapref::entry::Entry,
    DashMap,
};
use tokio::sync::Mutex;
use anyhow::*;


// shared state for side of an aqueduct connection
struct Conn {
    side: Side,
    receivers: DashMap<ChanId, Arc<Mutex<Receiver>>>,
}

#[derive(Default)]
struct Receiver {

}

impl Conn {
    // handle an incoming QUIC unidirectional stream or datagram
    async fn handle(self: &Arc<Self>, mut rframes: read::Frames) -> Result<()> {
        while let Some(r) = rframes.frame().await? {
            match r {
                read::Frame::Version(r) => {
                    rframes = r.validate().await?;
                }
                read::Frame::ConnectionControl(_) => {
                    bail!("ConnectionControl frame past handshake");
                }
                read::Frame::ChannelControl(_) => {
                    bail!("ChannelControl frame not on bidi stream");
                }
                read::Frame::Message(r) => {
                    rframes = self.handle_message(r).await?;
                }
                _ => todo!(),
            }
        }
        Ok(())
    }

    async fn handle_message(self: &Arc<Self>, r: read::Message) -> Result<read::Frames> {
        let (r, sent_on) = r.sent_on().await?;
        ensure!(sent_on.dir().side_to() == self.side, "message chan id wrong dir");

        // route
        let receiver =
            if sent_on.minted_by() == self.side {
                if let Some(receiver) = self.receivers.get(&sent_on) {
                    Arc::clone(&*receiver)
                } else {
                    // ignore frame
                    let (r, _) = r.message_num().await?;
                    let (mut r, _) = r.attachments_len().await?;
                    while r.next_attachment().await?.is_some() {}
                    let r = r.done();
                    let (r, _) = r.payload_len().await?;
                    let (r, _) = r.payload().await?;
                    return Ok(r);
                }
            } else {
                Arc::clone(&*self.receivers.entry(sent_on).or_default())
            };

        // process
        let mut receiver = receiver.lock().await;

        todo!()
    }

    // handle an incoming QUIC bidirectional stream
    async fn bidi_stream(
        self: &Arc<Self>,
        mut rframes: read::Frames,
        stream_send: quinn::SendStream,
    ) -> Result<()> {
        Ok(())
    }
}

// server-side Aqueduct connection handshake
async fn server_handshake(conn: &quinn::Connection) -> Result<()> {
    let (mut stream_send, stream_recv) = conn.accept_bi().await?;
    let mut rframes = read::Frames::from_bidi_stream(stream_recv);

    let r = rframes.frame().await?.unwrap();
    let read::Frame::Version(r) = r else { bail!("expected Version frame") };
    rframes = r.validate().await?;

    let r = rframes.frame().await?.unwrap();
    let read::Frame::ConnectionControl(r) = r else { bail!("expected ConnectionControl frame") };
    rframes = r.skip_headers().await?;

    let mut wframes = write::Frames::default();
    wframes.version();
    wframes.connection_control();
    wframes.send_stream(&mut stream_send).await?;
    stream_send.finish().unwrap();

    ensure!(rframes.frame().await?.is_none(), "expected end of stream");

    Ok(())
}


