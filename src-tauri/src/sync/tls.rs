use std::sync::Arc;

use sqlx::SqlitePool;
use tokio_rustls::rustls::client::danger::{
    HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier,
};
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use tokio_rustls::rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use tokio_rustls::rustls::{self, DigitallySignedStruct, DistinguishedName, SignatureScheme};
use tokio_rustls::{TlsAcceptor, TlsConnector};

use crate::error::AppError;
use crate::sync::identity::DeviceIdentity;

/// Install the `ring` crypto provider as the process-wide rustls default.
/// Must be called exactly once before any TLS config is built. A second
/// call is a no-op (we swallow the "already set" error).
pub fn install_crypto_provider() {
    // `install_default` returns Err if a provider is already installed;
    // that's fine — it means another caller beat us to it.
    let _ = rustls::crypto::ring::default_provider().install_default();
}

/// Loads every active peer cert from `trusted_devices` (sync_enabled = 1).
/// Each cert is parsed from PEM into DER form for rustls.
async fn load_trusted_certs(db: &SqlitePool) -> Result<Vec<CertificateDer<'static>>, AppError> {
    let rows = sqlx::query_file!("queries/sync/list_all_trusted_certs.sql")
        .fetch_all(db)
        .await
        .map_err(|e| AppError::Db(format!("load trusted certs: {e}")))?;

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let mut reader = r.cert_pem.as_bytes();
        for item in rustls_pemfile::certs(&mut reader) {
            let der = item.map_err(|e| {
                AppError::Internal(format!("parse trusted cert for {}: {e}", r.device_id))
            })?;
            out.push(der);
        }
    }
    Ok(out)
}

/// Parses this device's identity (cert + key) from PEM into DER form.
fn parse_identity(
    identity: &DeviceIdentity,
) -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>), AppError> {
    let mut cert_reader = identity.cert_pem.as_bytes();
    let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut cert_reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Internal(format!("parse own cert: {e}")))?;
    if certs.is_empty() {
        return Err(AppError::Internal(
            "own cert PEM contained no entries".into(),
        ));
    }

    let mut key_reader = identity.key_pem.as_bytes();
    let key = rustls_pemfile::private_key(&mut key_reader)
        .map_err(|e| AppError::Internal(format!("parse own key: {e}")))?
        .ok_or_else(|| AppError::Internal("own key PEM contained no entries".into()))?;

    Ok((certs, key))
}

/// Build a `TlsAcceptor` configured for mTLS: peers must present a
/// certificate whose DER matches an entry in `trusted_devices`.
pub async fn server_acceptor(
    db: &SqlitePool,
    identity: &DeviceIdentity,
) -> Result<TlsAcceptor, AppError> {
    let trusted = load_trusted_certs(db).await?;
    let (certs, key) = parse_identity(identity)?;

    let verifier = Arc::new(TrustedDeviceVerifier::new(trusted));
    let config = rustls::ServerConfig::builder()
        .with_client_cert_verifier(verifier)
        .with_single_cert(certs, key)
        .map_err(|e| AppError::Internal(format!("tls server config: {e}")))?;

    Ok(TlsAcceptor::from(Arc::new(config)))
}

/// Build a `TlsConnector` configured for mTLS: the remote server must
/// present a certificate whose DER matches an entry in `trusted_devices`.
pub async fn client_connector(
    db: &SqlitePool,
    identity: &DeviceIdentity,
) -> Result<TlsConnector, AppError> {
    let trusted = load_trusted_certs(db).await?;
    let (certs, key) = parse_identity(identity)?;

    let verifier = Arc::new(TrustedDeviceVerifier::new(trusted));
    let config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_client_auth_cert(certs, key)
        .map_err(|e| AppError::Internal(format!("tls client config: {e}")))?;

    Ok(TlsConnector::from(Arc::new(config)))
}

// ---------------------------------------------------------------------------
// Verifier: a presented cert is valid iff its DER bytes exactly match a
// `trusted_devices.cert_pem` entry. Self-signed peer certs make a normal
// PKI chain unworkable, and our pairing flow has already established
// out-of-band trust for these exact bytes.
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct TrustedDeviceVerifier {
    trusted: Vec<CertificateDer<'static>>,
}

impl TrustedDeviceVerifier {
    fn new(trusted: Vec<CertificateDer<'static>>) -> Self {
        Self { trusted }
    }

    fn is_trusted(&self, presented: &CertificateDer<'_>) -> bool {
        self.trusted
            .iter()
            .any(|t| t.as_ref() == presented.as_ref())
    }

    fn supported_schemes() -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::ED25519,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
        ]
    }
}

impl ServerCertVerifier for TrustedDeviceVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if self.is_trusted(end_entity) {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General(
                "peer cert not in trusted_devices".into(),
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        Self::supported_schemes()
    }
}

impl ClientCertVerifier for TrustedDeviceVerifier {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        // We don't advertise any CA subjects; peers always send their
        // self-signed leaf cert directly.
        &[]
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, rustls::Error> {
        if self.is_trusted(end_entity) {
            Ok(ClientCertVerified::assertion())
        } else {
            Err(rustls::Error::General(
                "peer cert not in trusted_devices".into(),
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        Self::supported_schemes()
    }
}
