//! Module for the actual network connection API.

use crate::{
    ed25519::{PublicKey, KeyPair},
    error::Error,
    cert::{
        key_pair_to_rustls_cert_key,
        rustls_cert_to_pub_key,
        rustls_server_name_to_pub_key,
    },
    url::QkaiUrl,
};
use std::{
    sync::Arc,
    net::{
        ToSocketAddrs,
        SocketAddr,
        Ipv4Addr,
        Ipv6Addr,
    },
    time::{SystemTime, Duration},
    str,
};
use dashmap::{
    DashMap,
    mapref::entry::Entry,
};
use tokio::{
    sync::mpsc::{Receiver, channel},
    spawn,
};
use rustls::{
    version::TLS13,
    SignatureScheme,
    Certificate,
    CertificateError,
    client::{
        ServerCertVerifier,
        ServerCertVerified,
        ServerName,
    },
};
use quinn::{
    Endpoint,
    Connection,
    SendStream,
    RecvStream,
    TransportConfig,
    congestion::BbrConfig,
};

#[derive(Clone)]
pub struct Client {
    ipv4_endpoint: Option<Endpoint>,
    ipv6_endpoint: Option<Endpoint>,
    connections: Arc<DashMap<QkaiUrl, Connection>>,
}

pub struct Server {
    endpoint: Endpoint,
    recv_new_stream: Receiver<(SendStream, RecvStream)>,
    key_pair: KeyPair,
}

/*pub struct SendStream {

}

pub struct RecvStream {

}*/

/*fn client_endpoint(addr: SocketAddr) -> Result<Endpoint> {

}*/

impl Client {
    pub fn new() -> Result<Self, Error> {
        //let bind_to = bind_to.to_socket_addrs().map_err(|_| Error::ToSocketAddrFail)?
        //    .next().ok_or(Error::ToSocketAddrFail)?;
        let rustls_client_config = rustls::client::ClientConfig::builder()
            .with_safe_default_cipher_suites()
            .with_safe_default_kx_groups()
            .with_protocol_versions(&[&TLS13]).unwrap()
            .with_custom_certificate_verifier(Arc::new(QkaiServerCertVerifier) as _)
            .with_no_client_auth();
        let mut transport_config = TransportConfig::default();
        transport_config.max_concurrent_uni_streams(0u32.into());
        transport_config.max_idle_timeout(Some(Duration::from_secs(300).try_into().unwrap()));
        transport_config.keep_alive_interval(None);
        transport_config.congestion_controller_factory(Arc::new(BbrConfig::default()));
        let mut quinn_client_config =
            quinn::ClientConfig::new(Arc::new(rustls_client_config) as _);
        quinn_client_config.transport_config(Arc::new(transport_config));
        let ipv4_endpoint = Endpoint::client((Ipv4Addr::UNSPECIFIED, 0).into())
            .map(|mut endpoint| {
                endpoint.set_default_client_config(quinn_client_config.clone());
                endpoint
            })
            .map_err(|e| warn!(%e, "error constructing ipv4 client endpoint")).ok();
        let ipv6_endpoint = Endpoint::client((Ipv6Addr::UNSPECIFIED, 0).into())
            .map(|mut endpoint| {
                endpoint.set_default_client_config(quinn_client_config);
                endpoint
            })
            .map_err(|e| warn!(%e, "error constructing ipv4 client endpoint")).ok();
        if ipv4_endpoint.is_none() && ipv6_endpoint.is_none() {
            return Err(Error::CouldNotBindClient);
        }
        Ok(Self {
            ipv4_endpoint,
            ipv6_endpoint,
            connections: Default::default(),
        })
    }

    pub async fn connect<U: ToQkaiUrl>(&self, url: U) -> Result<(SendStream, RecvStream), Error> {
        let url = url.to_url()?;
        let conn = match self.connections.entry(url) {
            Entry::Occupied(entry) => entry.get().clone(),
            Entry::Vacant(entry) => {

                let pub_key_b64 = url.public_key.to_base64_bytes();
                let endpoint = if url.addr.is_ipv4() {
                    self.ipv4_endpoint.as_ref().or(self.ipv6_endpoint.as_ref()).unwrap()
                } else {
                    self.ipv6_endpoint.as_ref().or(self.ipv4_endpoint.as_ref()).unwrap()
                };
                let conn = endpoint
                    .connect(url.addr, str::from_utf8(&pub_key_b64).unwrap())?.await?;
                entry.insert(conn.clone());

                let connections_2 = Arc::downgrade(&self.connections);
                let conn_2 = conn.clone();
                spawn(async move {
                    conn_2.closed().await;
                    if let Some(connections) = connections_2.upgrade() {
                        println!("closing it by the way");
                        connections.remove(&url);
                    }
                });                

                conn
            }
        };
        Ok(conn.open_bi().await?)
    }
}

