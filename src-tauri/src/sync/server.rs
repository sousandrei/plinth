use std::net::SocketAddr;
use std::sync::Arc;

use sqlx::SqlitePool;
use tauri::AppHandle;
use tokio::net::{TcpListener, TcpStream};

use crate::error::AppError;
use crate::sync::cert_match::{PeerIdentity, resolve_peer};
use crate::sync::identity::DeviceIdentity;
use crate::sync::session;
use crate::sync::tls;

/// Handle to a running sync server task, plus the address it bound to.
pub struct ServerHandle {
    pub local_addr: SocketAddr,
}

pub async fn spawn(
    db: SqlitePool,
    identity: Arc<DeviceIdentity>,
    app: AppHandle,
) -> Result<ServerHandle, AppError> {
    let listener = TcpListener::bind("0.0.0.0:0")
        .await
        .map_err(|e| AppError::Io(format!("sync listen: {e}")))?;
    let local_addr = listener
        .local_addr()
        .map_err(|e| AppError::Io(format!("sync addr: {e}")))?;

    tokio::spawn(accept_loop(listener, db, identity, app));

    Ok(ServerHandle { local_addr })
}

async fn accept_loop(
    listener: TcpListener,
    db: SqlitePool,
    identity: Arc<DeviceIdentity>,
    app: AppHandle,
) {
    loop {
        let (stream, peer_addr) = match listener.accept().await {
            Ok(pair) => pair,
            Err(e) => {
                eprintln!("sync accept: {e}");
                continue;
            }
        };
        let db = db.clone();
        let identity = identity.clone();
        let app = app.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, db, identity, app).await {
                eprintln!("sync conn from {peer_addr}: {e}");
            }
        });
    }
}

async fn handle_connection(
    tcp: TcpStream,
    db: SqlitePool,
    identity: Arc<DeviceIdentity>,
    app: AppHandle,
) -> Result<(), AppError> {
    let acceptor = tls::server_acceptor(&db, &identity).await?;
    let tls = acceptor
        .accept(tcp)
        .await
        .map_err(|e| AppError::Io(format!("tls accept: {e}")))?;

    let peer = extract_peer(&tls, &db).await?;
    session::handle_inbound(tls, peer, db, app).await
}

async fn extract_peer<S>(
    tls: &tokio_rustls::server::TlsStream<S>,
    db: &SqlitePool,
) -> Result<PeerIdentity, AppError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let (_io, conn) = tls.get_ref();
    let certs = conn
        .peer_certificates()
        .ok_or_else(|| AppError::Internal("tls peer presented no cert".into()))?;
    let leaf = certs
        .first()
        .ok_or_else(|| AppError::Internal("tls peer cert chain empty".into()))?;

    resolve_peer(db, leaf)
        .await?
        .ok_or_else(|| AppError::Internal("peer cert not trusted (post-handshake)".into()))
}
