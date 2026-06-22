use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::{
    Session,
    db::DbPool,
    error::AppError,
    sync::{
        PeerInfo, PeerRegistry,
        pairing::{
            self, PAIRING_PORT, PairToken, PairingState, WireAccount, WireAccountSummary,
            WireCategory, WireMember, WireSpace, WireSpaceSetting, WireTransaction, WireUser,
        },
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
/// the scheduler's 30s polling interval. The existing snapshot fallback
/// in the session protocol handles the case where the peer's cursor is
/// behind `change_log.min_seq`, so this also works as a "full pull"
/// when the user has missed batches that have since been GC'd.
#[tauri::command]
pub async fn force_sync_now(
    peers: State<'_, PeerRegistry>,
    db: State<'_, DbPool>,
    in_flight: State<'_, crate::sync::scheduler::DialInFlight>,
    app: AppHandle,
) -> Result<u64, AppError> {
    let identity = crate::sync::identity::ensure_identity(&db).await?;
    let identity = Arc::new(identity);
    let peer_count = peers.snapshot().len() as u64;
    crate::sync::scheduler::dial_all_peers(&peers, &db, &identity, &app, &in_flight).await;
    Ok(peer_count)
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

    let local_device_id_key = "device_id";
    let local_device_id =
        sqlx::query_file_scalar!("queries/settings/get_setting.sql", local_device_id_key)
            .fetch_optional(&*db)
            .await
            .map_err(|e| AppError::Db(format!("remove_trusted_device read device_id: {e}")))?;

    let target = sqlx::query!(
        "SELECT device_id, cert_pem FROM trusted_devices WHERE space_id = ?1 AND id = ?2",
        active.space_id,
        id
    )
    .fetch_optional(&*db)
    .await
    .map_err(|e| AppError::Db(format!("remove_trusted_device fetch target: {e}")))?;

    if let Some(ref t) = target {
        if let Some(ref local_id) = local_device_id
            && local_id == &t.device_id
        {
            return Err(AppError::InvalidInput(
                "cannot remove this device from itself".into(),
            ));
        }

        // Tombstone in evicted_devices
        sqlx::query_file!(
            "queries/sync/insert_evicted_device.sql",
            active.space_id,
            t.device_id,
            t.cert_pem
        )
        .execute(&*db)
        .await
        .map_err(|e| AppError::Db(format!("remove_trusted_device insert evicted: {e}")))?;
    }

    // Delete from trusted_devices (triggers delete change_log row)
    sqlx::query_file!(
        "queries/sync/delete_trusted_device.sql",
        active.space_id,
        id
    )
    .execute(&*db)
    .await
    .map_err(|e| AppError::Db(format!("remove_trusted_device delete: {e}")))?;

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

    let member_users =
        sqlx::query_file!("queries/sync/list_space_member_users.sql", active.space_id,)
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

    let categories = sqlx::query_file!("queries/sync/list_pairing_categories.sql", active.space_id)
        .fetch_all(&*db)
        .await
        .map_err(|e| AppError::Db(format!("list pairing categories: {e}")))?
        .into_iter()
        .map(|r| WireCategory {
            id: r.id,
            name: r.name,
            color: r.color,
            space_id: r.space_id,
        })
        .collect();

    let accounts = sqlx::query_file!("queries/sync/list_pairing_accounts.sql", active.space_id)
        .fetch_all(&*db)
        .await
        .map_err(|e| AppError::Db(format!("list pairing accounts: {e}")))?
        .into_iter()
        .map(|r| WireAccount {
            id: r.id,
            name: r.name,
            currency: r.currency,
            account_type: r.account_type,
            account_source: r.account_source,
            color: r.color,
            space_id: r.space_id,
        })
        .collect();

    let transactions = sqlx::query_file!("queries/spaces/export_transactions.sql", active.space_id)
        .fetch_all(&*db)
        .await
        .map_err(|e| AppError::Db(format!("list pairing transactions: {e}")))?
        .into_iter()
        .map(|r| WireTransaction {
            id: r.id,
            booking_date: r.booking_date,
            value_date: r.value_date,
            reference: r.reference,
            text: r.text,
            currency: r.currency,
            amount: r.amount,
            balance: r.balance,
            approved: r.approved,
            note: r.note,
            category: if r.category.is_empty() {
                None
            } else {
                Some(r.category)
            },
            account_id: r.account_id,
        })
        .collect();

    let account_summaries = sqlx::query_file!(
        "queries/spaces/export_account_summaries.sql",
        active.space_id
    )
    .fetch_all(&*db)
    .await
    .map_err(|e| AppError::Db(format!("list pairing account summaries: {e}")))?
    .into_iter()
    .map(|r| WireAccountSummary {
        month: r.month,
        account_id: r.account_id,
        balance: r.balance,
    })
    .collect();

    let space_settings = sqlx::query_file!("queries/sync/list_space_settings.sql", active.space_id)
        .fetch_all(&*db)
        .await
        .map_err(|e| AppError::Db(format!("list pairing space settings: {e}")))?
        .into_iter()
        .map(|r| WireSpaceSetting {
            space_id: r.space_id,
            key: r.key,
            value: r.value,
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
            categories,
            accounts,
            transactions,
            account_summaries,
            space_settings,
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

    let result =
        pairing::run_joiner((*db).clone(), address, Some(joining), device_display_name).await?;

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
) -> Result<SpaceUsers, AppError> {
    let peer = registry
        .snapshot()
        .into_iter()
        .find(|p| p.device_id == peer_device_id)
        .ok_or_else(|| AppError::NotFound(format!("peer {peer_device_id} not in registry")))?;

    let pairing_port = peer.pairing_port.unwrap_or(PAIRING_PORT);
    let address = format!("{token}|{}:{}", peer.host, pairing_port);
    let result = pairing::run_joiner((*db).clone(), address, None, device_display_name).await?;

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

    let result =
        pairing::run_joiner((*db).clone(), address, Some(joining), device_display_name).await?;

    Ok(JoinResult {
        space_id: result.space_id,
        space_name: result.space_name,
    })
}
