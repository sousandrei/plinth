use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use serde::Serialize;
use sqlx::SqlitePool;
use tauri::AppHandle;

use crate::error::AppError;

const SERVICE_TYPE: &str = "_plinth._tcp.local.";
const PEER_TTL_SECS: u64 = 90;

#[derive(Debug, Clone, Serialize)]
pub struct PeerInfo {
    pub device_id: String,
    pub host: String,
    pub port: u16,
    pub space_ids: Vec<String>,
    pub last_seen: u64,
}

#[derive(Debug, Default, Clone)]
pub struct PeerRegistry {
    inner: Arc<Mutex<HashMap<String, PeerInfo>>>,
}

impl PeerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn upsert(&self, peer: PeerInfo) {
        let mut guard = self.inner.lock().unwrap();
        guard.insert(peer.device_id.clone(), peer);
    }

    pub fn remove(&self, device_id: &str) {
        let mut guard = self.inner.lock().unwrap();
        guard.remove(device_id);
    }

    pub fn snapshot(&self) -> Vec<PeerInfo> {
        let guard = self.inner.lock().unwrap();
        guard.values().cloned().collect()
    }

    fn reap(&self, ttl_secs: u64) {
        let now = now_unix();
        let mut guard = self.inner.lock().unwrap();
        guard.retain(|_, p| now.saturating_sub(p.last_seen) < ttl_secs);
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Reads this device's stable ID from app_settings. Initialized in db.rs at
/// first launch, so by the time discovery starts it always exists.
async fn read_device_id(db: &SqlitePool) -> Result<String, AppError> {
    let row = sqlx::query_file!("queries/settings/get_setting.sql", "device_id")
        .fetch_optional(db)
        .await
        .map_err(|e| AppError::Db(format!("read device_id: {e}")))?;
    row.map(|r| r.value)
        .ok_or_else(|| AppError::Internal("device_id missing from app_settings".into()))
}

/// All space IDs visible on this device. Used to populate the mDNS TXT
/// record so peers can decide whether to attempt a sync session.
///
/// In Step 3 this will narrow to spaces that have at least one
/// `trusted_devices` row with `sync_enabled = 1`.
async fn read_advertised_space_ids(db: &SqlitePool) -> Result<Vec<String>, AppError> {
    let rows = sqlx::query_file!("queries/sync/list_advertised_space_ids.sql")
        .fetch_all(db)
        .await
        .map_err(|e| AppError::Db(format!("list advertised spaces: {e}")))?;
    Ok(rows.into_iter().map(|r| r.id).collect())
}

/// Starts the mDNS discovery background task. Registers this device's
/// service record under `_plinth._tcp.local` and subscribes to the same
/// service type to populate the peer registry.
///
/// Returns immediately after spawning. The returned `PeerRegistry` is
/// managed as Tauri state and queried by the `list_peers` command.
pub fn spawn(app: AppHandle, db: SqlitePool, registry: PeerRegistry) {
    tauri::async_runtime::spawn(async move {
        if let Err(e) = run(app, db, registry).await {
            eprintln!("sync::discovery: {e}");
        }
    });
}

async fn run(
    _app: AppHandle,
    db: SqlitePool,
    registry: PeerRegistry,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let device_id = read_device_id(&db).await?;
    let space_ids = read_advertised_space_ids(&db).await.unwrap_or_default();

    let daemon = ServiceDaemon::new()?;

    // Choose an ephemeral port for the future mTLS listener. For now this
    // is just advertised; the listener arrives in Step 5.
    let port: u16 = pick_ephemeral_port();
    let hostname = gethostname::gethostname().to_string_lossy().into_owned();
    let host_record = format!("{}.local.", sanitize_host(&hostname));

    let mut properties: HashMap<String, String> = HashMap::new();
    properties.insert("device_id".into(), device_id.clone());
    properties.insert("spaces".into(), space_ids.join(","));

    let info = ServiceInfo::new(
        SERVICE_TYPE,
        &device_id,
        &host_record,
        "",
        port,
        Some(properties),
    )?
    .enable_addr_auto();

    daemon.register(info)?;

    let receiver = daemon.browse(SERVICE_TYPE)?;

    let reaper_registry = registry.clone();
    tauri::async_runtime::spawn(async move {
        let mut ticker =
            tokio::time::interval(std::time::Duration::from_secs(PEER_TTL_SECS / 3));
        loop {
            ticker.tick().await;
            reaper_registry.reap(PEER_TTL_SECS);
        }
    });

    // Block on mDNS events. The channel is sync, so we hop to a blocking
    // thread for the recv loop.
    let registry_for_loop = registry.clone();
    let self_device_id = device_id.clone();
    tokio::task::spawn_blocking(move || {
        while let Ok(event) = receiver.recv() {
            match event {
                ServiceEvent::ServiceResolved(info) => {
                    let props = info.get_properties();
                    let Some(peer_device_id) = props.get_property_val_str("device_id") else {
                        continue;
                    };
                    if peer_device_id == self_device_id {
                        continue;
                    }
                    let space_ids = props
                        .get_property_val_str("spaces")
                        .map(|s| {
                            s.split(',')
                                .filter(|s| !s.is_empty())
                                .map(|s| s.to_string())
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    let host = info
                        .get_addresses()
                        .iter()
                        .next()
                        .map(|a| a.to_string())
                        .unwrap_or_else(|| info.get_hostname().to_string());

                    registry_for_loop.upsert(PeerInfo {
                        device_id: peer_device_id.to_string(),
                        host,
                        port: info.get_port(),
                        space_ids,
                        last_seen: now_unix(),
                    });
                }
                ServiceEvent::ServiceRemoved(_, fullname) => {
                    // fullname is "{instance}.{service_type}"; instance is the device_id
                    if let Some(instance) = fullname.split('.').next() {
                        registry_for_loop.remove(instance);
                    }
                }
                _ => {}
            }
        }
    });

    Ok(())
}

/// Picks an ephemeral TCP port by binding to 0 and closing immediately.
/// The OS is allowed to reassign this port — the Step 5 mTLS listener
/// will bind a fresh port anyway. For now the value advertised is just
/// a placeholder so peers see something concrete.
fn pick_ephemeral_port() -> u16 {
    std::net::TcpListener::bind("0.0.0.0:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .unwrap_or(0)
}

/// mDNS hostnames can only contain letters, digits, and hyphens.
fn sanitize_host(input: &str) -> String {
    input
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect()
}
