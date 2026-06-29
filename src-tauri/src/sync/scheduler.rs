use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tauri::AppHandle;
use tokio::task::JoinHandle;
use tokio::time::sleep;

use crate::error::AppError;
use crate::sync::client;
use crate::sync::debounce::DebounceTrigger;
use crate::sync::discovery::{PeerInfo, PeerRegistry};
use crate::sync::gc;
use crate::sync::identity::DeviceIdentity;

const DIAL_INTERVAL: Duration = Duration::from_secs(30);
const PING_INTERVAL: Duration = Duration::from_secs(2);
const PING_TIMEOUT: Duration = Duration::from_secs(2);

/// Shared set of device_ids currently being dialled. Used by both the
/// scheduler and `force_sync_now` to avoid launching two parallel
/// sessions for the same peer.
#[derive(Default, Clone)]
pub struct DialInFlight(pub Arc<Mutex<HashSet<String>>>);

impl DialInFlight {
    pub fn new() -> Self {
        Self::default()
    }
}

pub fn spawn(
    peers: PeerRegistry,
    db: SqlitePool,
    identity: Arc<DeviceIdentity>,
    app: AppHandle,
    debounce: DebounceTrigger,
    in_flight: DialInFlight,
) -> JoinHandle<()> {
    let ping_peers = peers.clone();
    let ping_db = db.clone();
    let ping_identity = identity.clone();
    tokio::spawn(ping_loop(ping_peers, ping_db, ping_identity));
    tokio::spawn(run(peers, db, identity, app, debounce, in_flight))
}

async fn ping_loop(peers: PeerRegistry, db: SqlitePool, identity: Arc<DeviceIdentity>) {
    loop {
        sleep(PING_INTERVAL).await;
        ping_all_peers(&peers, &db, &identity).await;
    }
}

/// Heartbeat ping every trusted peer. Independent from the full sync
/// loop: no `in_flight` coordination (TCP+TLS overhead per ping is
/// negligible and a ping alongside a sync keeps `last_seen` fresh during
/// long batch transfers).
async fn ping_all_peers(peers: &PeerRegistry, db: &SqlitePool, identity: &Arc<DeviceIdentity>) {
    let trusted = trusted_device_ids(db).await;

    let peer_map: HashMap<_, _> = peers
        .snapshot()
        .into_iter()
        .map(|p| (p.device_id.clone(), p))
        .collect();

    for device_id in &trusted {
        let Some(peer) = peer_map.get(device_id) else {
            continue;
        };

        let peer = peer.clone();
        let peers = peers.clone();
        let db = db.clone();
        let identity = identity.clone();
        let device_id = device_id.clone();

        tokio::spawn(async move {
            match tokio::time::timeout(PING_TIMEOUT, client::ping(&peer, &db, &identity)).await {
                Ok(Ok(())) => {
                    peers.touch(&device_id);
                }
                Ok(Err(e)) => {
                    eprintln!("scheduler: ping {device_id}: {e}");
                }
                Err(_) => {
                    eprintln!("scheduler: ping {device_id}: timeout");
                }
            }
        });
    }
}

async fn run(
    peers: PeerRegistry,
    db: SqlitePool,
    identity: Arc<DeviceIdentity>,
    app: AppHandle,
    mut debounce: DebounceTrigger,
    in_flight: DialInFlight,
) {
    loop {
        tokio::select! {
            _ = sleep(DIAL_INTERVAL) => {},
            _ = debounce.wait_for_fire() => {},
        }

        let (handles, _) = dial_all_peers(&peers, &db, &identity, &app, &in_flight).await;
        // Drop handles — tasks run detached, fire-and-forget.
        drop(handles);
    }
}

async fn trusted_device_ids(db: &SqlitePool) -> HashSet<String> {
    // Only dial peers that share at least one existing space.  When a space
    // is deleted the trusted_devices row is orphaned (no CASCADE) — we must
    // not dial that peer for a space we no longer have.
    match sqlx::query_file!("queries/sync/list_trusted_device_ids.sql")
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

pub async fn dial_all_peers(
    peers: &PeerRegistry,
    db: &SqlitePool,
    identity: &Arc<DeviceIdentity>,
    app: &AppHandle,
    in_flight: &DialInFlight,
) -> (Vec<JoinHandle<Result<(), AppError>>>, Vec<PeerInfo>) {
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

    let mut handles = Vec::new();
    let mut dialled = Vec::new();

    for device_id in &trusted {
        let Some(peer) = peer_map.get(device_id) else {
            // Trusted but not currently visible on the LAN — skip.
            continue;
        };

        {
            let mut guard = match in_flight.0.lock() {
                Ok(g) => g,
                Err(e) => {
                    eprintln!("scheduler: in_flight mutex poisoned: {e}");
                    continue;
                }
            };
            if guard.contains(device_id) {
                continue;
            }
            guard.insert(device_id.clone());
        }

        let peer_for_task = peer.clone();
        let peer_for_list = peer.clone();
        let peers = peers.clone();
        let db = db.clone();
        let identity = identity.clone();
        let app = app.clone();
        let in_flight = in_flight.clone();
        let device_id = device_id.clone();

        let handle = tokio::spawn(async move {
            let result = client::dial(&peer_for_task, &db, &identity, app).await;
            if result.is_ok() {
                peers.touch(&device_id);
                if let Err(e) = gc::run(&db).await {
                    eprintln!("scheduler: gc after dial {device_id}: {e}");
                }
            } else if let Err(ref e) = result {
                eprintln!("scheduler: dial {device_id} failed: {e}");
            }
            match in_flight.0.lock() {
                Ok(mut guard) => {
                    guard.remove(&device_id);
                }
                Err(e) => {
                    eprintln!("scheduler: in_flight mutex poisoned on cleanup: {e}");
                }
            }
            result
        });

        dialled.push(peer_for_list);
        handles.push(handle);
    }

    (handles, dialled)
}

/// Await a batch of dial tasks and fold their outcomes into a single
/// `SyncSummary`. Peers that didn't get a connection attempt (off-LAN or
/// skipped due to `in_flight`) are absent from the input and therefore
/// absent from `dialed` — callers that need to surface those separately
/// should layer their own messaging.
pub async fn await_dials(
    handles: Vec<JoinHandle<Result<(), AppError>>>,
    peers: Vec<PeerInfo>,
) -> SyncSummary {
    let dialed = handles.len() as u64;
    let mut ok = 0;
    let mut failed = Vec::new();

    for (handle, peer) in handles.into_iter().zip(peers) {
        match handle.await {
            Ok(Ok(())) => ok += 1,
            Ok(Err(e)) => failed.push(SyncFailure {
                device_id: peer.device_id,
                name: peer.name,
                error: e.to_string(),
            }),
            Err(join_err) => failed.push(SyncFailure {
                device_id: peer.device_id,
                name: peer.name,
                error: format!("sync task panicked: {join_err}"),
            }),
        }
    }

    SyncSummary { dialed, ok, failed }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SyncSummary {
    pub dialed: u64,
    pub ok: u64,
    pub failed: Vec<SyncFailure>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SyncFailure {
    pub device_id: String,
    pub name: String,
    pub error: String,
}
