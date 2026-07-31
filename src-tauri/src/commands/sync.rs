use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sqlx::{Sqlite, Transaction};
use tauri::{AppHandle, State};

use crate::{
    Session,
    db::DbPool,
    error::AppError,
    sync::{
        PeerInfo, PeerRegistry, SyncSummary,
        debounce::DebounceSender,
        pairing::{self, PAIRING_PORT, PairToken, PairingState, WireUser},
        trust_mode::TrustMode,
    },
};

// ---------------------------------------------------------------------------
// Device identity
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_device_name() -> String {
    gethostname::gethostname().to_string_lossy().into_owned()
}

#[tauri::command]
pub fn get_local_address() -> Option<String> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    Some(socket.local_addr().ok()?.ip().to_string())
}

// ---------------------------------------------------------------------------
// Peer discovery
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_peers(registry: State<'_, PeerRegistry>) -> Result<Vec<PeerInfo>, AppError> {
    Ok(registry.snapshot())
}

/// Trigger an immediate sync with every visible trusted peer, bypassing
/// the scheduler's 30s polling interval. Awaits each peer's dial so the
/// caller knows whether the sync actually completed when the invocation
/// returns. The existing snapshot fallback in the session protocol
/// handles the case where the peer's cursor is behind `change_log.min_seq`,
/// so this also works as a "full pull" when the user has missed batches
/// that have since been GC'd.
#[tauri::command]
pub async fn force_sync_now(
    peers: State<'_, PeerRegistry>,
    db: State<'_, DbPool>,
    in_flight: State<'_, crate::sync::scheduler::DialInFlight>,
    app: AppHandle,
) -> Result<SyncSummary, AppError> {
    let identity = crate::sync::identity::ensure_identity(&db).await?;
    let identity = Arc::new(identity);
    let (handles, dialled) =
        crate::sync::scheduler::dial_all_peers(&peers, &db, &identity, &app, &in_flight).await;
    Ok(crate::sync::scheduler::await_dials(handles, dialled).await)
}

#[tauri::command]
pub async fn record_device_user_grant(
    space_id: String,
    session: State<'_, Session>,
    db: State<'_, DbPool>,
    debounce: State<'_, DebounceSender>,
) -> Result<(), AppError> {
    let user_session = session.require_user_valid(db.inner()).await?;
    let identity = crate::sync::identity::ensure_identity(&db).await?;

    sqlx::query_file!(
        "queries/sync/upsert_device_user_grant.sql",
        space_id,
        identity.device_id,
        user_session.user_id,
        chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
    )
    .execute(db.inner())
    .await
    .map_err(|e| AppError::Db(format!("record_device_user_grant: {e}")))?;

    debounce.notify_mutation();
    Ok(())
}

// ---------------------------------------------------------------------------
// Trusted devices
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SpaceDevice {
    pub space_id: String,
    pub device_id: String,
    pub display_name: String,
    pub paired_at: String,
}

#[tauri::command]
pub async fn list_space_devices(
    session: State<'_, Session>,
    db: State<'_, DbPool>,
) -> Result<Vec<SpaceDevice>, AppError> {
    let active = session.require_valid(db.inner()).await?;
    let rows = sqlx::query_file!("queries/sync/list_space_devices.sql", active.space_id)
        .fetch_all(&*db)
        .await
        .map_err(|e| AppError::Db(format!("list_space_devices: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|r| SpaceDevice {
            space_id: r.space_id,
            device_id: r.device_id,
            display_name: r.display_name,
            paired_at: r.paired_at,
        })
        .collect())
}

#[tauri::command]
pub async fn remove_space_device(
    device_id: String,
    session: State<'_, Session>,
    db: State<'_, DbPool>,
    debounce: State<'_, DebounceSender>,
) -> Result<(), AppError> {
    let active = session.require_valid(db.inner()).await?;

    let local_device_id_key = "device_id";
    let local_device_id =
        sqlx::query_file_scalar!("queries/settings/get_setting.sql", local_device_id_key)
            .fetch_optional(&*db)
            .await
            .map_err(|e| AppError::Db(format!("remove_space_device read device_id: {e}")))?;

    if let Some(ref local_id) = local_device_id
        && local_id == &device_id
    {
        return Err(AppError::InvalidInput(
            "cannot remove this device from itself".into(),
        ));
    }

    let mut tx = db
        .inner()
        .begin()
        .await
        .map_err(|e| AppError::Db(format!("remove_space_device begin: {e}")))?;
    require_owner_tx(&mut tx, &active.space_id, &active.user_id).await?;
    let fingerprint = sqlx::query_file!("queries/sync/get_device_fingerprint.sql", &device_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AppError::Db(format!("remove_space_device fingerprint: {e}")))?
        .ok_or_else(|| AppError::NotFound(format!("device {device_id}")))?
        .fingerprint;
    if fingerprint.is_empty() {
        return Err(AppError::InvalidInput(format!(
            "device {device_id} has no certificate fingerprint"
        )));
    }
    let sync_seq = sqlx::query_file_scalar!("queries/settings/get_setting.sql", "sync_seq")
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| AppError::Db(format!("remove_space_device revision: {e}")))?
        .parse::<i64>()
        .map_err(|e| AppError::Internal(format!("invalid sync_seq: {e}")))?;
    sqlx::query_file!(
        "queries/sync/create_durable_revocation.sql",
        uuid::Uuid::new_v4().simple().to_string(),
        &active.space_id,
        &device_id,
        fingerprint,
        sync_seq + 1,
        &active.user_id
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Db(format!("remove_space_device revocation: {e}")))?;
    sqlx::query_file!(
        "queries/sync/delete_space_device.sql",
        &active.space_id,
        &device_id
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Db(format!("remove_space_device delete: {e}")))?;
    tx.commit()
        .await
        .map_err(|e| AppError::Db(format!("remove_space_device commit: {e}")))?;

    debounce.notify_mutation();
    Ok(())
}

