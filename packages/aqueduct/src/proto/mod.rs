use crate::frame::{common::*, read, write};
use quinn;
use std::sync::Arc;

struct Connection {
    side: Side,
    quic_connection: quinn::Connection,
}

impl Connection {
    fn new(side: Side, quic_connection: quinn::Connection) -> Arc<Self> {
        let this = Arc::new(Connection {
            side,
            quic_connection,
        });

        // handle streams
        this.spawn({
            let this = this.clone();
            async move {
                loop {
                    let stream = this.quic_connection.accept_uni().await?;
                    let r = read::Frames::from_stream(stream);
                    this.spawn({
                        let this = this.clone();
                        async move { this.handle_frames(r).await }
                    });
                }
            }
        });

        // handle datagrams
        this.spawn({
            let this = this.clone();
            async move {
                loop {
                    let datagram = this.quic_connection.read_datagram().await?;
                    let r = read::Frames::from_datagram(datagram);
                    this.spawn({
                        let this = this.clone();
                        async move { this.handle_frames(r).await }
                    });
                }
            }
        });

        this
    }

    fn spawn(&self, f: impl Future<Output = read::Result<()>> + Send + 'static) {
        tokio::task::spawn(async move {
            match f.await {
                Ok(()) => (),
                Err(read::Error::Reset) => trace!("received stream reset"),
                Err(read::Error::Other(e)) => {
                    error!(%e, "unrecoverable error in connection task");
                    todo!()
                }
            }
        });
    }

    async fn handle_frames(&self, r: read::Frames) -> read::Result<()> {
        while let Some(r) = r.frame().await? {
            match r {
                read::Frame::Version(r) => todo!(),
                read::Frame::AckVersion(r) => todo!(),
                read::Frame::ConnectionHeaders(r) => todo!(),
                read::Frame::RouteTo(r) => return self.handle_routed_frames(r).await,
                read::Frame::Message(r) => todo!(),
                read::Frame::SentUnreliable(r) => todo!(),
                read::Frame::FinishSender(r) => todo!(),
                read::Frame::CancelSender(r) => todo!(),
                read::Frame::AckReliable(r) => todo!(),
                read::Frame::AckNackUnreliable(r) => todo!(),
                read::Frame::CloseReceiver(r) => todo!(),
                read::Frame::ForgetChannel(r) => todo!(),
            }
        }
        Ok(())
    }

    async fn handle_routed_frames(&self, r: read::RouteTo) -> read::Result<()> {
        let read::RouteTo { chan_id, mut next } = r;
        while let Some(r) = next.frame().await? {
            next = match r {
                read::Frame::Version(r) => todo!(),
                read::Frame::AckVersion(r) => todo!(),
                read::Frame::ConnectionHeaders(r) => todo!(),
                read::Frame::RouteTo(r) => todo!(),
                read::Frame::Message(r) => {
                    let read::Message {
                        message_num,
                        message_headers: mut r,
                    } = r;
                    while r.remaining_bytes() > 0 {
                        let (message_header_key, message_header_val) = r.header().await?;
                    }
                    let mut r = r.done().await?;
                    while r.remaining_bytes() > 0 {
                        r = {
                            let r = r.attachment().await?;
                            let read::MessageAttachment {
                                channel: attached_chan_id,
                                channel_headers: mut r,
                            } = r;
                            while r.remaining_bytes() > 0 {
                                let (attached_channel_header_key, attached_channel_header_val) =
                                    r.header().await?;
                            }
                            r.done()
                        };
                    }
                    let r = r.done().await?;
                    let (payload, r) = r.payload().await?;

                    r
                }
                read::Frame::SentUnreliable(r) => todo!(),
                read::Frame::FinishSender(r) => todo!(),
                read::Frame::CancelSender(r) => todo!(),
                read::Frame::AckReliable(r) => todo!(),
                read::Frame::AckNackUnreliable(r) => todo!(),
                read::Frame::CloseReceiver(r) => todo!(),
                read::Frame::ForgetChannel(r) => todo!(),
            };
        }
        Ok(())
    }
}
