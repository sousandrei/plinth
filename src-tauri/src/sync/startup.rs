use std::sync::Arc;

use sqlx::SqlitePool;
use tauri::AppHandle;

use crate::error::AppError;
use crate::sync::debounce::{new_debounce, DebounceHandle};
use crate::sync::{discovery, identity, scheduler, server, tls, PairingState, PeerRegistry};

/// Everything the sync engine exposes to the rest of the app after startup.
/// Held as Tauri state so commands can reach pairing + peer registry.
pub struct SyncRuntime {
    pub peers: PeerRegistry,
    pub pairing: Arc<PairingState>,
    pub local_addr: std::net::SocketAddr,
    /// Commands call `debounce.notify_mutation()` after any DB write.
    /// Bulk import flows call `debounce.set_inhibited(true/false)`.
    pub debounce: DebounceHandle,
}

/// Boot the sync engine: load identity, build the mTLS acceptor, start
/// the listener, start mDNS discovery, and launch the outbound dial
/// scheduler. Returns the runtime handle for the caller to register
/// with Tauri.
pub async fn start(handle: AppHandle, db: SqlitePool) -> Result<SyncRuntime, AppError> {
    tls::install_crypto_provider();

    let identity = identity::ensure_identity(&db).await?;
    let identity = Arc::new(identity);

    let acceptor = tls::server_acceptor(&db, &identity).await?;
    let server_handle = server::spawn(db.clone(), acceptor, handle.clone()).await?;

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
