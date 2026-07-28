use sqlx::SqlitePool;
use tokio_rustls::rustls::pki_types::CertificateDer;

use crate::error::AppError;
use crate::sync::trust_mode::TrustMode;

/// One (space, device) grant as observed at TLS-resolve time. The
/// `trust_mode` here drives the per-space filter in `session.rs` —
/// see `TrustMode::ships_outbound` and `TrustMode::accepts_inbound`.
#[derive(Debug, Clone)]
pub struct SharedSpaceGrant {
    pub space_id: String,
    pub trust_mode: TrustMode,
}

/// A peer that we trust for at least one space, resolved from its TLS cert.
#[derive(Debug, Clone)]
pub struct PeerIdentity {
    pub device_id: String,
    pub shared_spaces: Vec<SharedSpaceGrant>,
}

impl PeerIdentity {
    /// True if we share `space_id` with this peer at all — i.e. there
    /// is *some* grant (active, revoking, or revocation_only). Used as
    /// a generic existence check.
    #[allow(dead_code)] // public predicate for upcoming Step 30.4c/d
    pub fn shares_space(&self, space_id: &str) -> bool {
        self.shared_spaces.iter().any(|g| g.space_id == space_id)
    }

    /// Spaces we may ship data to this peer for. Filters out grants
    /// the peer has revoked or restricted.
    pub fn outbound_spaces(&self) -> impl Iterator<Item = &str> {
        self.shared_spaces
            .iter()
            .filter(|g| g.trust_mode.ships_outbound())
            .map(|g| g.space_id.as_str())
    }

    /// Spaces we accept incoming data for from this peer. A `revoking`
    /// grant is inbound-allowed so the peer can ship us the revocation
    /// record.
    #[allow(dead_code)] // used by session entry-point filters
    pub fn inbound_spaces(&self) -> impl Iterator<Item = &str> {
        self.shared_spaces
            .iter()
            .filter(|g| g.trust_mode.accepts_inbound())
            .map(|g| g.space_id.as_str())
    }

    pub fn outbound_contains(&self, space_id: &str) -> bool {
        self.shared_spaces
            .iter()
            .any(|g| g.space_id == space_id && g.trust_mode.ships_outbound())
    }

    pub fn inbound_contains(&self, space_id: &str) -> bool {
        self.shared_spaces
            .iter()
            .any(|g| g.space_id == space_id && g.trust_mode.accepts_inbound())
    }
}

/// Resolve a TLS-presented certificate to a trusted peer by comparing its
/// DER bytes against every device cert in the `devices` table. Returns `None`
/// if the cert is not in the trusted set for any space.
///
/// Equality is on DER, not PEM: PEM formatting (line widths, trailing
/// newlines) varies between encoders, but DER is canonical.
pub async fn resolve_peer(
    db: &SqlitePool,
    presented: &CertificateDer<'_>,
) -> Result<Option<PeerIdentity>, AppError> {
    let rows = sqlx::query_file!("queries/sync/list_device_certs_with_space.sql")
        .fetch_all(db)
        .await
        .map_err(|e| AppError::Db(format!("resolve peer: {e}")))?;

    let mut device_id: Option<String> = None;
    let mut shared_spaces: Vec<SharedSpaceGrant> = Vec::new();

    for r in rows {
        let mut reader = r.cert_pem.as_bytes();
        let mut matched = false;
        for parsed in rustls_pemfile::certs(&mut reader) {
            let der = parsed.map_err(|e| {
                AppError::Internal(format!("parse trusted cert for {}: {e}", r.device_id))
            })?;
            if der.as_ref() == presented.as_ref() {
                matched = true;
                break;
            }
        }
        if !matched {
            continue;
        }
        if device_id.is_none() {
            device_id = Some(r.device_id.clone());
        }
        let trust_mode = TrustMode::parse(&r.trust_mode).ok_or_else(|| {
            AppError::Internal(format!(
                "resolve_peer: unknown trust_mode {:?} for {}/{}",
                r.trust_mode, r.device_id, r.space_id
            ))
        })?;
        shared_spaces.push(SharedSpaceGrant {
            space_id: r.space_id.clone(),
            trust_mode,
        });
    }

    Ok(device_id.map(|device_id| PeerIdentity {
        device_id,
        shared_spaces,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_split_outbound_and_inbound() {
        let peer = PeerIdentity {
            device_id: "dev-1".into(),
            shared_spaces: vec![
                SharedSpaceGrant {
                    space_id: "s1".into(),
                    trust_mode: TrustMode::Active,
                },
                SharedSpaceGrant {
                    space_id: "s2".into(),
                    trust_mode: TrustMode::Revoking,
                },
                SharedSpaceGrant {
                    space_id: "s3".into(),
                    trust_mode: TrustMode::RevocationOnly,
                },
            ],
        };

        assert!(peer.shares_space("s1"));
        assert!(peer.shares_space("s2"));
        assert!(peer.shares_space("s3"));
        assert!(!peer.shares_space("s4"));

        // Outbound: only active.
        let out: Vec<&str> = peer.outbound_spaces().collect();
        assert_eq!(out, vec!["s1"]);

        // Inbound: active + revoking.
        let inbound: Vec<&str> = peer.inbound_spaces().collect();
        assert_eq!(inbound, vec!["s1", "s2"]);

        assert!(peer.outbound_contains("s1"));
        assert!(!peer.outbound_contains("s2"));
        assert!(!peer.outbound_contains("s3"));

        assert!(peer.inbound_contains("s1"));
        assert!(peer.inbound_contains("s2"));
        assert!(!peer.inbound_contains("s3"));
    }
}
