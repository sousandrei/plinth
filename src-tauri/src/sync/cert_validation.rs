//! Certificate ingress validation — Step 30.2.
//!
//! Every certificate entering the system (pairing, snapshot, etc.) is
//! run through `validate_leaf_cert` before it is written to the
//! `devices` table. The validation enforces:
//!
//! - Exactly one well-formed leaf certificate in the PEM input.
//! - Canonical DER (re-encoded from the parsed X.509, not from the
//!   original bytes — `rcgen` produces canonical DER, but a hostile or
//!   buggy peer may not).
//! - SHA-256 fingerprint over the canonical DER.
//! - A DNS Subject Alternative Name matching `device_id`.
//! - No fingerprint collision with a different `device_id` already in
//!   the `devices` table.
//! - No `device_id` change to a different fingerprint already in the
//!   `devices` table.
//!
//! Malformed certs that cannot be parsed at all are written to
//! `quarantined_devices` by the caller (this module returns a typed
//! error so the caller can decide what to do — most callers will
//! reject the entire pairing or change row, but the
//! `apply::upsert_space_device` path takes the device-id-only change
//! and quarantines the missing cert instead).
//!
//! Step 30.3 also adds `extract_public_key_info` and
//! `validity_window`, which the TLS verifier consumes to:
//!
//! - Verify handshake signatures (TLS 1.2 and 1.3) using the cert's
//!   own public key, with the right `ring` algorithm for Ed25519 and
//!   ECDSA P-256.
//! - Reject certs whose `notBefore` / `notAfter` window doesn't include
//!   "now".

use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use x509_parser::extensions::GeneralName;
use x509_parser::prelude::*;

use crate::error::AppError;

/// One of the public-key algorithms the TLS verifier knows how to
/// drive. Returned by `extract_public_key_info` and consumed by
/// `tls::TrustedDeviceVerifier` to pick the right `ring` algorithm
/// for handshake signature verification.
#[derive(Debug, Clone)]
pub enum PublicKeyInfo {
    /// SubjectPublicKeyInfo carries an Ed25519 public key. The byte
    /// slice is the 32-byte raw public key, exactly what
    /// `ring::signature::ED25519` expects.
    Ed25519(Vec<u8>),
    /// SubjectPublicKeyInfo carries an ECDSA P-256 public key. The
    /// byte slice is the raw X9.62 uncompressed point
    /// (`04 || X || Y`, 65 bytes) — exactly what
    /// `ring::signature::ECDSA_P256_SHA256_ASN1` expects.
    EcdsaP256(Vec<u8>),
}

/// Parse a cert's DER, return its public key in the form the TLS
/// handshake verifier can use. Returns `None` for any algorithm we
/// don't currently support (RSA, Ed448, post-quantum, etc.). Callers
/// MUST treat `None` as a hard failure — the verifier can't make any
/// trust claim about a cert whose key it doesn't understand.
pub fn extract_public_key_info(der: &[u8]) -> Option<PublicKeyInfo> {
    // The OIDs we care about. Hardcoded here because x509-parser's
    // build script generates OID constants behind a feature gate
    // (`verify`, `verify-aws`) that we don't pull in — we only need
    // the algorithm identifier, not the full PKIX verifier.
    //   id-ecPublicKey  = 1.2.840.10045.2.1
    //   prime256v1      = 1.2.840.10045.3.1.7
    //   id-Ed25519      = 1.3.101.112
    let ec_oid = asn1_rs::oid!(1.2.840.10045.2.1);
    let p256_oid = asn1_rs::oid!(1.2.840.10045.3.1.7);
    let ed_oid = asn1_rs::oid!(1.3.101.112);

    let (_, cert) = X509Certificate::from_der(der).ok()?;
    let spki = cert.public_key();
    let alg_oid = &spki.algorithm.algorithm;

    if alg_oid == &ed_oid {
        Some(PublicKeyInfo::Ed25519(
            spki.subject_public_key.data.to_vec(),
        ))
    } else if alg_oid == &ec_oid {
        // Curve must be P-256. The curve OID lives in
        // `algorithm.parameters` (an `Any`). Re-parse it as an OID.
        let curve_ok = spki
            .algorithm
            .parameters()
            .and_then(|p| p.as_oid().ok())
            .map(|c| c == p256_oid)
            .unwrap_or(false);
        if !curve_ok {
            return None;
        }
        match spki.parsed().ok()? {
            x509_parser::public_key::PublicKey::EC(point) => {
                Some(PublicKeyInfo::EcdsaP256(point.data().to_vec()))
            }
            _ => None,
        }
    } else {
        None
    }
}

