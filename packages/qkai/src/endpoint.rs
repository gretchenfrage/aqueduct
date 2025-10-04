//! Module for the actual network connection API.

use crate::{
    cert::{rustls_cert_to_pub_key, rustls_server_name_to_pub_key},
    error::Error,
    url::QkaiUrl,
};
use rustls::{
    Certificate, CertificateError, SignatureScheme,
    client::{ServerCertVerified, ServerCertVerifier, ServerName},
};
use std::{str, time::SystemTime};

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

pub struct QkaiServerCertVerifier;

impl ServerCertVerifier for QkaiServerCertVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &Certificate,
        intermediates: &[Certificate],
        server_name: &ServerName,
        _scts: &mut dyn Iterator<Item = &[u8]>,
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
