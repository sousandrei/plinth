use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use sqlx::SqlitePool;
use tauri::AppHandle;
use tokio::task::JoinHandle;
use tokio::time::sleep;

use crate::sync::client;
use crate::sync::debounce::DebounceTrigger;
use crate::sync::discovery::PeerRegistry;
use crate::sync::gc;
use crate::sync::identity::DeviceIdentity;

const DIAL_INTERVAL: Duration = Duration::from_secs(30);

/// Spawn the background dial-scheduler and return its handle. The task runs
/// forever until the process exits.
///
/// The scheduler wakes on two signals:
///   1. A 30-second periodic tick — syncs any peer not already in session.
///   2. A debounce fire — the 5s sliding timer expired after a DB mutation;
///      triggers an immediate sync round against all reachable peers.
pub fn spawn(
    peers: PeerRegistry,
    db: SqlitePool,
    identity: Arc<DeviceIdentity>,
    app: AppHandle,
    debounce: DebounceTrigger,
) -> JoinHandle<()> {
    tokio::spawn(run(peers, db, identity, app, debounce))
}

async fn run(
    peers: PeerRegistry,
    db: SqlitePool,
    identity: Arc<DeviceIdentity>,
    app: AppHandle,
    mut debounce: DebounceTrigger,
) {
    let in_flight: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));

    loop {
        tokio::select! {
            _ = sleep(DIAL_INTERVAL) => {},
            _ = debounce.wait_for_fire() => {},
        }

        dial_all_peers(&peers, &db, &identity, &app, &in_flight).await;
    }
}

async fn dial_all_peers(
    peers: &PeerRegistry,
    db: &SqlitePool,
    identity: &Arc<DeviceIdentity>,
    app: &AppHandle,
    in_flight: &Arc<Mutex<HashSet<String>>>,
) {
    let snapshot = peers.snapshot();

    for peer in snapshot {
        {
            let guard = in_flight.lock().unwrap();
            if guard.contains(&peer.device_id) {
                continue;
            }
        }
        in_flight.lock().unwrap().insert(peer.device_id.clone());

        let db = db.clone();
        let identity = identity.clone();
        let app = app.clone();
        let in_flight = in_flight.clone();
        let device_id = peer.device_id.clone();

        tokio::spawn(async move {
            match client::dial(&peer, &db, &identity, app).await {
                Ok(()) => {
                    if let Err(e) = gc::run(&db).await {
                        eprintln!("scheduler: gc after dial {}: {e}", peer.device_id);
                    }
                }
                Err(e) => {
                    eprintln!("scheduler: dial {} failed: {e}", peer.device_id);
                }
            }
            in_flight.lock().unwrap().remove(&device_id);
        });
    }
}
