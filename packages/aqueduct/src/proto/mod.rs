// protocol implementation

mod codec;

use self::codec::*;
use crate::{
    *,
    zero_copy::{*, quic::*},
};
use std::net::SocketAddr;
use dashmap::DashMap;
use anyhow::{Error, Context, bail};
use quinn::*;
use tokio::*;


/// Logic for encoding a message and attaching its attachments in some format.
pub trait EncoderAttacher<M>: Send + Sync + 'static {
    fn encode(&self, msg: M, attach: AttachTarget) -> Result<MultiBytes, Error>;
}

/// Logic for decoding a message and detaching its attachments in some format.
pub trait DecoderDetacher<M>: Send + Sync + 'static {
    fn decode(&self, encoded: MultiBytes, detach: DetachTarget) -> Result<M, Error>;
}

/// Passed to an [`EncoderAttacher`] to attach attachments to.
pub struct AttachTarget {

}

impl AttachTarget {
    pub fn attach_sender<M, D>(&mut self, _sender: IntoSender<M>, _decoder: D) -> u32
    where
        D: DecoderDetacher<M>,
    {
        todo!()
    }

    pub fn attach_receiver<M, E>(&mut self, _receiver: Receiver<M>, _encoder: E) -> u32
    where
        E: EncoderAttacher<M>,
    {
        todo!()
    }
    /*
    pub fn attach_oneshot_sender<M, D>(&mut self, sender: OneshotSender<M>, decoder: D) -> u32
    where
        D: DecoderDetacher<M>,
    {
        todo!()
    }

    pub fn attach_oneshot_receiver<M, E>(&mut self, receiver: OneshotReceiver<M>, encoder: E) -> u32
    where
        E: EncoderAttacher<M>,
    {
        todo!()
    }*/
}

/// Passed to an [`DecoderDettacher`] to detach attachments from.
pub struct DetachTarget {
}

impl DetachTarget {
    pub fn detach_sender<M, E: EncoderAttacher<M>>(
        &mut self,
        _attachment_idx: u32,
        _encoder: E,
    ) -> Option<IntoSender<M>> {
        todo!()
    }

    pub fn detach_receiver<M, D: DecoderDetacher<M>>(
        &mut self,
        _attachment_idx: u32,
        _decoder: D,
    ) -> Option<Receiver<M>> {
        todo!()
    }
    /*
    pub fn detach_oneshot_sender<M, E: EncoderAttacher<M>>(
        &mut self,
        attachment_idx: u32,
        encoder: E,
    ) -> Result<OneshotSender<M>, MissingAttachment> {
        todo!()
    }

    pub fn detach_oneshot_receiver<M, D: DecoderDetacher<M>>(
        &mut self,
        attachment_idx: u32,
        decoder: D,
    ) -> Result<OneshotReceiver<M>, MissingAttachment> {
        todo!()
    }
    
    pub fn remaining_attachments(&self) -> usize {
        self.remaining_attachments
    }*/
}




struct Conn {
    senders: DashMap<ChanId, NetSender>,
    receivers: DashMap<ChanId, NetReceiver>,
}

struct NetSender {
    closed: bool,
    remote_reachable: bool,
}

struct NetReceiver {
    closed: bool,
    remote_reachable: bool,
}

pub fn server<T, D>(bind_to: &str, decoder: D) -> Result<Receiver<T>, Error>
where
    D: DecoderDetacher<T>,
{
    let bind_to = bind_to.parse::<SocketAddr>()
        .context("invalid bind_to address")?;
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()])
        .expect("unexpected rcgen error");
    let cert_der = rustls::pki_types::CertificateDer::from(cert.cert);
    let priv_key = rustls::pki_types::PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der());
    let server_config = ServerConfig::with_single_cert(vec![cert_der.clone()], priv_key.into())
        .expect("unexpected quinn serverconfig creation error");
    let endpoint = Endpoint::server(server_config, bind_to)
        .context("failed to create quinn endpoint")?;

    let (send, recv) = channel();

    spawn(async move {
        loop {
            while let Some(incoming) = endpoint.accept().await {
                spawn(async move {
                    match incoming.await {
                        Ok(connection) => {
                            if let Err(e) = handle_connection(connection).await {
                                warn!(%e, "protocol error");
                            }
                        },
                        Err(e) => warn!(%e, "error accepting incoming"),
                    }
                });
            }
            todo!()
        }
    });

    Ok(recv.into_receiver())
}

async fn handle_connection(connection: Connection) -> Result<(), Error> {
    let (send, recv) = connection.accept_bi().await?;
    let mut frames = FramesReader::new(QuicReader::stream(recv));

    let r = frames.read_frame().await?;
    let FrameReader::Version(r) = r else { bail!("expected Version frame") };
    frames = r.read_validate().await?;

    let r = frames.read_frame().await?;
    let FrameReader::ConnectionControl(r) = r else { bail!("expected ConnectionControl frame") };
    let (r, _) = r.read_headers_len().await?;
    let (r, headers) = r.read_headers().await?;
    frames = r;


    //ensure!(recv.read_byte()? == VERSION_FRAME_MAGIC_BYTES[0], "expected version frame");
    //let 
    
    Ok(())
}
