
use crate::frame::{
    common::*,
    read,
    write,
};
use std::{
    sync::{
        atomic::{Ordering, AtomicBool},
        Mutex,
        Condvar,
        Arc,
    },
    collections::HashMap,
};
use multibytes::MultiBytes;
use tokio::sync::OnceCell;
use dashmap::{
    DashMap,
    mapref::entry::Entry,
};
use anyhow::{anyhow, Result};


pub fn connection(
    conn: quinn::Connection,
    side: Side,
    conn_headers: Vec<(MultiBytes, MultiBytes)>,
) {
    let state = Arc::new(State {
        conn,
        side,
        local_acked_version: false.into(),
        remote_acked_version: false.into(),
        conn_headers: OnceCell::new(),
        conn_headers_mutex: Mutex::new(()),
        conn_headers_condvar: Condvar::new(),
        senders: DashMap::new(),
        receivers: DashMap::new(),
    });

    // handle datagrams
    state.spawn({
        let state = state.clone();
        async move {
            loop {
                let datagram = state.conn.read_datagram().await?;
                let frames = read::Frames::from_datagram(datagram);
                state.spawn(state.clone().handle_frames_ignore_reset(frames));
            }
        }
    });

    // handle streams
    state.spawn({
        let state = state.clone();
        async move {
            loop {
                let stream = state.conn.accept_uni().await?;
                let frames = read::Frames::from_stream(stream);
                state.spawn(state.clone().handle_frames_ignore_reset(frames));
            }
        }
    });

    // send connection headers
    state.spawn({
        let state = state.clone();
        async move {
            let mut write_frames = write::Frames::default();
            write_frames.version();
            let mut write_headers = write::Headers::default();
            for (key, val) in conn_headers {
                write_headers.header(key, val);
            }
            write_frames.connection_headers(write_headers);
            write_frames.send_on_new_stream(&state.conn).await
        }
    });
}

struct State {
    conn: quinn::Connection,
    side: Side,
    local_acked_version: AtomicBool,
    remote_acked_version: AtomicBool,
    conn_headers: OnceCell<Vec<(MultiBytes, MultiBytes)>>,
    conn_headers_mutex: Mutex<()>,
    conn_headers_condvar: Condvar,
    senders: DashMap<ChanId, Mutex<SenderState>>,
    receivers: DashMap<ChanId, Mutex<ReceiverState>>,
}

struct SenderState {
    net: Option<SenderNetState>,
}

struct SenderNetState {
    un_acked_attachments: HashMap<MessageNum, Vec<ChanId>>,
    acked_attachments: Option<Vec<ChanId>>,
}

struct ReceiverState {
    net: Option<ReceiverNetState>,
}

struct ReceiverNetState {
    ack_nack_unreliable_task: bool,
    ack_nacked_unreliable_up_to: u64,
    sent_unreliable: u64,
}

impl State {
    fn spawn(self: &Arc<Self>, task: impl Future<Output=Result<()>> + Send + 'static) {
        let state = Arc::clone(self);
        tokio::spawn(async move {
            if let Err(e) = task.await {
                error!(%e, "closing connection due to critical error");
                state.conn.close(0u8.into(), b"");
            }
        });
    }

    async fn handle_frames_ignore_reset(self: Arc<Self>, frames: read::Frames) -> Result<()> {
        self.handle_frames(frames).await
            .or_else(|e| match e {
                read::Error::Reset => Ok(()),
                read::Error::Other(e) => Err(e),
            })
    }

    async fn handle_frames(self: &Arc<Self>, mut frames: read::Frames) -> read::Result<()> {
        let mut version = false;
        let mut route_to = None;

        while let Some(frame) = frames.frame().await? {
            // version negotiation logic
            if !matches!(&frame, &read::Frame::Version(_))
                && !version
                && !self.local_acked_version.load(Ordering::Relaxed)
            {
                return read::Result::Err(anyhow!("expected VERSION frame").into());
            }

            // wait to process frames until connection headers received
            if !matches!(&frame, &read::Frame::Version(_) | &read::Frame::ConnectionHeaders(_))
                && !self.conn_headers.initialized()
            {
                drop(self.conn_headers_condvar.wait_while(
                    self.conn_headers_mutex.lock().unwrap(),
                    |_| !self.conn_headers.initialized(),
                ).unwrap());
            }

            // fully read and process the frame
            frames = self.handle_frame(frame, &mut version, &mut route_to).await?;
        }
        read::Result::Ok(())
    }

    async fn handle_frame(
        self: &Arc<Self>,
        frame: read::Frame,
        version: &mut bool,
        route_to: &mut Option<ChanId>,
    ) -> read::Result<read::Frames> {
        read::Result::Ok(match frame {
            read::Frame::Version(frames) => {
                *version = true;
                frames
            }
            read::Frame::AckVersion(frames) => {
                self.remote_acked_version.store(true, Ordering::Relaxed);
                frames
            }
            read::Frame::ConnectionHeaders(read) => {
                self.handle_connection_headers_frame(read).await?
            }
            read::Frame::RouteTo(read::RouteTo { chan_id, next }) => {
                if self.handle_route_to_frame_chan_id(chan_id).await? {
                    return todo!();
                }
                *route_to = Some(chan_id);
                next
            }
            read::Frame::Message(read_message) => self.handle_message_frame(read_message).await?,
            read::Frame::SentUnreliable(read::SentUnreliable { count, next }) => {
                self.handle_sent_unreliable_frame(*route_to, count).await?;
                next
            }
            read::Frame::FinishSender(_) => {
                todo!()
            }
            read::Frame::CancelSender(_) => {
                todo!()
            }
            read::Frame::AckReliable(_) => {
                todo!()
            }
            read::Frame::AckNackUnreliable(_) => {
                todo!()
            }
            read::Frame::CloseReceiver(_) => {
                todo!()
            }
            read::Frame::ForgetChannel(_) => {
                todo!()
            }
        })
    }

