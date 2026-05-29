use sqlx::SqlitePool;
use tokio::net::TcpStream;

use crate::error::AppError;
use crate::sync::cert_match::resolve_peer;
use crate::sync::discovery::PeerInfo;
use crate::sync::identity::DeviceIdentity;
use crate::sync::session;
use crate::sync::tls;

/// Dial one discovered peer, complete the mTLS handshake, resolve the
/// peer identity from the server cert, and run a full delta-exchange
/// session. Returns normally when the session completes (both sides
/// have exchanged Bye) or errors on any transport or protocol failure.
///
/// This is called from the periodic scheduler in `sync/scheduler.rs`.
/// One dial attempt per peer per tick; the scheduler is responsible for
/// retry backoff.
pub async fn dial(
    peer: &PeerInfo,
    db: &SqlitePool,
    identity: &DeviceIdentity,
) -> Result<(), AppError> {
    let addr = format!("{}:{}", peer.host, peer.port);

    let tcp = TcpStream::connect(&addr)
        .await
        .map_err(|e| AppError::Io(format!("dial {addr}: {e}")))?;

    let connector = tls::client_connector(db, identity).await?;

    // The server cert's SAN is set to the peer's device_id UUID (a DNS-name
    // SAN) by `identity::ensure_identity`. We know the peer's device_id from
    // mDNS discovery via `peer.device_id`.
    let server_name = tokio_rustls::rustls::pki_types::ServerName::try_from(peer.device_id.clone())
        .map_err(|e| AppError::Internal(format!("server name from device_id: {e}")))?;

    let tls = connector
        .connect(server_name, tcp)
        .await
        .map_err(|e| AppError::Io(format!("tls connect {addr}: {e}")))?;

    let peer_identity = extract_peer_identity(&tls, db).await?;

    session::handle_outbound(tls, peer_identity, db.clone()).await
}

/// Read the server's leaf cert off the completed client-side handshake
/// and resolve it against `trusted_devices`. The TLS verifier has already
/// rejected untrusted certs, but we need the resolved identity to carry
/// `device_id` and `shared_space_ids` into the session.
async fn extract_peer_identity<S>(
    tls: &tokio_rustls::client::TlsStream<S>,
    db: &SqlitePool,
) -> Result<crate::sync::cert_match::PeerIdentity, AppError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let (_io, conn) = tls.get_ref();
    let certs = conn
        .peer_certificates()
        .ok_or_else(|| AppError::Internal("tls server presented no cert".into()))?;
    let leaf = certs
        .first()
        .ok_or_else(|| AppError::Internal("tls server cert chain empty".into()))?;

    resolve_peer(db, leaf)
        .await?
        .ok_or_else(|| AppError::Internal("server cert not in trusted_devices (post-handshake)".into()))
}
