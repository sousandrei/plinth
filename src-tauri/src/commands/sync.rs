use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::{
    db::DbPool,
    error::AppError,
    sync::{
        pairing::{
            self, PairToken, PairingState, SpaceBundle, WireMember, WireSpace, WireUser,
            PAIRING_PORT,
        },
        PeerInfo, PeerRegistry,
    },
    Session,
};

// ---------------------------------------------------------------------------
// Device identity
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_device_name() -> String {
    gethostname::gethostname()
        .to_string_lossy()
        .into_owned()
}

// ---------------------------------------------------------------------------
// Peer discovery
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_peers(registry: State<'_, PeerRegistry>) -> Result<Vec<PeerInfo>, AppError> {
    Ok(registry.snapshot())
}

// ---------------------------------------------------------------------------
// Trusted devices
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TrustedDevice {
    pub id: String,
    pub space_id: String,
    pub device_id: String,
    pub display_name: String,
    pub sync_enabled: bool,
    pub paired_at: String,
}

#[tauri::command]
pub async fn list_trusted_devices(
    session: State<'_, Session>,
    db: State<'_, DbPool>,
) -> Result<Vec<TrustedDevice>, AppError> {
    let active = session.require()?;
    let rows = sqlx::query_file!("queries/sync/list_trusted_devices.sql", active.space_id)
        .fetch_all(&*db)
        .await
        .map_err(|e| AppError::Db(format!("list_trusted_devices: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|r| TrustedDevice {
            id: r.id,
            space_id: r.space_id,
            device_id: r.device_id,
            display_name: r.display_name,
            sync_enabled: r.sync_enabled != 0,
            paired_at: r.paired_at,
        })
        .collect())
}

#[tauri::command]
pub async fn remove_trusted_device(
    id: String,
    session: State<'_, Session>,
    db: State<'_, DbPool>,
) -> Result<(), AppError> {
    let active = session.require()?;
    sqlx::query_file!("queries/sync/delete_trusted_device.sql", active.space_id, id)
        .execute(&*db)
        .await
        .map_err(|e| AppError::Db(format!("remove_trusted_device: {e}")))?;
    Ok(())
}

#[tauri::command]
pub async fn set_trusted_device_sync(
    id: String,
    enabled: bool,
    session: State<'_, Session>,
    db: State<'_, DbPool>,
) -> Result<(), AppError> {
    let active = session.require()?;
    let flag: i64 = if enabled { 1 } else { 0 };
    sqlx::query_file!(
        "queries/sync/set_trusted_device_sync.sql",
        active.space_id,
        id,
        flag
    )
    .execute(&*db)
    .await
    .map_err(|e| AppError::Db(format!("set_trusted_device_sync: {e}")))?;
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
) -> Result<PairToken, AppError> {
    let active = session.require()?;

    let space = sqlx::query_file!("queries/sync/get_space.sql", active.space_id)
        .fetch_optional(&*db)
        .await
        .map_err(|e| AppError::Db(format!("get_space: {e}")))?
        .ok_or_else(|| AppError::NotFound(format!("space {}", active.space_id)))?;
    let space = WireSpace {
        id: space.id,
        name: space.name,
        created_at: space.created_at,
        updated_at: space.updated_at,
    };

    let owner_user_row = sqlx::query_file!("queries/sync/get_user.sql", active.user_id)
        .fetch_optional(&*db)
        .await
        .map_err(|e| AppError::Db(format!("get_user: {e}")))?
        .ok_or_else(|| AppError::NotFound(format!("user {}", active.user_id)))?;
    let owner_user = WireUser {
        id: owner_user_row.id,
        name: owner_user_row.name,
        pin_hash: owner_user_row.pin_hash,
        created_at: owner_user_row.created_at,
        updated_at: owner_user_row.updated_at,
    };

    let members = sqlx::query_file!(
        "queries/sync/list_space_members_for_pairing.sql",
        active.space_id,
    )
    .fetch_all(&*db)
    .await
    .map_err(|e| AppError::Db(format!("list members: {e}")))?
    .into_iter()
    .map(|r| WireMember {
        space_id: r.space_id,
        user_id: r.user_id,
        role: r.role,
        joined_at: r.joined_at,
    })
    .collect();

    let member_users = sqlx::query_file!(
        "queries/sync/list_space_member_users.sql",
        active.space_id,
    )
    .fetch_all(&*db)
    .await
    .map_err(|e| AppError::Db(format!("list member users: {e}")))?
    .into_iter()
    .map(|r| WireUser {
        id: r.id,
        name: r.name,
        pin_hash: r.pin_hash,
        created_at: r.created_at,
        updated_at: r.updated_at,
    })
    .collect();

    pairing::start_host_session(
        (*db).clone(),
        pairing.inner().clone(),
        pairing::HostInputs {
            space,
            members,
            member_users,
            owner_user,
            host_display_name,
        },
    )
    .await
}

#[tauri::command]
pub async fn accept_pair_token_from_peer(
    peer_device_id: String,
    token: String,
    device_display_name: String,
    session: State<'_, Session>,
    db: State<'_, DbPool>,
    registry: State<'_, PeerRegistry>,
) -> Result<JoinResult, AppError> {
    let user_session = session.require_user()?;

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
        pin_hash: user_row.pin_hash,
        created_at: user_row.created_at,
        updated_at: user_row.updated_at,
    };

    let bundle: SpaceBundle =
        pairing::run_joiner((*db).clone(), address, Some(joining), device_display_name).await?;

    Ok(JoinResult {
        space_id: bundle.space.id,
        space_name: bundle.space.name,
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
) -> Result<SpaceUsers, AppError> {
    let peer = registry
        .snapshot()
        .into_iter()
        .find(|p| p.device_id == peer_device_id)
        .ok_or_else(|| AppError::NotFound(format!("peer {peer_device_id} not in registry")))?;

    let pairing_port = peer.pairing_port.unwrap_or(PAIRING_PORT);
    let address = format!("{token}|{}:{}", peer.host, pairing_port);
    let bundle = pairing::run_joiner((*db).clone(), address, None, device_display_name).await?;

    Ok(SpaceUsers {
        space_id: bundle.space.id,
        space_name: bundle.space.name,
        users: bundle.users.into_iter().map(|u| BundleUser {
            id: u.id,
            name: u.name,
        }).collect(),
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
) -> Result<JoinResult, AppError> {
    let user_session = session.require_user()?;

    let user_row = sqlx::query_file!("queries/sync/get_user.sql", user_session.user_id)
        .fetch_optional(&*db)
        .await
        .map_err(|e| AppError::Db(format!("get_user: {e}")))?
        .ok_or_else(|| AppError::NotFound(format!("user {}", user_session.user_id)))?;

    let joining = WireUser {
        id: user_row.id,
        name: user_row.name,
        pin_hash: user_row.pin_hash,
        created_at: user_row.created_at,
        updated_at: user_row.updated_at,
    };

    let bundle: SpaceBundle =
        pairing::run_joiner((*db).clone(), address, Some(joining), device_display_name).await?;

    Ok(JoinResult {
        space_id: bundle.space.id,
        space_name: bundle.space.name,
    })
}
