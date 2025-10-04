//! Handling X.509 certificates.

use crate::ed25519::{KeyPair, PublicKey};
use ed25519_dalek::pkcs8::EncodePrivateKey as _;
use x509_parser::prelude::FromDer as _;

/// Convert our crate's key pair type to rustls types.
///
/// Given input:
///
/// - ed25519 public and private key, represented as [`KeyPair`].
///
/// Create outputs:
///
/// - Self-signed X.509 certificate, represented as [`rustls::Certificate`], wherein:
///   
///   - The public and private keys are those input ed25519 keys.
///   - The sole subject alt name is the base64 encoding of the public key.
/// - DER-encoded PKCS#8 private key, containing the input ed25519 private key, represented as a
///   [`rustls::PrivateKey`].
pub fn key_pair_to_rustls_cert_key(
    key_pair: KeyPair,
) -> (
    rustls::pki_types::CertificateDer<'static>,
    rustls::pki_types::PrivatePkcs8KeyDer<'static>,
) {
    // simply wrap the private key bytes in dalek representation
    let dalek_raw_priv_key: ed25519_dalek::SigningKey =
        ed25519_dalek::SigningKey::from_bytes(&key_pair.private_key().to_bytes());

    // encapsulate the private key in the PKCS#8 format and encode that with DER encoding
    //
    // now, rather than just being the raw bytes of an ed25519 key, it's a generic "private key"
    // with metadata describing that fact that it happens to be of the ed25519 algorithm
    let dalek_pkcs_priv_key: ed25519_dalek::pkcs8::SecretDocument =
        dalek_raw_priv_key.to_pkcs8_der().unwrap();

    // convert that PKCS#8 DER encapsulation of the private key from
    // ed25519_dalek::pkcs::SecretDocument to rustls::pki_types::PrivatePkcs8KeyDer representation,
    // using the DER-encoded bytes as the common format they both understand
    let rustls_pkcs_priv_key: rustls::pki_types::PrivatePkcs8KeyDer =
        rustls::pki_types::PrivatePkcs8KeyDer::from(dalek_pkcs_priv_key.as_bytes().to_vec());

    // wrap the rustls::pki_types::PrivatePkcs8KeyDer in an rcgen::KeyPair, which is generic over
    // whether the DER format is PKCS#8 or something else like RSA or Sec1
    let rcgen_key_pair: rcgen::KeyPair = rcgen::KeyPair::try_from(&rustls_pkcs_priv_key).unwrap();

    // now, use rcgen to mint the self-signed X.509 certificate
    let subject_alt_names: Vec<String> = vec![key_pair.public_key().to_base64()];
    let rcgen_params: rcgen::CertificateParams =
        rcgen::CertificateParams::new(subject_alt_names).unwrap();
    let rcgen_cert: rcgen::Certificate = rcgen_params.self_signed(&rcgen_key_pair).unwrap();

    // convert that X.509 certificate from rcgen::Certificate to rustls::Certificate
    // representation, using the DER encoding of the X.509 certificate as the common format they
    // both understand
    let rustls_cert: rustls::pki_types::CertificateDer<'static> = rcgen_cert.der().clone();

    // done
    (rustls_cert, rustls_pkcs_priv_key)
}

/// Given a [`rustls::Certificate`], ensure that it contains an ed25519 public key, and extract
/// that as our [`PublicKey`] representation.
///
/// This is somewhat of a mirror to `key_pair_to_rustls_cert_key`.
pub(crate) fn rustls_cert_to_pub_key(
    rustls_cert: &rustls::pki_types::CertificateDer,
) -> Result<PublicKey, ()> {
    // convert that X.509 certificate from rustls::CertificateDer to
    // x509_parser::certificate::X509Certificate, which actually provides us meaningful information
    // about its internal structure, using the DER encoding of the X.509 certificate as the common
    // format they both understand
    let (_, x509_parser_cert) = x509_parser::certificate::X509Certificate::from_der(&*rustls_cert)
        .map_err(|e| warn!(%e, "rustls understood a cert that x509_parser did not"))?;
    let subject_pki = &x509_parser_cert.tbs_certificate.subject_pki;

    // validate that it's ed25519
    let sig_algo =
        x509_parser::signature_algorithm::SignatureAlgorithm::try_from(&subject_pki.algorithm)
            .map_err(|e| warn!(%e, "rustls allowed a sig algo that x509_parser did not"))?;
    if matches!(
        sig_algo,
        x509_parser::signature_algorithm::SignatureAlgorithm::ED25519
    ) && subject_pki.subject_public_key.data.len() == 32
        && subject_pki.subject_public_key.unused_bits == 0
    {
        // copy into our representation
        let mut buf = [0; 32];
        buf.copy_from_slice(subject_pki.subject_public_key.data.as_ref());
        Ok(PublicKey::from_bytes(buf))
    } else {
        warn!("rustls unexpectedly allowed non-ed25519 cert");
        Err(())
    }
}

/// Given a [`rustls::client::ServerName`], decode it as a base64-encoded public key.
///
/// This is somewhat of a mirror to `key_pair_to_rustls_cert_key`.
pub(crate) fn rustls_server_name_to_pub_key(
    rustls_server_name: &rustls::pki_types::ServerName,
) -> Result<PublicKey, ()> {
    match rustls_server_name {
        &rustls::pki_types::ServerName::DnsName(ref name) => PublicKey::from_base64(name.as_ref())
            .map_err(|e| warn!(%e, "rustls server name unexpectedly failed to decode")),
        _ => {
            warn!("rustls server name was unexpectedly not a dns name");
            Err(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foobar() {
        let key = KeyPair::generate();
        println!("{:#?}", key);
        let (cert, _) = key_pair_to_rustls_cert_key(key);
        assert_eq!(key.public_key(), rustls_cert_to_pub_key(&cert).unwrap(),);
    }
}