impl Server {
    pub fn new<A: ToSocketAddrs>(bind_to: A, key_pair: KeyPair) -> Result<Self, Error> {
        let bind_to = bind_to.to_socket_addrs().map_err(|_| Error::ToSocketAddrFail)?
            .next().ok_or(Error::ToSocketAddrFail)?;
        let (rustls_cert, rustls_key) = key_pair_to_rustls_cert_key(key_pair);
        let mut rustls_server_config = rustls::server::ServerConfig::builder()
            .with_safe_default_cipher_suites()
            .with_safe_default_kx_groups()
            .with_protocol_versions(&[&TLS13]).unwrap()
            .with_no_client_auth()
            .with_single_cert(vec![rustls_cert], rustls_key).unwrap();
        rustls_server_config.max_early_data_size = u32::MAX;
        let mut transport_config = TransportConfig::default();
        transport_config.max_concurrent_uni_streams(0u32.into());
        transport_config.max_idle_timeout(Some(Duration::from_secs(300).try_into().unwrap()));
        transport_config.keep_alive_interval(None);
        transport_config.congestion_controller_factory(Arc::new(BbrConfig::default()));
        let mut quinn_server_config =
            quinn::ServerConfig::with_crypto(Arc::new(rustls_server_config) as _);
        quinn_server_config.transport_config(Arc::new(transport_config));
        let endpoint = Endpoint::server(quinn_server_config, bind_to)?;
        //let mut endpoint = Endpoint::client()
        //client.endpoint.set_server_config(Some(quinn_server_config));
        let (send_new_stream, recv_new_stream) = channel(1 << 16);
        let endpoint_2 = endpoint.clone();
        spawn(async move {
            while let Some(conn) = endpoint_2.accept().await {
                trace!("accepted connection, creating task to accept streams on it");
                let send_new_stream = send_new_stream.clone();
                spawn(async move {
                    let conn = match conn.await {
                        Ok(conn) => conn,
                        Err(e) => {
                            trace!(%e, "accept connection failed");
                            return;
                        }
                    };

                    while let Ok(new_stream) = conn.accept_bi()/* woah he's bisexual */.await
                        .map_err(|e| trace!(%e, "accept stream failed"))
                    {
                        if send_new_stream.send(new_stream).await.is_err() {
                            trace!("stoppign accepting streams because received dropped");
                            return;
                        }
                    }
                });
            }
            trace!("stopping accepting connections because endpoint returned None");
        });
        Ok(Self {
            endpoint,
            recv_new_stream,
            key_pair,
        })
    }

    pub fn key_pair(&self) -> KeyPair {
        self.key_pair
    }

    pub fn public_key(&self) -> PublicKey {
        self.key_pair.public_key()
    }

    pub fn local_addr(&self) -> Result<SocketAddr, Error> {
        Ok(self.endpoint.local_addr()?)
    }

    pub fn localhost_url(&self) -> Result<QkaiUrl, Error> {
        let mut addr = self.local_addr()?;
        match &mut addr {
            &mut SocketAddr::V4(ref mut addr) => addr.set_ip(Ipv4Addr::LOCALHOST),
            &mut SocketAddr::V6(ref mut addr) => addr.set_ip(Ipv6Addr::LOCALHOST),
        };
        Ok(QkaiUrl { public_key: self.public_key(), addr })
    }

    pub async fn accept(&mut self) -> Result<(SendStream, RecvStream), Error> {
        self.recv_new_stream.recv().await.ok_or(Error::NewStreamChannelDropped)
    }
    /*
    pub async fn ping_self<A: ToSocketAddrs>(&self, reach_at: A) -> Result<QkaiUrl, Error> {
        let reach_at = reach_at.to_socket_addrs().map_err(|_| Error::ToSocketAddrFail)?
            .next().ok_or(Error::ToSocketAddrFail)?;
        let url = QkaiUrl { public_key: self.public_key(), addr: reach_at };
        self.client.connect(url).await?;
        Ok(url)
    }
    */
}

/// Type that be try to be converted to [`QkaiUrl`].
pub trait ToQkaiUrl {
    fn to_url(self) -> Result<QkaiUrl, Error>;
}

impl ToQkaiUrl for QkaiUrl {
    fn to_url(self) -> Result<QkaiUrl, Error> {
        Ok(self)
    }
}

impl<'a> ToQkaiUrl for &'a str {
    fn to_url(self) -> Result<QkaiUrl, Error> {
        QkaiUrl::parse(self)
    }
}

struct QkaiServerCertVerifier;

impl ServerCertVerifier for QkaiServerCertVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &Certificate,
        intermediates: &[Certificate],
        server_name: &ServerName,
        _scts: &mut dyn Iterator<Item=&[u8]>,
        _ocsp_response: &[u8],
        _now: SystemTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if !intermediates.is_empty() {
            trace!("rejecting server cert because has intermediates");
            return Err(CertificateError::ApplicationVerificationFailure.into());
        }
        let end_entity_pub_key = rustls_cert_to_pub_key(end_entity)
            .map_err(|()| CertificateError::ApplicationVerificationFailure)?;
        let server_name_pub_key = rustls_server_name_to_pub_key(server_name)
            .map_err(|()| CertificateError::ApplicationVerificationFailure)?;
        if end_entity_pub_key == server_name_pub_key {
            trace!("accepting server cert");
            Ok(ServerCertVerified::assertion())
        } else {
            trace!("rejecting server cert");
            Err(CertificateError::ApplicationVerificationFailure.into())
        }
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![SignatureScheme::ED25519]
    }
}


#[cfg(test)]
mod tests {
    use crate::*;
    use tokio::{*, io::*};

    #[tokio::test]
    async fn foobar() {
        // create localhost server, arbitrary port and random key pair
        let mut server = Server::new("[::1]:0", KeyPair::generate()).unwrap();
        let url = server.localhost_url().unwrap();
        println!("{}", url);
        
        // spawn tasks that make it work as an echo server
        spawn(async move {
            while let Ok((mut send, mut recv)) = server.accept().await {
                spawn(async move {
                    let mut buf = [0; 4096];
                    while let Ok(Some(n)) = recv.read(&mut buf).await {
                        if send.write_all(&buf[..n]).await.is_err() {
                            break;
                        }
                    }
                });
            }
        });

        // create client, do a little hello world
        let client = Client::new().unwrap();
        let (mut send, recv) = client.connect(url).await.unwrap();
        send.write_all(b"hello world\n").await.unwrap();
        let mut recv = BufReader::new(recv);
        let mut buf = String::new();
        recv.read_line(&mut buf).await.unwrap();
        assert_eq!(buf, "hello world\n");
    }
}