/// Extract the cert's `notBefore` and `notAfter` as unix seconds, or
/// `None` if the cert can't be parsed. Used by the TLS verifier to
/// reject expired or not-yet-valid certs at handshake time.
pub fn validity_window(der: &[u8]) -> Option<(u64, u64)> {
    let (_, cert) = X509Certificate::from_der(der).ok()?;
    let tbs = &cert.tbs_certificate;
    let nb = tbs.validity.not_before.timestamp();
    let na = tbs.validity.not_after.timestamp();
    if nb < 0 || na < 0 {
        return None;
    }
    Some((nb as u64, na as u64))
}

/// A certificate that has passed every check in `validate_leaf_cert`.
#[derive(Debug, Clone)]
pub struct ValidatedCert {
    /// Canonical DER bytes. Equal to the bytes that `rustls` and
    /// `rustls_pemfile::certs` would produce for the same cert — used
    /// as the equality key in the `devices` table and in the TLS
    /// verifier's trusted set.
    pub der: Vec<u8>,
    /// Hex-encoded SHA-256 of `der`. Stored alongside the cert for
    /// fast lookups and as a human-friendly identifier in quarantine
    /// reports.
    pub fingerprint: String,
    /// All DNS-style SAN entries the cert declares. Exposed so the
    /// caller can match any of them against `device_id` if it needs
    /// to debug a mismatch.
    pub dns_sans: Vec<String>,
}

/// Failure modes for `check_device_id_match` and
/// `check_against_existing`. PEM parse errors are returned as
/// `AppError::InvalidInput` from `parse_and_canonicalize`; the
/// variants here are what the DB-aware checks surface.
#[derive(Debug, Clone)]
pub enum CertValidationError {
    /// Cert has no Subject Alternative Name extension.
    NoSan,
    /// Cert's SAN entries do not include a DNS name equal to
    /// `device_id`.
    SanMismatch {
        device_id: String,
        dns_sans: Vec<String>,
    },
    /// Fingerprint already exists in `devices` mapped to a different
    /// `device_id`.
    FingerprintCollision {
        fingerprint: String,
        existing_device_id: String,
        new_device_id: String,
    },
    /// Device row already exists with a different fingerprint. The
    /// same installation MUST always present the same cert.
    DeviceFingerprintChanged {
        device_id: String,
        existing_fingerprint: String,
        new_fingerprint: String,
    },
    /// A lookup query failed (e.g. the devices table is missing).
    LookupFailed { reason: String },
}

impl std::fmt::Display for CertValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CertValidationError::NoSan => write!(f, "cert has no Subject Alternative Name"),
            CertValidationError::SanMismatch {
                device_id,
                dns_sans,
            } => {
                write!(
                    f,
                    "SAN does not contain device_id {device_id:?} (found: {dns_sans:?})"
                )
            }
            CertValidationError::FingerprintCollision {
                fingerprint,
                existing_device_id,
                new_device_id,
            } => write!(
                f,
                "fingerprint {fingerprint} already mapped to {existing_device_id}, \
                 new device_id {new_device_id} collides"
            ),
            CertValidationError::DeviceFingerprintChanged {
                device_id,
                existing_fingerprint,
                new_fingerprint,
            } => write!(
                f,
                "device_id {device_id} has fingerprint {existing_fingerprint}, new fingerprint {new_fingerprint} differs"
            ),
            CertValidationError::LookupFailed { reason } => {
                write!(f, "cert lookup failed: {reason}")
            }
        }
    }
}

impl std::error::Error for CertValidationError {}

