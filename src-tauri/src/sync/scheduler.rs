use std::collections::{HashSet, HashMap};
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

async fn trusted_device_ids(db: &SqlitePool) -> HashSet<String> {
    // Only dial peers that share at least one existing space.  When a space
    // is deleted the trusted_devices row is orphaned (no CASCADE) — we must
    // not dial that peer for a space we no longer have.
    match sqlx::query_file!(
        "queries/sync/list_trusted_device_ids.sql"
    )
    .fetch_all(db)
    .await
    {
        Ok(rows) => rows.into_iter().map(|r| r.device_id).collect(),
        Err(e) => {
            eprintln!("scheduler: load trusted device ids: {e}");
            HashSet::new()
        }
    }
}

async fn dial_all_peers(
    peers: &PeerRegistry,
    db: &SqlitePool,
    identity: &Arc<DeviceIdentity>,
    app: &AppHandle,
    in_flight: &Arc<Mutex<HashSet<String>>>,
) {
    // Only dial peers we have a trusted_devices row for — this prevents
    // handshake failures against unknown LAN peers and stops the noise
    // from devices we haven't paired with (or where pairing was cancelled).
    let trusted = trusted_device_ids(db).await;

    // Index peers by device_id for O(1) lookup.
    let peer_map: HashMap<_, _> = peers
        .snapshot()
        .into_iter()
        .map(|p| (p.device_id.clone(), p))
        .collect();

    for device_id in &trusted {
        let Some(peer) = peer_map.get(device_id) else {
            // Trusted but not currently visible on the LAN — skip.
            continue;
        };

        {
            let guard = in_flight.lock().unwrap();
            if guard.contains(device_id) {
                continue;
            }
        }
        in_flight.lock().unwrap().insert(device_id.clone());

        let peer = peer.clone();
        let db = db.clone();
        let identity = identity.clone();
        let app = app.clone();
        let in_flight = in_flight.clone();
        let device_id = device_id.clone();

        tokio::spawn(async move {
            match client::dial(&peer, &db, &identity, app).await {
                Ok(()) => {
                    if let Err(e) = gc::run(&db).await {
                        eprintln!("scheduler: gc after dial {device_id}: {e}");
                    }
                }
                Err(e) => {
                    eprintln!("scheduler: dial {device_id} failed: {e}");
                }
            }
            in_flight.lock().unwrap().remove(&device_id);
        });
    }
}
