use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use sqlx::SqlitePool;
use tokio::task::JoinHandle;
use tokio::time::sleep;

use crate::sync::client;
use crate::sync::discovery::PeerRegistry;
use crate::sync::identity::DeviceIdentity;

/// How often the scheduler wakes and attempts to sync with every known peer.
const DIAL_INTERVAL: Duration = Duration::from_secs(30);

/// Spawn the background dial-scheduler and return its handle. The task runs
/// forever until the process exits; the caller may drop the handle if it
/// doesn't need to await termination.
///
/// On each tick, every peer in the `PeerRegistry` that isn't already in an
/// active session gets a dial attempt in its own task. If the dial or session
/// fails, the error is logged and the peer will be retried on the next tick.
pub fn spawn(
    peers: PeerRegistry,
    db: SqlitePool,
    identity: Arc<DeviceIdentity>,
) -> JoinHandle<()> {
    tokio::spawn(run(peers, db, identity))
}

async fn run(peers: PeerRegistry, db: SqlitePool, identity: Arc<DeviceIdentity>) {
    // device_ids of peers that currently have an active outbound session.
    let in_flight: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));

    loop {
        sleep(DIAL_INTERVAL).await;

        let snapshot = peers.snapshot();

        for peer in snapshot {
            // Skip if a session to this peer is already running.
            {
                let guard = in_flight.lock().unwrap();
                if guard.contains(&peer.device_id) {
                    continue;
                }
            }
            in_flight.lock().unwrap().insert(peer.device_id.clone());

            let db = db.clone();
            let identity = identity.clone();
            let in_flight = in_flight.clone();
            let device_id = peer.device_id.clone();

            tokio::spawn(async move {
                if let Err(e) = client::dial(&peer, &db, &identity).await {
                    eprintln!("scheduler: dial {} failed: {e}", peer.device_id);
                }
                in_flight.lock().unwrap().remove(&device_id);
            });
        }
    }
}