/// Step 30.4: transition an existing (space, device) grant between
/// trust modes ('active' ↔ 'revoking' ↔ 'revocation_only'). The new
/// state is captured in change_log and propagates to peers on their
/// next sync round. Note: this command does NOT evict a device — to
/// revoke access entirely, call `remove_space_device` afterwards (or
/// directly, skipping the revoking intermediate).
#[tauri::command]
pub async fn set_space_device_trust_mode(
    device_id: String,
    trust_mode: String,
    session: State<'_, Session>,
    db: State<'_, DbPool>,
    debounce: State<'_, DebounceSender>,
) -> Result<(), AppError> {
    let active = session.require_valid(db.inner()).await?;

    let mode: TrustMode = trust_mode.parse().map_err(|e: AppError| {
        AppError::InvalidInput(format!("set_space_device_trust_mode: {e}"))
    })?;

    let local_device_id = sqlx::query_file_scalar!("queries/settings/get_setting.sql", "device_id")
        .fetch_optional(&*db)
        .await
        .map_err(|e| AppError::Db(format!("set_space_device_trust_mode read device_id: {e}")))?
        .ok_or_else(|| AppError::Internal("device_id missing from app_settings".into()))?;

    if local_device_id == device_id {
        return Err(AppError::InvalidInput(
            "cannot change trust mode of this device from itself".into(),
        ));
    }

    let mut tx = db
        .inner()
        .begin()
        .await
        .map_err(|e| AppError::Db(format!("set_space_device_trust_mode begin: {e}")))?;
    require_owner_tx(&mut tx, &active.space_id, &active.user_id).await?;
    let updated = sqlx::query_file!(
        "queries/sync/update_space_device_trust_mode.sql",
        active.space_id,
        device_id,
        mode
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Db(format!("set_space_device_trust_mode: {e}")))?;

    if updated.rows_affected() == 0 {
        return Err(AppError::NotFound(format!(
            "no space_devices row for ({}, {})",
            active.space_id, device_id
        )));
    }
    tx.commit()
        .await
        .map_err(|e| AppError::Db(format!("set_space_device_trust_mode commit: {e}")))?;

    debounce.notify_mutation();
    Ok(())
}

// ---------------------------------------------------------------------------
// Pairing
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn generate_pair_token(
    host_display_name: String,
    session: State<'_, Session>,
    db: State<'_, DbPool>,
    pairing: State<'_, Arc<PairingState>>,
    app: AppHandle,
) -> Result<PairToken, AppError> {
    let active = session.require_valid(db.inner()).await?;
    let mut tx = db
        .inner()
        .begin()
        .await
        .map_err(|e| AppError::Db(format!("generate_pair_token begin: {e}")))?;
    require_owner_tx(&mut tx, &active.space_id, &active.user_id).await?;
    tx.commit()
        .await
        .map_err(|e| AppError::Db(format!("generate_pair_token commit: {e}")))?;
    let identity = crate::sync::identity::ensure_identity(&db).await?;

    let snapshot = crate::sync::snapshot::collect_snapshot(
        &db,
        Some(&app),
        &active.space_id,
        identity.device_id,
        host_display_name,
        identity.cert_pem,
    )
    .await?;

    pairing::start_host_session((*db).clone(), pairing.inner().clone(), snapshot, app).await
}

#[tauri::command]
pub async fn accept_pair_token_from_peer(
    peer_device_id: String,
    token: String,
    device_display_name: String,
    session: State<'_, Session>,
    db: State<'_, DbPool>,
    registry: State<'_, PeerRegistry>,
    app: AppHandle,
) -> Result<JoinResult, AppError> {
    let user_session = session.require_user_valid(db.inner()).await?;

    let peer = registry
        .snapshot()
        .into_iter()
        .find(|p| p.device_id == peer_device_id)
        .ok_or_else(|| AppError::NotFound(format!("peer {peer_device_id} not in registry")))?;

    let pairing_port = peer.pairing_port.unwrap_or(PAIRING_PORT);
    let address = format!("{token}|{}:{}", peer.host, pairing_port);

    let user_row = sqlx::query_file!("queries/sync/get_user.sql", user_session.user_id)
        .fetch_optional(&*db)
        .await
        .map_err(|e| AppError::Db(format!("get_user: {e}")))?
        .ok_or_else(|| AppError::NotFound(format!("user {}", user_session.user_id)))?;

    let joining = WireUser {
        id: user_row.id,
        name: user_row.name,
        created_at: user_row.created_at,
        updated_at: user_row.updated_at,
    };

    let result = pairing::run_joiner(
        (*db).clone(),
        address,
        Some(joining),
        device_display_name,
        app,
    )
    .await?;

    Ok(JoinResult {
        space_id: result.space_id,
        space_name: result.space_name,
    })
}

