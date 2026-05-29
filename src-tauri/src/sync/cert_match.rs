use sqlx::SqlitePool;
use tokio_rustls::rustls::pki_types::CertificateDer;

use crate::error::AppError;

/// A peer that we trust for at least one space, resolved from its TLS cert.
#[derive(Debug, Clone)]
pub struct PeerIdentity {
    pub device_id: String,
    pub shared_space_ids: Vec<String>,
}

/// Resolve a TLS-presented certificate to a trusted peer by comparing its
/// DER bytes against every active `trusted_devices` row. Returns `None`
/// if the cert is not in the trusted set for any space.
///
/// Equality is on DER, not PEM: PEM formatting (line widths, trailing
/// newlines) varies between encoders, but DER is canonical.
pub async fn resolve_peer(
    db: &SqlitePool,
    presented: &CertificateDer<'_>,
) -> Result<Option<PeerIdentity>, AppError> {
    let rows = sqlx::query_file!("queries/sync/list_all_trusted_certs_with_space.sql")
        .fetch_all(db)
        .await
        .map_err(|e| AppError::Db(format!("resolve peer: {e}")))?;

    let mut device_id: Option<String> = None;
    let mut shared_space_ids: Vec<String> = Vec::new();

    for r in rows {
        let mut reader = r.cert_pem.as_bytes();
        for parsed in rustls_pemfile::certs(&mut reader) {
            let der = parsed.map_err(|e| {
                AppError::Internal(format!("parse trusted cert for {}: {e}", r.device_id))
            })?;
            if der.as_ref() == presented.as_ref() {
                if device_id.is_none() {
                    device_id = Some(r.device_id.clone());
                }
                shared_space_ids.push(r.space_id.clone());
                break;
            }
        }
    }

    Ok(device_id.map(|device_id| PeerIdentity {
        device_id,
        shared_space_ids,
    }))
}