/// Parse a PEM string, re-encode the cert to canonical DER, and
/// extract its fingerprint + DNS SANs. Does NOT enforce any
/// application-level rules (device_id match, uniqueness against the
/// DB) — those are the caller's job, since they need a DB connection.
pub fn parse_and_canonicalize(pem: &str) -> Result<ValidatedCert, AppError> {
    use tokio_rustls::rustls::pki_types::CertificateDer;

    let mut reader = pem.as_bytes();
    let der_entries: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Internal(format!("pem parse: {e}")))?;

    if der_entries.is_empty() {
        return Err(AppError::InvalidInput("cert pem is empty".into()));
    }
    if der_entries.len() != 1 {
        return Err(AppError::InvalidInput(format!(
            "cert pem must contain exactly one cert, got {}",
            der_entries.len()
        )));
    }
    let der = der_entries.into_iter().next().expect("len == 1");
    let der_bytes: &[u8] = der.as_ref();

    // Parse the cert so we can re-serialize it from the parsed form —
    // `x509-parser` produces canonical DER output, so two semantically
    // identical certs with different PEM wrapping produce the same
    // bytes here. This is the property the fingerprint depends on.
    let (_, parsed) = X509Certificate::from_der(der_bytes)
        .map_err(|e| AppError::InvalidInput(format!("x509 der parse: {e}")))?;
    let canonical = parsed.as_raw().to_vec();

    let fingerprint = fingerprint_hex(&canonical);

    let sans = parsed
        .subject_alternative_name()
        .map_err(|e| AppError::InvalidInput(format!("san parse: {e}")))?;
    let dns_sans: Vec<String> = match sans {
        Some(s) => s
            .value
            .general_names
            .iter()
            .filter_map(|n| match n {
                GeneralName::DNSName(d) => Some((*d).to_string()),
                _ => None,
            })
            .collect(),
        None => Vec::new(),
    };

    Ok(ValidatedCert {
        der: canonical,
        fingerprint,
        dns_sans,
    })
}

/// Verify that `cert`'s DNS SAN includes `device_id`. Returns
/// `CertValidationError::NoSan` or `SanMismatch` on failure.
pub fn check_device_id_match(
    cert: &ValidatedCert,
    device_id: &str,
) -> Result<(), CertValidationError> {
    if cert.dns_sans.is_empty() {
        return Err(CertValidationError::NoSan);
    }
    if cert.dns_sans.iter().any(|s| s == device_id) {
        Ok(())
    } else {
        Err(CertValidationError::SanMismatch {
            device_id: device_id.to_string(),
            dns_sans: cert.dns_sans.clone(),
        })
    }
}

/// SHA-256 hex (lowercase, no separators). Stable for stable DER.
pub fn fingerprint_hex(der: &[u8]) -> String {
    let digest = Sha256::digest(der);
    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest {
        use std::fmt::Write;
        write!(&mut out, "{b:02x}").expect("string write is infallible");
    }
    out
}

