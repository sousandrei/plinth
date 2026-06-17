use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};
use sqlx::SqlitePool;

use crate::error::AppError;

/// This device's persistent network identity.
#[derive(Debug, Clone)]
pub struct DeviceIdentity {
    pub device_id: String,
    pub cert_pem: String,
    pub key_pem: String,
}

/// Loads (and lazily generates) the device's self-signed certificate and
/// private key. The cert's Common Name is the `device_id`, so peers can
/// cross-check the cert against the pairing handshake. Stored in
/// `app_settings` for persistence.
pub async fn ensure_identity(db: &SqlitePool) -> Result<DeviceIdentity, AppError> {
    let rows = sqlx::query_file!("queries/sync/get_identity.sql")
        .fetch_all(db)
        .await
        .map_err(|e| AppError::Db(format!("read identity: {e}")))?;

    let mut device_id: Option<String> = None;
    let mut cert_pem: Option<String> = None;
    let mut key_pem: Option<String> = None;
    for r in rows {
        match r.key.as_str() {
            "device_id" => device_id = Some(r.value),
            "device_cert_pem" => cert_pem = Some(r.value),
            "device_key_pem" => key_pem = Some(r.value),
            _ => {}
        }
    }

    let device_id = device_id
        .ok_or_else(|| AppError::Internal("device_id missing from app_settings".into()))?;

    if let (Some(cert_pem), Some(key_pem)) = (cert_pem, key_pem) {
        return Ok(DeviceIdentity {
            device_id,
            cert_pem,
            key_pem,
        });
    }

    // First-time generation. Self-signed Ed25519 cert with the device_id
    // as the Common Name and a 10-year validity window.
    let key =
        KeyPair::generate().map_err(|e| AppError::Internal(format!("keypair generate: {e}")))?;
    let mut params = CertificateParams::new(vec![device_id.clone()])
        .map_err(|e| AppError::Internal(format!("cert params: {e}")))?;
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, device_id.clone());
    params.distinguished_name = dn;
    let cert = params
        .self_signed(&key)
        .map_err(|e| AppError::Internal(format!("cert sign: {e}")))?;

    let cert_pem = cert.pem();
    let key_pem = key.serialize_pem();

    let cert_key = "device_cert_pem".to_string();
    let key_key = "device_key_pem".to_string();
    let cert_val = cert_pem.clone();
    let key_val = key_pem.clone();
    sqlx::query_file!("queries/settings/set_setting.sql", cert_key, cert_val)
        .execute(db)
        .await
        .map_err(|e| AppError::Db(format!("save device_cert_pem: {e}")))?;
    sqlx::query_file!("queries/settings/set_setting.sql", key_key, key_val)
        .execute(db)
        .await
        .map_err(|e| AppError::Db(format!("save device_key_pem: {e}")))?;

    Ok(DeviceIdentity {
        device_id,
        cert_pem,
        key_pem,
    })
}