#[derive(Debug, Serialize)]
pub struct JoinResult {
    pub space_id: String,
    pub space_name: String,
}

/// Join a space on a fresh device (no session required). Sends `None` as the
/// joining user so the host does not create a duplicate membership row. The
/// returned `SpaceUsers` lists every user in the space so the frontend can
/// ask "which one are you?" and set a local PIN for that identity.
#[tauri::command]
pub async fn join_space(
    peer_device_id: String,
    token: String,
    device_display_name: String,
    db: State<'_, DbPool>,
    registry: State<'_, PeerRegistry>,
    app: AppHandle,
) -> Result<SpaceUsers, AppError> {
    let peer = registry
        .snapshot()
        .into_iter()
        .find(|p| p.device_id == peer_device_id)
        .ok_or_else(|| AppError::NotFound(format!("peer {peer_device_id} not in registry")))?;

    let pairing_port = peer.pairing_port.unwrap_or(PAIRING_PORT);
    let address = format!("{token}|{}:{}", peer.host, pairing_port);
    let result =
        pairing::run_joiner((*db).clone(), address, None, device_display_name, app).await?;

    Ok(SpaceUsers {
        space_id: result.space_id,
        space_name: result.space_name,
        users: result
            .users
            .into_iter()
            .map(|u| BundleUser {
                id: u.id,
                name: u.name,
            })
            .collect(),
    })
}

#[derive(Debug, Serialize)]
pub struct BundleUser {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct SpaceUsers {
    pub space_id: String,
    pub space_name: String,
    pub users: Vec<BundleUser>,
}

#[tauri::command]
pub async fn accept_pair_token(
    address: String,
    device_display_name: String,
    session: State<'_, Session>,
    db: State<'_, DbPool>,
    app: AppHandle,
) -> Result<JoinResult, AppError> {
    let user_session = session.require_user_valid(db.inner()).await?;

    let user_row = sqlx::query_file!("queries/sync/get_user.sql", user_session.user_id)
        .fetch_optional(&*db)
        .await
        .map_err(|e| AppError::Db(format!("get_user: {e}")))?
        .ok_or_else(|| AppError::NotFound(format!("user {}", user_session.user_id)))?;

    let joining = WireUser {
        id: user_row.id,
        name: user_row.name,
        created_at: user_row.created_at,
        updated_at: user_row.updated_at,
    };

    let result = pairing::run_joiner(
        (*db).clone(),
        address,
        Some(joining),
        device_display_name,
        app,
    )
    .await?;

    Ok(JoinResult {
        space_id: result.space_id,
        space_name: result.space_name,
    })
}

// ---------------------------------------------------------------------------
// Step 30.2 — Quarantined certs
// ---------------------------------------------------------------------------

/// One row in the `quarantined_devices` table, exposed to the UI so
/// the user can see which certs were rejected and why. Populated by
/// `pairing::upsert_device_and_grant` and the snapshot apply path
/// when ingress validation fails.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct QuarantinedDevice {
    pub id: i64,
    pub space_id: Option<String>,
    pub claimed_device_id: String,
    pub fingerprint: Option<String>,
    pub cert_pem: String,
    pub reason: String,
    pub quarantined_at: String,
}

#[tauri::command]
pub async fn list_quarantined_devices(
    db: State<'_, DbPool>,
) -> Result<Vec<QuarantinedDevice>, AppError> {
    let rows = sqlx::query_file!("queries/sync/list_quarantined_devices.sql")
        .fetch_all(&*db)
        .await
        .map_err(|e| AppError::Db(format!("list_quarantined_devices: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|r| QuarantinedDevice {
            id: r.id,
            space_id: r.space_id,
            claimed_device_id: r.claimed_device_id,
            fingerprint: r.fingerprint,
            cert_pem: r.cert_pem,
            reason: r.reason,
            quarantined_at: r.quarantined_at,
        })
        .collect())
}

async fn require_owner_tx(
    tx: &mut Transaction<'_, Sqlite>,
    space_id: &str,
    user_id: &str,
) -> Result<(), AppError> {
    let row = sqlx::query_file!("queries/spaces/get_member_role.sql", space_id, user_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|e| AppError::Db(format!("require_owner_tx: {e}")))?;

    match row {
        None => Err(AppError::Forbidden),
        Some(r) if r.role != "owner" => Err(AppError::Forbidden),
        _ => Ok(()),
    }
}
