use tokio::io::AsyncWriteExt;
use tokio_rustls::server::TlsStream;

use crate::error::AppError;
use crate::sync::cert_match::PeerIdentity;

/// Handle a freshly-accepted inbound mTLS session. This is the dispatch
/// point for Step 5's delta exchange protocol; for now it cleanly closes
/// the stream after logging the peer.
///
/// The caller has already validated the peer's cert against
/// `trusted_devices`, so `peer` is known and at least one shared space
/// exists. Per-space sync logic belongs in the protocol module that will
/// live alongside this one.
pub async fn handle_inbound<S>(
    mut stream: TlsStream<S>,
    peer: PeerIdentity,
) -> Result<(), AppError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    tracing_log_peer(&peer);
    stream
        .shutdown()
        .await
        .map_err(|e| AppError::Io(format!("session shutdown: {e}")))?;
    Ok(())
}

/// Eprintln rather than a full tracing setup — the sync engine is still
/// being wired and we don't want to pull in a logging stack yet.
fn tracing_log_peer(peer: &PeerIdentity) {
    eprintln!(
        "sync: inbound session from device_id={} spaces={:?}",
        peer.device_id, peer.shared_space_ids
    );
}
