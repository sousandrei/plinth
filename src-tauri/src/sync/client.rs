use std::time::Duration;

use sqlx::SqlitePool;
use tauri::AppHandle;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

use crate::error::AppError;
use crate::sync::cert_match::resolve_peer;
use crate::sync::discovery::PeerInfo;
use crate::sync::frame;
use crate::sync::identity::DeviceIdentity;
use crate::sync::session;
use crate::sync::tls;
use crate::sync::wire::{Bye, Frame, Ping};

/// Step 30.3: 5-second budget for the TCP connect. Aggressive enough
/// to surface dead peers quickly, forgiving enough for one round of
/// LAN retransmits.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// Step 30.3: 5-second budget for the TLS handshake (which includes
/// the device_id / validity / signature checks the verifier now
/// performs). Same rationale as above.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

pub async fn dial(
    peer: &PeerInfo,
    db: &SqlitePool,
    identity: &DeviceIdentity,
    app: AppHandle,
) -> Result<(), AppError> {
    let addr = format!("{}:{}", peer.host, peer.port);

    let tcp = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(&addr))
        .await
        .map_err(|_| AppError::Io(format!("dial {addr}: connect timeout")))?
        .map_err(|e| AppError::Io(format!("dial {addr}: {e}")))?;

    let connector = tls::client_connector_for_peer(db, identity, peer.device_id.clone()).await?;
    let server_name = tokio_rustls::rustls::pki_types::ServerName::try_from(peer.device_id.clone())
        .map_err(|e| AppError::Internal(format!("server name from device_id: {e}")))?;

    let tls = tokio::time::timeout(HANDSHAKE_TIMEOUT, connector.connect(server_name, tcp))
        .await
        .map_err(|_| AppError::Io(format!("dial {addr}: handshake timeout")))?
        .map_err(|e| AppError::Io(format!("tls connect {addr}: {e}")))?;

    let peer_identity = extract_peer_identity(&tls, db).await?;

    session::handle_outbound(tls, peer_identity, db.clone(), app).await
}

/// Lightweight presence probe. TCP + mTLS + `Ping` + `Pong` + `Bye`. No
/// `Hello` exchange, no cursor exchange, no batch shipping. Returns
/// `Ok(())` when the peer responded with `Pong`; any other outcome is an
/// error so the caller can `peers.touch()` only on success.
pub async fn ping(
    peer: &PeerInfo,
    db: &SqlitePool,
    identity: &DeviceIdentity,
) -> Result<(), AppError> {
    let addr = format!("{}:{}", peer.host, peer.port);

    let tcp = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(&addr))
        .await
        .map_err(|_| AppError::Io(format!("ping tcp {addr}: timeout")))?
        .map_err(|e| AppError::Io(format!("ping tcp {addr}: {e}")))?;

    let connector = tls::client_connector_for_peer(db, identity, peer.device_id.clone()).await?;
    let server_name = tokio_rustls::rustls::pki_types::ServerName::try_from(peer.device_id.clone())
        .map_err(|e| AppError::Internal(format!("server name from device_id: {e}")))?;

    let mut tls = tokio::time::timeout(HANDSHAKE_TIMEOUT, connector.connect(server_name, tcp))
        .await
        .map_err(|_| AppError::Io(format!("ping tls {addr}: handshake timeout")))?
        .map_err(|e| AppError::Io(format!("ping tls {addr}: {e}")))?;

    frame::write_frame(&mut tls, &Frame::Ping(Ping {})).await?;

    match frame::read_frame(&mut tls).await? {
        Frame::Pong(_) => {}
        other => {
            return Err(AppError::Internal(format!(
                "ping: expected Pong, got {:?}",
                std::mem::discriminant(&other)
            )));
        }
    }

    frame::write_frame(&mut tls, &Frame::Bye(Bye {})).await?;
    tls.flush()
        .await
        .map_err(|e| AppError::Io(format!("ping flush: {e}")))?;
    Ok(())
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

    resolve_peer(db, leaf)
        .await?
        .ok_or_else(|| AppError::Internal("server cert not in devices (post-handshake)".into()))
}
