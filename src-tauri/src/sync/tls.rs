use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use ring::signature::{ECDSA_P256_SHA256_ASN1, ED25519, ED25519_PUBLIC_KEY_LEN, UnparsedPublicKey};
use sqlx::SqlitePool;
use tokio_rustls::rustls::client::danger::{
    HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier,
};
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use tokio_rustls::rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use tokio_rustls::rustls::{
    self, DigitallySignedStruct, DistinguishedName, Error, SignatureScheme,
};
use tokio_rustls::{TlsAcceptor, TlsConnector};

use crate::error::AppError;
use crate::sync::cert_validation::{self, PublicKeyInfo};
use crate::sync::identity::DeviceIdentity;

/// Install the `ring` crypto provider as the process-wide rustls default.
/// Must be called exactly once before any TLS config is built. A second
/// call is a no-op (we swallow the "already set" error).
pub fn install_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

/// Per-connection knobs for the verifier. The most important field is
/// `expected_device_id`: when set, the verifier rejects any cert
/// whose SAN doesn't list it. This is how we bind a TLS connection
/// to the device_id mDNS told us to dial (Step 30.3).
#[derive(Debug, Clone, Default)]
pub struct VerifierPolicy {
    /// The device_id we expect to see in the cert's DNS SAN. `None`
    /// means "any trusted cert is fine" (the legacy behavior,
    /// preserved for non-mTLS local flows).
    pub expected_device_id: Option<String>,
}