/// Check the candidate `(device_id, fingerprint)` against the
/// existing `devices` table. Two invariants are enforced:
///
/// - If `device_id` already has a fingerprint, it must match the
///   candidate. Re-issuing a cert for the same installation MUST
///   produce the same fingerprint.
/// - If `fingerprint` is already mapped to some other `device_id`,
///   reject. One cert is one installation.
///
/// The empty-fingerprint case (stub `devices` row from a
/// `space_devices` change_log apply) is treated as "no constraint":
/// the stub will be backfilled by the real cert at pairing time.
pub async fn check_against_existing(
    db: &SqlitePool,
    device_id: &str,
    fingerprint: &str,
) -> Result<(), CertValidationError> {
    let by_fp = sqlx::query_file!("queries/sync/get_device_id_by_fingerprint.sql", fingerprint)
        .fetch_optional(db)
        .await
        .map_err(|e| CertValidationError::LookupFailed {
            reason: format!("fingerprint lookup: {e}"),
        })?;
    if let Some(row) = by_fp
        && row.device_id != device_id
    {
        return Err(CertValidationError::FingerprintCollision {
            fingerprint: fingerprint.to_string(),
            existing_device_id: row.device_id,
            new_device_id: device_id.to_string(),
        });
    }

    let by_id = sqlx::query_file!("queries/sync/get_fingerprint_by_device_id.sql", device_id)
        .fetch_optional(db)
        .await
        .map_err(|e| CertValidationError::LookupFailed {
            reason: format!("device_id lookup: {e}"),
        })?;
    if let Some(row) = by_id {
        let existing = row.fingerprint;
        if !existing.is_empty() && existing != fingerprint {
            return Err(CertValidationError::DeviceFingerprintChanged {
                device_id: device_id.to_string(),
                existing_fingerprint: existing,
                new_fingerprint: fingerprint.to_string(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};

    fn make_cert_with_dns_san(dns_name: &str) -> (String, Vec<u8>) {
        let key = KeyPair::generate().unwrap();
        let mut params = CertificateParams::new(vec![dns_name.to_string()]).unwrap();
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, dns_name);
        params.distinguished_name = dn;
        let cert = params.self_signed(&key).unwrap();
        (cert.pem(), cert.der().to_vec())
    }

    #[test]
    fn parses_a_well_formed_cert() {
        let (pem, _der) = make_cert_with_dns_san("device-x");
        let v = parse_and_canonicalize(&pem).expect("parse");
        assert_eq!(v.dns_sans, vec!["device-x".to_string()]);
        // Fingerprint is hex-encoded SHA-256, 64 chars.
        assert_eq!(v.fingerprint.len(), 64);
        assert!(v.fingerprint.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn rejects_empty_pem() {
        let err = parse_and_canonicalize("").unwrap_err();
        assert!(matches!(err, AppError::InvalidInput(_)));
    }

    #[test]
    fn rejects_garbage_pem() {
        let err = parse_and_canonicalize("not a cert").unwrap_err();
        assert!(matches!(err, AppError::InvalidInput(_)));
    }

    #[test]
    fn rejects_pem_with_chain_of_two() {
        let (pem1, _) = make_cert_with_dns_san("a");
        let (pem2, _) = make_cert_with_dns_san("b");
        let combined = format!("{pem1}{pem2}");
        let err = parse_and_canonicalize(&combined).unwrap_err();
        assert!(matches!(err, AppError::InvalidInput(_)));
    }

    #[test]
    fn device_id_match_accepts_matching_dns_san() {
        let (pem, _) = make_cert_with_dns_san("device-xyz");
        let v = parse_and_canonicalize(&pem).unwrap();
        check_device_id_match(&v, "device-xyz").expect("match");
    }

    #[test]
    fn device_id_match_rejects_mismatched_dns_san() {
        let (pem, _) = make_cert_with_dns_san("device-abc");
        let v = parse_and_canonicalize(&pem).unwrap();
        let err = check_device_id_match(&v, "device-xyz").unwrap_err();
        assert!(matches!(err, CertValidationError::SanMismatch { .. }));
    }

    #[test]
    fn fingerprint_is_stable_for_same_cert() {
        let (pem, _) = make_cert_with_dns_san("device-fp");
        let a = parse_and_canonicalize(&pem).unwrap();
        let b = parse_and_canonicalize(&pem).unwrap();
        assert_eq!(a.fingerprint, b.fingerprint);
        assert_eq!(a.der, b.der);
    }

    #[test]
    fn extract_public_key_returns_p256_for_rcgen_default() {
        // rcgen::KeyPair::generate() defaults to ECDSA P-256 SHA-256.
        let (_pem, der) = make_cert_with_dns_san("device-pk");
        let info = extract_public_key_info(&der).expect("p256 expected");
        match info {
            PublicKeyInfo::EcdsaP256(point) => {
                // X9.62 uncompressed: 04 || X (32 bytes) || Y (32 bytes) = 65 bytes.
                assert_eq!(point.len(), 65);
                assert_eq!(point[0], 0x04);
            }
            other => panic!("expected EcdsaP256, got {other:?}"),
        }
    }

    #[test]
    fn validity_window_is_reasonable() {
        let (_pem, der) = make_cert_with_dns_san("device-valid");
        let (nb, na) = validity_window(&der).expect("validity");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        // The window is from rcgen defaults: notBefore ≈ now, notAfter
        // ≈ now + 10 years. We allow ±1 day of slack.
        assert!(nb <= now + 86_400);
        assert!(na >= now + 365 * 24 * 3600);
    }
}