    async fn handle_connection_headers_frame(
        self: &Arc<Self>,
        mut read_headers: read::ConnectionHeaders,
    ) -> read::Result<read::Frames> {
        if read_headers.remaining_bytes() > 64_000 {
            return read::Result::Err(anyhow!("CONNECTION_HEADERS too large").into());
        }
        let mut headers = Vec::new();
        while read_headers.remaining_bytes() > 0 {
            headers.push(read_headers.header().await?);
        }
        let guard = self.conn_headers_mutex.lock().unwrap();
        if self.conn_headers.initialized() {
            return read::Result::Err(anyhow!("duplicate CONNECTION_HEADERS").into());
        }
        self.conn_headers.set(headers).unwrap();
        self.conn_headers_condvar.notify_all();
        drop(guard);
        Ok(read_headers.done())
    }

    async fn handle_route_to_frame_chan_id(
        self: &Arc<Self>,
        chan_id: ChanId,
    ) -> read::Result<bool> {
        if chan_id.sender() == self.side {
            return read::Result::Err(anyhow!("ROUTE_TO wrong direction").into());
        }

        if self.senders.get(&chan_id).is_some() {
            return read::Result::Ok(false);
        }

        if chan_id.creator() == self.side {
            let mut write_frames =
                write::Frames::new_with_version(&self.remote_acked_version);
            write_frames.forget_channel(chan_id);
            write_frames.send_on_new_stream(&self.conn).await?;
            read::Result::Ok(true)
        } else {
            if !self.create_default_sender(chan_id) {
                return read::Result::Ok(false); 
            }

            self.spawn({
                let this = self.clone();
                async move {
                    let mut write_frames =
                        write::Frames::new_with_version(&this.remote_acked_version);
                    write_frames.route_to(chan_id);
                    write_frames.send_on_new_stream(&this.conn).await?;
                    Ok(())
                }
            });
            read::Result::Ok(false)
        }
    }

    async fn handle_message_frame(
        self: &Arc<Self>,
        read_message: read::Message,
    ) -> read::Result<read::Frames> {
        let read::Message {
            message_num,
            message_headers: mut read_message_headers,
        } = read_message;

        // message headers
        if read_message_headers.remaining_bytes() > 64_000 {
            return read::Result::Err(anyhow!("MESSAGE headers too large").into());
        }
        let mut message_headers = Vec::new();
        while read_message_headers.remaining_bytes() > 0 {
            message_headers.push(read_message_headers.header().await?);
        }

        // message attached channels
        let mut message_attachments = read_message_headers.done().await?;
        while message_attachments.remaining_bytes() > 0 {
            let read::MessageAttachment {
                channel,
                channel_headers: mut read_channel_headers,
            } = message_attachments.attachment().await?;
            
            if channel.creator() == self.side {
                return read::Result::Err(anyhow!("attachment wrong creator").into());
            }

            if channel.sender() == self.side {
                self.create_default_sender(channel);
            } else {
                self.create_default_receiver(channel);
            }

            // message attached channel headers
            if read_channel_headers.remaining_bytes() > 64_000 {
                return read::Result::Err(anyhow!("channel headers too large").into());
            }
            let mut channel_headers = Vec::new();
            while read_channel_headers.remaining_bytes() > 0 {
                channel_headers.push(read_channel_headers.header().await?);
            }

            message_attachments = read_channel_headers.done();
        }

        // message payload
        let mut read_payload = message_attachments.done().await?;
        if read_payload.bytes() > 16_000_000 {
            return read::Result::Err(anyhow!("message too large").into());
        }
        let (payload, next) = read_payload.read().await?;

        Ok(next)
    }

    async fn handle_sent_unreliable_frame(
        self: &Arc<Self>,
        route_to: Option<ChanId>,
        count: u64,
    ) -> read::Result<()> {
        let route_to = route_to.ok_or_else(|| anyhow!("unrouted SENT_UNRELIABLE"))?;
        todo!()
    }

    fn create_default_sender(&self, chan_id: ChanId) -> bool {
        debug_assert!(chan_id.sender() == self.side);
        if let Entry::Vacant(entry) = self.senders.entry(chan_id) {
            entry.insert(Mutex::new(SenderState {
                net: Some(SenderNetState {
                    un_acked_attachments: HashMap::new(),
                    acked_attachments:
                        if chan_id == ChanId::ENTRYPOINT || chan_id.creator() != self.side {
                            None
                        } else {
                            Some(Vec::new())
                        },
                }),
            }));
            true
        } else {
            false
        }
    }

    fn create_default_receiver(&self, chan_id: ChanId) {
        debug_assert!(chan_id.sender() != self.side);
        if let Entry::Vacant(entry) = self.receivers.entry(chan_id) {
            entry.insert(Mutex::new(ReceiverState {
                net: Some(/*ReceiverNetState {

                }*/ todo!()),
            }));
        }
    }
}