/// Loads every device cert from the `devices` table. Each cert is parsed
/// from PEM into DER form for rustls.
async fn load_device_certs(db: &SqlitePool) -> Result<Vec<CertificateDer<'static>>, AppError> {
    let rows = sqlx::query_file!("queries/sync/list_device_certs.sql")
        .fetch_all(db)
        .await
        .map_err(|e| AppError::Db(format!("load device certs: {e}")))?;

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let mut reader = r.cert_pem.as_bytes();
        for item in rustls_pemfile::certs(&mut reader) {
            let der = item.map_err(|e| {
                AppError::Internal(format!("parse device cert for {}: {e}", r.device_id))
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
/// certificate whose DER matches an entry in `devices` AND whose
/// signature is verifiable against that cert's public key.
pub async fn server_acceptor(
    db: &SqlitePool,
    identity: &DeviceIdentity,
) -> Result<TlsAcceptor, AppError> {
    let trusted = load_device_certs(db).await?;
    let (certs, key) = parse_identity(identity)?;

    let verifier = Arc::new(TrustedDeviceVerifier::new(
        trusted,
        VerifierPolicy::default(),
    ));
    let config = rustls::ServerConfig::builder()
        .with_client_cert_verifier(verifier)
        .with_single_cert(certs, key)
        .map_err(|e| AppError::Internal(format!("tls server config: {e}")))?;

    Ok(TlsAcceptor::from(Arc::new(config)))
}

/// Build a `TlsConnector` that binds the cert to a specific
/// `expected_device_id` — used by `client::dial` after mDNS picks
/// the peer to call. The verifier rejects any cert whose SAN does
/// not list this device_id, preventing one peer from presenting
/// another peer's cert.
pub async fn client_connector_for_peer(
    db: &SqlitePool,
    identity: &DeviceIdentity,
    expected_device_id: String,
) -> Result<TlsConnector, AppError> {
    let trusted = load_device_certs(db).await?;
    let (certs, key) = parse_identity(identity)?;

    let verifier = Arc::new(TrustedDeviceVerifier::new(
        trusted,
        VerifierPolicy {
            expected_device_id: Some(expected_device_id),
        },
    ));
    let config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_client_auth_cert(certs, key)
        .map_err(|e| AppError::Internal(format!("tls client config: {e}")))?;

    Ok(TlsConnector::from(Arc::new(config)))
}

// ---------------------------------------------------------------------------
// Verifier: a presented cert is valid iff:
//   1. Its DER bytes match a `devices` cert exactly (Step 30.1 trust
//      anchor — the pairing flow has already established this out of
//      band).
//   2. Its `notBefore` / `notAfter` window contains "now" (Step 30.3).
//   3. Its signature scheme matches its public-key type, and the
//      signature on each TLS handshake message verifies against that
//      public key (Step 30.3).
//   4. If the verifier was built with `expected_device_id`, the
//      presented cert's SAN contains that device_id (Step 30.3 —
//      binds the outbound connection to the device mDNS selected).
//
// We deliberately do NOT build a PKI chain. Every cert is self-signed
// and the DER equality is the trust anchor. PKI doesn't fit our
// pairing-time-trust model.
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct TrustedDeviceVerifier {
    trusted: Vec<TrustedEntry>,
    policy: VerifierPolicy,
}

#[derive(Debug, Clone)]
struct TrustedEntry {
    der: CertificateDer<'static>,
    public_key: Option<PublicKeyInfo>,
    device_id: Option<String>,
    not_before: u64,
    not_after: u64,
}

/// Map a cert's `SignatureScheme` to the public-key algorithm it
/// implies. Returns `None` for schemes we don't support — in which
/// case the verifier rejects the cert at handshake-signature time.
fn scheme_to_alg(scheme: SignatureScheme) -> Option<PublicKeyInfo> {
    match scheme {
        SignatureScheme::ED25519 => {
            // The actual public key bytes are filled in by the
            // caller; we use this just to know "yes, an Ed25519 key
            // is fine". A dummy 32-byte vector is enough.
            Some(PublicKeyInfo::Ed25519(vec![0; ED25519_PUBLIC_KEY_LEN]))
        }
        SignatureScheme::ECDSA_NISTP256_SHA256 => {
            // Same as Ed25519: the real key is in `TrustedEntry`.
            Some(PublicKeyInfo::EcdsaP256(vec![0; 65]))
        }
        _ => None,
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl TrustedDeviceVerifier {
    fn new(trusted: Vec<CertificateDer<'static>>, policy: VerifierPolicy) -> Self {
        let trusted = trusted
            .into_iter()
            .filter_map(|der| {
                // Skip certs that we can't parse at all. The
                // `devices` table may contain legacy stubs with
                // empty cert_pem, or rows where Step 30.2's PEM
                // parse succeeded but the x509-parser can't make
                // sense of the bytes. Such certs are not
                // "load-bearing" trusted anchors; we filter them
                // out of the verifier so they don't disable the
                // valid peers.
                let public_key = cert_validation::extract_public_key_info(der.as_ref());
                let (not_before, not_after) = cert_validation::validity_window(der.as_ref())?;
                let device_id = extract_san_device_id(der.as_ref());
                Some(TrustedEntry {
                    der,
                    public_key,
                    device_id,
                    not_before,
                    not_after,
                })
            })
            .collect();
        Self { trusted, policy }
    }

    fn find_trusted(&self, presented: &CertificateDer<'_>) -> Option<&TrustedEntry> {
        self.trusted
            .iter()
            .find(|t| t.der.as_ref() == presented.as_ref())
    }

    fn check_validity(&self, entry: &TrustedEntry) -> Result<(), Error> {
        let now = unix_now();
        if now < entry.not_before {
            return Err(Error::General(format!(
                "cert not yet valid (not_before={})",
                entry.not_before
            )));
        }
        if now > entry.not_after {
            return Err(Error::General(format!(
                "cert expired (not_after={})",
                entry.not_after
            )));
        }
        Ok(())
    }

    fn check_device_id(&self, entry: &TrustedEntry) -> Result<(), Error> {
        let Some(expected) = &self.policy.expected_device_id else {
            return Ok(());
        };
        match &entry.device_id {
            Some(id) if id == expected => Ok(()),
            Some(id) => Err(Error::General(format!(
                "cert device_id {id:?} does not match expected {expected:?}"
            ))),
            None => Err(Error::General(
                "cert has no device_id in SAN — cannot bind to mDNS-selected peer".into(),
            )),
        }
    }

    fn verify_signature_with(
        &self,
        entry: &TrustedEntry,
        scheme: SignatureScheme,
        message: &[u8],
        signature: &[u8],
    ) -> Result<(), Error> {
        let Some(pk_info) = &entry.public_key else {
            return Err(Error::General(
                "trusted cert has no parseable public key".into(),
            ));
        };
        match (pk_info, scheme) {
            (PublicKeyInfo::Ed25519(key), SignatureScheme::ED25519) => {
                let key = UnparsedPublicKey::new(&ED25519, key.as_slice());
                key.verify(message, signature)
                    .map_err(|_| Error::General("Ed25519 handshake signature invalid".into()))
            }
            (PublicKeyInfo::EcdsaP256(point), SignatureScheme::ECDSA_NISTP256_SHA256) => {
                let key = UnparsedPublicKey::new(&ECDSA_P256_SHA256_ASN1, point.as_slice());
                key.verify(message, signature)
                    .map_err(|_| Error::General("ECDSA P-256 handshake signature invalid".into()))
            }
            _ => Err(Error::General(format!(
                "signature scheme {scheme:?} does not match cert key type"
            ))),
        }
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

/// Pull the first DNS SAN off a cert, if any. Returns `None` for
/// certs with no SAN extension.
fn extract_san_device_id(der: &[u8]) -> Option<String> {
    use x509_parser::prelude::*;
    let (_, cert) = X509Certificate::from_der(der).ok()?;
    let san = cert.subject_alternative_name().ok()??;
    for name in &san.value.general_names {
        if let x509_parser::extensions::GeneralName::DNSName(d) = name {
            return Some((*d).to_string());
        }
    }
    None
}

impl ServerCertVerifier for TrustedDeviceVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        let Some(entry) = self.find_trusted(end_entity) else {
            return Err(Error::General("peer cert not in devices".into()));
        };
        self.check_validity(entry)?;
        self.check_device_id(entry)?;
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        let entry = self
            .find_trusted(cert)
            .ok_or_else(|| Error::General("tls12 sig: peer cert not in devices".into()))?;
        if scheme_to_alg(dss.scheme).is_none() {
            return Err(Error::General(format!(
                "tls12 sig: scheme {:?} not supported",
                dss.scheme
            )));
        }
        self.verify_signature_with(entry, dss.scheme, message, dss.signature())?;
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        let entry = self
            .find_trusted(cert)
            .ok_or_else(|| Error::General("tls13 sig: peer cert not in devices".into()))?;
        if scheme_to_alg(dss.scheme).is_none() {
            return Err(Error::General(format!(
                "tls13 sig: scheme {:?} not supported",
                dss.scheme
            )));
        }
        self.verify_signature_with(entry, dss.scheme, message, dss.signature())?;
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        Self::supported_schemes()
    }
}

impl ClientCertVerifier for TrustedDeviceVerifier {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, Error> {
        let Some(entry) = self.find_trusted(end_entity) else {
            return Err(Error::General("peer cert not in devices".into()));
        };
        self.check_validity(entry)?;
        // For inbound connections, we don't know what device_id the
        // peer claims to be ahead of time — the cert itself is the
        // source of truth. Bind here is skipped; the dialer
        // extracts the device_id post-handshake and re-checks it
        // against the mDNS announcement.
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        let entry = self
            .find_trusted(cert)
            .ok_or_else(|| Error::General("tls12 sig: peer cert not in devices".into()))?;
        if scheme_to_alg(dss.scheme).is_none() {
            return Err(Error::General(format!(
                "tls12 sig: scheme {:?} not supported",
                dss.scheme
            )));
        }
        self.verify_signature_with(entry, dss.scheme, message, dss.signature())?;
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        let entry = self
            .find_trusted(cert)
            .ok_or_else(|| Error::General("tls13 sig: peer cert not in devices".into()))?;
        if scheme_to_alg(dss.scheme).is_none() {
            return Err(Error::General(format!(
                "tls13 sig: scheme {:?} not supported",
                dss.scheme
            )));
        }
        self.verify_signature_with(entry, dss.scheme, message, dss.signature())?;
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        Self::supported_schemes()
    }
}

// ---------------------------------------------------------------------------
// Verification tests for Step 30.3 — TLS verification
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};
    use rcgen::{PKCS_ECDSA_P256_SHA256, PKCS_ED25519};
    use time::{Duration as TimeDuration, OffsetDateTime};

    fn make_p256_cert(device_id: &str) -> (String, Vec<u8>, KeyPair) {
        let key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).unwrap();
        let mut params = CertificateParams::new(vec![device_id.to_string()]).unwrap();
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, device_id);
        params.distinguished_name = dn;
        let cert = params.self_signed(&key).unwrap();
        (cert.pem(), cert.der().to_vec(), key)
    }

    fn make_p256_cert_with_validity(
        device_id: &str,
        not_before: OffsetDateTime,
        not_after: OffsetDateTime,
    ) -> (String, Vec<u8>, KeyPair) {
        let key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).unwrap();
        let mut params = CertificateParams::new(vec![device_id.to_string()]).unwrap();
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, device_id);
        params.distinguished_name = dn;
        params.not_before = not_before;
        params.not_after = not_after;
        let cert = params.self_signed(&key).unwrap();
        (cert.pem(), cert.der().to_vec(), key)
    }

    fn der_to_cert(der: &[u8]) -> CertificateDer<'static> {
        CertificateDer::from(der.to_vec())
    }

    fn now_offset() -> OffsetDateTime {
        OffsetDateTime::now_utc()
    }

    fn past() -> OffsetDateTime {
        now_offset() - TimeDuration::days(365 * 20)
    }

    /// A cert whose `notAfter` is in the past MUST fail validity
    /// check. The verifier refuses the cert at handshake before
    /// even looking at signatures.
    #[test]
    fn expired_cert_fails_at_handshake() {
        let (_pem, der, _key) =
            make_p256_cert_with_validity("peer-1", past(), now_offset() - TimeDuration::days(1));
        let entry = der_to_cert(&der);
        let verifier = TrustedDeviceVerifier::new(vec![entry.clone()], VerifierPolicy::default());

        let result = verifier.verify_server_cert(
            &entry,
            &[],
            &tokio_rustls::rustls::pki_types::ServerName::try_from("peer-1").unwrap(),
            &[],
            UnixTime::now(),
        );
        assert!(result.is_err(), "expired cert must be rejected");
    }

    /// A cert whose `notBefore` is in the future MUST also fail.
    #[test]
    fn not_yet_valid_cert_fails_at_handshake() {
        let future = now_offset() + TimeDuration::days(365);
        let (_pem, der, _key) =
            make_p256_cert_with_validity("peer-1", future, future + TimeDuration::days(365));
        let entry = der_to_cert(&der);
        let verifier = TrustedDeviceVerifier::new(vec![entry.clone()], VerifierPolicy::default());

        let result = verifier.verify_server_cert(
            &entry,
            &[],
            &tokio_rustls::rustls::pki_types::ServerName::try_from("peer-1").unwrap(),
            &[],
            UnixTime::now(),
        );
        assert!(result.is_err(), "not-yet-valid cert must be rejected");
    }

    /// A cert whose device_id (extracted from SAN) does not match
    /// the verifier's `expected_device_id` MUST fail. This is the
    /// "outbound endpoint's certificate identity must match the
    /// device selected through discovery" check.
    #[test]
    fn mismatched_device_id_fails_when_policy_set() {
        let (_pem, der, _key) = make_p256_cert("peer-real");
        let entry = der_to_cert(&der);
        let verifier = TrustedDeviceVerifier::new(
            vec![entry.clone()],
            VerifierPolicy {
                expected_device_id: Some("peer-expected".into()),
            },
        );
        let result = verifier.verify_server_cert(
            &entry,
            &[],
            &tokio_rustls::rustls::pki_types::ServerName::try_from("peer-real").unwrap(),
            &[],
            UnixTime::now(),
        );
        assert!(result.is_err(), "device_id mismatch must reject");
    }

    /// A cert that does match the expected device_id is accepted.
    #[test]
    fn matching_device_id_succeeds() {
        let (_pem, der, _key) = make_p256_cert("peer-1");
        let entry = der_to_cert(&der);
        let verifier = TrustedDeviceVerifier::new(
            vec![entry.clone()],
            VerifierPolicy {
                expected_device_id: Some("peer-1".into()),
            },
        );
        let result = verifier.verify_server_cert(
            &entry,
            &[],
            &tokio_rustls::rustls::pki_types::ServerName::try_from("peer-1").unwrap(),
            &[],
            UnixTime::now(),
        );
        assert!(result.is_ok(), "matching device_id must accept: {result:?}");
    }

    /// One malformed cert in the trusted set MUST NOT disable the
    /// other valid certs. The verifier filters unparseable rows
    /// out of the trusted set at construction time, so a malformed
    /// entry is silently dropped and the well-formed ones still
    /// verify.
    #[test]
    fn malformed_trusted_cert_does_not_disable_valid_peers() {
        let (_pem1, der1, _key1) = make_p256_cert("peer-1");
        let (_pem2, der2, _key2) = make_p256_cert("peer-2");
        let garbage = der_to_cert(b"not a real cert at all");

        let verifier = TrustedDeviceVerifier::new(
            vec![garbage, der_to_cert(&der1), der_to_cert(&der2)],
            VerifierPolicy::default(),
        );

        let entry1 = der_to_cert(&der1);
        let r1 = verifier.verify_server_cert(
            &entry1,
            &[],
            &tokio_rustls::rustls::pki_types::ServerName::try_from("peer-1").unwrap(),
            &[],
            UnixTime::now(),
        );
        assert!(r1.is_ok(), "valid peer-1 must still verify: {r1:?}");

        let entry2 = der_to_cert(&der2);
        let r2 = verifier.verify_server_cert(
            &entry2,
            &[],
            &tokio_rustls::rustls::pki_types::ServerName::try_from("peer-2").unwrap(),
            &[],
            UnixTime::now(),
        );
        assert!(r2.is_ok(), "valid peer-2 must still verify: {r2:?}");
    }

    /// A cert NOT in the trusted set MUST fail at handshake.
    #[test]
    fn unknown_cert_fails_at_handshake() {
        let (_pem, der, _key) = make_p256_cert("peer-known");
        let (_other_pem, other_der, _other_key) = make_p256_cert("peer-unknown");
        let entry = der_to_cert(&der);
        let verifier = TrustedDeviceVerifier::new(vec![entry], VerifierPolicy::default());

        let unknown = der_to_cert(&other_der);
        let result = verifier.verify_server_cert(
            &unknown,
            &[],
            &tokio_rustls::rustls::pki_types::ServerName::try_from("peer-unknown").unwrap(),
            &[],
            UnixTime::now(),
        );
        assert!(result.is_err(), "unknown cert must be rejected");
    }

    /// An Ed25519 cert (the algorithm `PKCS_ED25519` produces) is
    /// extractable as a PublicKeyInfo::Ed25519. Step 30.3 adds
    /// support for these even though our own devices default to
    /// ECDSA P-256.
    #[test]
    fn ed25519_cert_extracts_ed25519_public_key() {
        let key = KeyPair::generate_for(&PKCS_ED25519).unwrap();
        let mut params = CertificateParams::new(vec!["peer-ed".to_string()]).unwrap();
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, "peer-ed");
        params.distinguished_name = dn;
        let cert = params.self_signed(&key).unwrap();
        let der = cert.der().to_vec();
        let info = cert_validation::extract_public_key_info(&der).expect("Ed25519 expected");
        assert!(matches!(info, PublicKeyInfo::Ed25519(_)));
        let _ = key; // would be used by a real handshake-signature test
    }
}
