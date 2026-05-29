use std::net::SocketAddr;

use sqlx::SqlitePool;
use tauri::AppHandle;
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use tokio_rustls::TlsAcceptor;

use crate::error::AppError;
use crate::sync::cert_match::{resolve_peer, PeerIdentity};
use crate::sync::session;

/// Handle to a running sync server task, plus the address it bound to.
/// The address is what gets re-advertised over mDNS by Step 4d.
pub struct ServerHandle {
    pub local_addr: SocketAddr,
    pub task: JoinHandle<()>,
}

/// Bind an ephemeral TCP port, return its address, and spawn the accept
/// loop in the background. Each accepted connection runs through mTLS
/// and, if the peer cert matches `trusted_devices`, is dispatched to
/// `session::handle_inbound`.
pub async fn spawn(db: SqlitePool, acceptor: TlsAcceptor, app: AppHandle) -> Result<ServerHandle, AppError> {
    let listener = TcpListener::bind("0.0.0.0:0")
        .await
        .map_err(|e| AppError::Io(format!("sync listen: {e}")))?;
    let local_addr = listener
        .local_addr()
        .map_err(|e| AppError::Io(format!("sync addr: {e}")))?;

    let task = tokio::spawn(accept_loop(listener, acceptor, db, app));

    Ok(ServerHandle { local_addr, task })
}

async fn accept_loop(listener: TcpListener, acceptor: TlsAcceptor, db: SqlitePool, app: AppHandle) {
    loop {
        let (stream, peer_addr) = match listener.accept().await {
            Ok(pair) => pair,
            Err(e) => {
                eprintln!("sync accept: {e}");
                continue;
            }
        };
        let acceptor = acceptor.clone();
        let db = db.clone();
        let app = app.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, acceptor, db, app).await {
                eprintln!("sync conn from {peer_addr}: {e}");
            }
        });
    }
}

async fn handle_connection(
    tcp: TcpStream,
    acceptor: TlsAcceptor,
    db: SqlitePool,
    app: AppHandle,
) -> Result<(), AppError> {
    let tls = acceptor
        .accept(tcp)
        .await
        .map_err(|e| AppError::Io(format!("tls accept: {e}")))?;

    let peer = extract_peer(&tls, &db).await?;
    session::handle_inbound(tls, peer, db, app).await
}

/// Read the peer's leaf cert off the completed handshake and look it up
/// in `trusted_devices`. The TLS verifier (`tls::TrustedDeviceVerifier`)
/// has already rejected unknown certs, but we still need the cert here
/// to resolve which device/spaces it belongs to.
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
