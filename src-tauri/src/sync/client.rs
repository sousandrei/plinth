use sqlx::SqlitePool;
use tauri::AppHandle;
use tokio::net::TcpStream;

use crate::error::AppError;
use crate::sync::cert_match::resolve_peer;
use crate::sync::discovery::PeerInfo;
use crate::sync::identity::DeviceIdentity;
use crate::sync::session;
use crate::sync::tls;

pub async fn dial(
    peer: &PeerInfo,
    db: &SqlitePool,
    identity: &DeviceIdentity,
    app: AppHandle,
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

    session::handle_outbound(tls, peer_identity, db.clone(), app).await
}

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

    resolve_peer(db, leaf).await?.ok_or_else(|| {
        AppError::Internal("server cert not in trusted_devices (post-handshake)".into())
    })
}
