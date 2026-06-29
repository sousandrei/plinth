use std::sync::Arc;

use sqlx::SqlitePool;
use tauri::AppHandle;

use crate::error::AppError;
use crate::sync::debounce::{DebounceSender, new_debounce};
use crate::sync::{PairingState, PeerRegistry, discovery, identity, scheduler, server, tls};

pub struct SyncRuntime {
    pub peers: PeerRegistry,
    pub pairing: Arc<PairingState>,
    pub debounce: DebounceSender,
    pub in_flight: crate::sync::scheduler::DialInFlight,
}

pub async fn start(handle: AppHandle, db: SqlitePool) -> Result<SyncRuntime, AppError> {
    tls::install_crypto_provider();

    let identity = identity::ensure_identity(&db).await?;
    let identity = Arc::new(identity);

    let peers = PeerRegistry::new();

    let server_handle =
        server::spawn(db.clone(), identity.clone(), handle.clone(), peers.clone()).await?;

    discovery::spawn(db.clone(), peers.clone(), server_handle.local_addr.port());

    let (debounce_trigger, debounce_sender) = new_debounce();
    let in_flight = crate::sync::scheduler::DialInFlight::new();
    scheduler::spawn(
        peers.clone(),
        db,
        identity,
        handle,
        debounce_trigger,
        in_flight.clone(),
    );

    Ok(SyncRuntime {
        peers,
        pairing: Arc::new(PairingState::new()),
        debounce: debounce_sender,
        in_flight,
    })
}
