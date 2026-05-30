use std::sync::Arc;

use sqlx::SqlitePool;
use tauri::AppHandle;

use crate::error::AppError;
use crate::sync::debounce::{new_debounce, DebounceHandle};
use crate::sync::{discovery, identity, scheduler, server, tls, PairingState, PeerRegistry};

pub struct SyncRuntime {
    pub peers: PeerRegistry,
    pub pairing: Arc<PairingState>,
    pub local_addr: std::net::SocketAddr,
    pub debounce: DebounceHandle,
}

pub async fn start(handle: AppHandle, db: SqlitePool) -> Result<SyncRuntime, AppError> {
    tls::install_crypto_provider();

    let identity = identity::ensure_identity(&db).await?;
    let identity = Arc::new(identity);

    let server_handle = server::spawn(db.clone(), identity.clone(), handle.clone()).await?;

    let peers = PeerRegistry::new();
    discovery::spawn(handle.clone(), db.clone(), peers.clone(), server_handle.local_addr.port());

    let (debounce_handle, debounce_trigger) = new_debounce();
    scheduler::spawn(peers.clone(), db, identity, handle, debounce_trigger);

    Ok(SyncRuntime {
        peers,
        pairing: Arc::new(PairingState::new()),
        local_addr: server_handle.local_addr,
        debounce: debounce_handle,
    })
}
