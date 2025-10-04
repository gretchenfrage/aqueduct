//! Module for the actual network connection API.

use super::crypto_conversions::{rustls_cert_to_pub_key, rustls_server_name_to_pub_key};
use rustls::{
    CertificateError, DigitallySignedStruct, PeerIncompatible, SignatureScheme,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    crypto::{aws_lc_rs::default_provider, verify_tls13_signature},
    pki_types::{CertificateDer, ServerName, UnixTime},
};

#[derive(Debug)]
pub struct QkaiServerCertVerifier;

impl ServerCertVerifier for QkaiServerCertVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
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

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Err(rustls::Error::PeerIncompatible(
            PeerIncompatible::Tls12NotOffered,
        ))
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![SignatureScheme::ED25519]
    }
}
