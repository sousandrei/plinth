use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tauri::AppHandle;

use crate::error::AppError;
use crate::sync::apply::apply_tombstone;
use crate::sync::trust_mode::TrustMode;

pub const SNAPSHOT_SCHEMA_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Wire types — one shape per synced table and snapshot section. Used by
// both the pairing transfer (encrypted TCP) and the sync-session snapshot
// transfer (`Frame::Snapshot` chunks). The on-the-wire format is identical
// in both transports.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireOriginState {
    pub origin_device_id: String,
    pub high_water: i64,
    pub retained_floor: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireRowWinner {
    pub space_id: String,
    pub table_name: String,
    pub row_id: String,
    pub winning_seq: i64,
    pub winning_origin: String,
    pub deleted: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireDevice {
    pub device_id: String,
    pub cert_pem: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireSpaceDevice {
    pub space_id: String,
    pub device_id: String,
    pub trust_mode: TrustMode,
    pub paired_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireUser {
    pub id: String,
    pub name: String,
    pub pin_hash: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireSpace {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireMember {
    pub space_id: String,
    pub user_id: String,
    pub role: String,
    pub joined_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireCategory {
    pub id: String,
    pub name: String,
    pub color: String,
    pub space_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireAccount {
    pub id: String,
    pub name: String,
    pub currency: String,
    pub account_type: String,
    pub account_source: String,
    pub color: String,
    pub space_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireTransaction {
    pub id: String,
    pub booking_date: String,
    pub value_date: String,
    pub reference: String,
    pub text: String,
    pub currency: String,
    pub amount: i64,
    pub balance: i64,
    pub approved: i64,
    pub note: String,
    pub category: Option<String>,
    pub account_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireAccountSummary {
    pub month: String,
    pub account_id: String,
    pub balance: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireSpaceSetting {
    pub space_id: String,
    pub key: String,
    pub value: String,
}

/// One row of the mesh-wide `model_versions` registry. Joins the
/// snapshot so the joiner has the canonical MD5s immediately, then
/// receives the file bytes via `Frame::ModelData` in the model-sync
/// phase that follows the snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireModelVersion {
    pub space_id: String,
    pub version: i64,
    pub weights_md5: String,
    pub card_md5: String,
    pub trained_at: String,
}

/// Full snapshot of one space: identity, members, users, every synced
/// table's rows, devices & space grants, high-water vector, row winners/tombstones.
/// Constructed via `collect_snapshot`, applied via `apply_snapshot_frame`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceSnapshot {
    pub snapshot_schema_version: u32,
    pub protocol_version: u16,
    pub snapshot_id: String,
    pub host_device_id: String,
    pub host_device_name: String,
    pub host_cert_pem: String,

    pub high_water_vector: Vec<WireOriginState>,
    pub row_winners: Vec<WireRowWinner>,

    pub space: WireSpace,
    pub members: Vec<WireMember>,
    pub users: Vec<WireUser>,
    pub categories: Vec<WireCategory>,
    pub accounts: Vec<WireAccount>,
    pub transactions: Vec<WireTransaction>,
    pub account_summaries: Vec<WireAccountSummary>,
    pub space_settings: Vec<WireSpaceSetting>,
    pub model_versions: Vec<WireModelVersion>,
    pub devices: Vec<WireDevice>,
    pub space_devices: Vec<WireSpaceDevice>,
}

/// Tagged envelope sent over both the pairing transport (encrypted TCP)
/// and the sync session (`Frame::Snapshot(SnapshotFrame)`). Each variant
/// is independently serialized + framed; the transport layer is responsible
/// for chunking large vectors before encoding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SnapshotFrame {
    Space(WireSpace),
    HighWaterVector(Vec<WireOriginState>),
    RowWinners(Vec<WireRowWinner>),
    Devices(Vec<WireDevice>),
    SpaceDevices(Vec<WireSpaceDevice>),
    Members(Vec<WireMember>),
    Users(Vec<WireUser>),
    Categories(Vec<WireCategory>),
    Accounts(Vec<WireAccount>),
    Transactions(Vec<WireTransaction>),
    AccountSummaries(Vec<WireAccountSummary>),
    SpaceSettings(Vec<WireSpaceSetting>),
    ModelVersions(Vec<WireModelVersion>),
    End,
}

// ---------------------------------------------------------------------------
// Collect: gather a snapshot from the local DB
// ---------------------------------------------------------------------------

/// Gather every row needed to reconstruct `space_id` on a fresh device inside
/// a single read transaction to guarantee snapshot consistency.
/// Includes the active finetuned-model version (read from
/// `space_settings`), so receivers that already have a finetuned model
/// can advance their active version after a successful snapshot.
pub async fn collect_snapshot(
    db: &SqlitePool,
    app: Option<&AppHandle>,
    space_id: &str,
    host_device_id: String,
    host_device_name: String,
    host_cert_pem: String,
) -> Result<SpaceSnapshot, AppError> {
    let mut tx = db
        .begin()
        .await
        .map_err(|e| AppError::Db(format!("collect_snapshot begin tx: {e}")))?;

    let snapshot_id = format!("snap_{}", uuid::Uuid::new_v4().simple());

    let high_water_vector: Vec<WireOriginState> = sqlx::query_file_as!(
        WireOriginState,
        "queries/snapshots/list_origin_states.sql",
        space_id
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| AppError::Db(format!("collect_snapshot origin_states: {e}")))?;

    let row_winners: Vec<WireRowWinner> = sqlx::query_file_as!(
        WireRowWinner,
        "queries/snapshots/list_row_winners.sql",
        space_id
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| AppError::Db(format!("collect_snapshot row_winners: {e}")))?;

    let space = sqlx::query_file_as!(WireSpace, "queries/snapshots/list_space.sql", space_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AppError::Db(format!("collect_snapshot space: {e}")))?
        .ok_or_else(|| AppError::NotFound(format!("space {space_id}")))?;

    let members: Vec<WireMember> =
        sqlx::query_file_as!(WireMember, "queries/snapshots/list_members.sql", space_id)
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| AppError::Db(format!("collect_snapshot members: {e}")))?;

    let mut users: Vec<WireUser> = sqlx::query_file_as!(
        WireUser,
        "queries/snapshots/list_users_for_space.sql",
        space_id
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| AppError::Db(format!("collect_snapshot users: {e}")))?;

    // Step 31.1: Users without local credentials (PIN hash must not leak)
    for u in &mut users {
        u.pin_hash = None;
    }

    let categories: Vec<WireCategory> = sqlx::query_file_as!(
        WireCategory,
        "queries/sync/list_pairing_categories.sql",
        space_id
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| AppError::Db(format!("collect_snapshot categories: {e}")))?;

    let accounts: Vec<WireAccount> = sqlx::query_file_as!(
        WireAccount,
        "queries/sync/list_pairing_accounts.sql",
        space_id
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| AppError::Db(format!("collect_snapshot accounts: {e}")))?;

    let transactions: Vec<WireTransaction> = sqlx::query_file_as!(
        WireTransaction,
        "queries/snapshots/list_transactions.sql",
        space_id
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| AppError::Db(format!("collect_snapshot transactions: {e}")))?;

    let account_summaries: Vec<WireAccountSummary> = sqlx::query_file_as!(
        WireAccountSummary,
        "queries/snapshots/list_account_summaries.sql",
        space_id
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| AppError::Db(format!("collect_snapshot account_summaries: {e}")))?;

    let mut space_settings: Vec<WireSpaceSetting> = sqlx::query_file_as!(
        WireSpaceSetting,
        "queries/sync/list_space_settings.sql",
        space_id
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| AppError::Db(format!("collect_snapshot space_settings: {e}")))?;

    let active_model_version: Option<String> = sqlx::query_file!(
        "queries/training/get_setting.sql",
        space_id,
        "active_model_version"
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Db(format!("collect_snapshot model_version: {e}")))?
    .map(|r| r.value);

    if let Some(v) = active_model_version {
        let already = space_settings
            .iter()
            .any(|s| s.key == "active_model_version");
        if !already {
            space_settings.push(WireSpaceSetting {
                space_id: space_id.to_string(),
                key: "active_model_version".into(),
                value: v,
            });
        }
    }

    let model_versions: Vec<WireModelVersion> = sqlx::query_file_as!(
        WireModelVersion,
        "queries/snapshots/list_model_versions.sql",
        space_id
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| AppError::Db(format!("collect_snapshot model_versions: {e}")))?;

    let devices: Vec<WireDevice> = sqlx::query_file_as!(
        WireDevice,
        "queries/snapshots/list_devices_for_space.sql",
        space_id
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| AppError::Db(format!("collect_snapshot devices: {e}")))?;

    let space_devices_raw = sqlx::query_file!(
        "queries/snapshots/list_space_devices_for_space.sql",
        space_id
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| AppError::Db(format!("collect_snapshot space_devices: {e}")))?;

    let mut space_devices = Vec::with_capacity(space_devices_raw.len());
    for r in space_devices_raw {
        let mode = TrustMode::parse(&r.trust_mode).ok_or_else(|| {
            AppError::Internal(format!(
                "collect_snapshot: unknown trust_mode {:?} for device {}",
                r.trust_mode, r.device_id
            ))
        })?;
        space_devices.push(WireSpaceDevice {
            space_id: r.space_id,
            device_id: r.device_id,
            trust_mode: mode,
            paired_at: r.paired_at,
        });
    }

    tx.commit()
        .await
        .map_err(|e| AppError::Db(format!("collect_snapshot commit: {e}")))?;

    let _ = app; // reserved for future "include model files in snapshot"

    Ok(SpaceSnapshot {
        snapshot_schema_version: SNAPSHOT_SCHEMA_VERSION,
        protocol_version: crate::sync::wire::PROTOCOL_VERSION,
        snapshot_id,
        host_device_id,
        host_device_name,
        host_cert_pem,
        high_water_vector,
        row_winners,
        space,
        members,
        users,
        categories,
        accounts,
        transactions,
        account_summaries,
        space_settings,
        model_versions,
        devices,
        space_devices,
    })
}

// ---------------------------------------------------------------------------
// Boundary: validate the snapshot stream before opening apply_guard
// ---------------------------------------------------------------------------

/// Step 31.2: verify the snapshot boundary before opening the apply
/// transaction. Rejects mixed-space streams and unsupported schema
/// versions, so a hostile or buggy peer cannot poison the receiver's
/// state with a partial apply that passes all per-row checks.
///
/// Returns `Ok(())` on success. The caller MUST abort before any DB
/// write on `Err(_)`.
pub fn verify_snapshot_boundary(
    expected_space_id: &str,
    snapshot_schema_version: u32,
    chunks: &[crate::sync::wire::SnapshotChunk],
) -> Result<(), AppError> {
    if snapshot_schema_version != SNAPSHOT_SCHEMA_VERSION {
        return Err(AppError::InvalidInput(format!(
            "snapshot boundary: unsupported snapshot_schema_version {} (expected {})",
            snapshot_schema_version, SNAPSHOT_SCHEMA_VERSION
        )));
    }

    if expected_space_id.is_empty() {
        return Err(AppError::InvalidInput(
            "snapshot boundary: empty expected space_id".into(),
        ));
    }

    for (i, chunk) in chunks.iter().enumerate() {
        if chunk.space_id != expected_space_id {
            return Err(AppError::InvalidInput(format!(
                "snapshot boundary: chunk {i} has space_id {:?} != expected {:?}",
                chunk.space_id, expected_space_id
            )));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Install: advance cursors after snapshot apply commits
// ---------------------------------------------------------------------------

/// Step 31.2: install the complete cursor vector AFTER the snapshot
/// apply transaction commits. Opens a separate write transaction so a
/// crash between snapshot apply and cursor installation is detectable:
/// the snapshot is on disk but the cursor says "0" — the next sync will
/// request a fresh snapshot, which is correct (idempotent re-apply).
///
/// Each `WireOriginState` becomes a `sync_cursors(space_id, origin)` row
/// advanced to `high_water`. The host is included if it appears in the
/// vector; if not, we still install `sync_cursors(space_id, host) =
/// host_high_water` derived from the vector's max (defensive — the host
/// is by definition one of the origins, but a hostile snapshot claiming
/// otherwise shouldn't trap the receiver in an infinite re-snapshot
/// loop).
///
/// `MAX(...)` clamping inside `upsert_cursor.sql` guarantees the cursor
/// only ever moves forward — concurrent incremental syncs that landed
/// between the snapshot's capture and this install can't be undone.
pub async fn install_snapshot_cursors(
    db: &SqlitePool,
    space_id: &str,
    high_water_vector: &[WireOriginState],
    host_device_id: &str,
) -> Result<(), AppError> {
    let mut tx = db
        .begin()
        .await
        .map_err(|e| AppError::Db(format!("install_snapshot_cursors begin: {e}")))?;

    let mut max_high_water: i64 = 0;
    for os in high_water_vector {
        crate::sync::cursors::advance(&mut tx, space_id, &os.origin_device_id, os.high_water)
            .await
            .map_err(|e| AppError::Db(format!("install_snapshot_cursors: {e}")))?;
        if os.high_water > max_high_water {
            max_high_water = os.high_water;
        }
    }

    // Defensive: if the host isn't in the vector, install it at the
    // max observed high water so the next sync doesn't re-request the
    // whole backlog from seq=1.
    if !high_water_vector
        .iter()
        .any(|os| os.origin_device_id == host_device_id)
    {
        crate::sync::cursors::advance(&mut tx, space_id, host_device_id, max_high_water)
            .await
            .map_err(|e| AppError::Db(format!("install_snapshot_cursors host: {e}")))?;
    }

    tx.commit()
        .await
        .map_err(|e| AppError::Db(format!("install_snapshot_cursors commit: {e}")))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Apply: persist a snapshot inside an open transaction
// ---------------------------------------------------------------------------

/// Apply a single snapshot frame. Used by both pairing (encrypted TCP)
/// and sync-session fallback (`Frame::Snapshot`). The header frame
/// (`SnapshotFrame::Space`) carries the host identity in addition to the
/// space row.
pub async fn apply_snapshot_frame(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    snapshot: &SpaceSnapshot,
    frame: &SnapshotFrame,
) -> Result<(), AppError> {
    match frame {
        SnapshotFrame::Space(s) => {
            upsert_space(tx, s).await?;
        }
        SnapshotFrame::HighWaterVector(chunk) => {
            for os in chunk {
                upsert_origin_state(tx, &snapshot.space.id, os).await?;
            }
        }
        SnapshotFrame::RowWinners(chunk) => {
            for rw in chunk {
                upsert_row_winner(tx, rw).await?;
                // Step 31.2: honor tombstoned winners. The previous
                // chunks may have re-materialized a row that the host
                // has since deleted — apply the tombstone now to keep
                // the receiver's row set in sync with `row_winners`.
                if rw.deleted != 0 {
                    apply_tombstone(tx, &rw.table_name, &rw.row_id).await?;
                }
            }
        }
        SnapshotFrame::Devices(chunk) => {
            for d in chunk {
                upsert_device(tx, d).await?;
            }
        }
        SnapshotFrame::SpaceDevices(chunk) => {
            for sd in chunk {
                upsert_space_device(tx, sd).await?;
            }
        }
        SnapshotFrame::Members(chunk) => {
            for m in chunk {
                upsert_space_member(tx, m).await?;
            }
        }
        SnapshotFrame::Users(chunk) => {
            for u in chunk {
                upsert_user(tx, u).await?;
            }
        }
        SnapshotFrame::Categories(chunk) => {
            for c in chunk {
                upsert_category(tx, c).await?;
            }
        }
        SnapshotFrame::Accounts(chunk) => {
            for a in chunk {
                upsert_account(tx, a).await?;
            }
        }
        SnapshotFrame::Transactions(chunk) => {
            for t in chunk {
                upsert_transaction(tx, t).await?;
            }
        }
        SnapshotFrame::AccountSummaries(chunk) => {
            for s in chunk {
                upsert_account_summary(tx, s).await?;
            }
        }
        SnapshotFrame::SpaceSettings(chunk) => {
            for s in chunk {
                upsert_space_setting(tx, s).await?;
            }
        }
        SnapshotFrame::ModelVersions(chunk) => {
            for m in chunk {
                upsert_model_version(tx, m).await?;
            }
        }
        SnapshotFrame::End => {
            upsert_device_and_grant(
                tx,
                &snapshot.space.id,
                &snapshot.host_device_id,
                &snapshot.host_device_name,
                &snapshot.host_cert_pem,
            )
            .await?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Per-table upserts (one row at a time, in a transaction). Mirrors the
// apply/* SQL files used during normal sync — same conflict policy, same
// `ON CONFLICT` clauses.
// ---------------------------------------------------------------------------

async fn upsert_origin_state(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    space_id: &str,
    os: &WireOriginState,
) -> Result<(), AppError> {
    sqlx::query_file!(
        "queries/sync/apply/upsert_origin_state.sql",
        space_id,
        os.origin_device_id,
        os.high_water,
        os.retained_floor
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Db(format!("upsert_origin_state: {e}")))?;
    Ok(())
}

async fn upsert_row_winner(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    rw: &WireRowWinner,
) -> Result<(), AppError> {
    sqlx::query_file!(
        "queries/sync/upsert_row_winner.sql",
        rw.space_id,
        rw.table_name,
        rw.row_id,
        rw.winning_seq,
        rw.winning_origin,
        rw.deleted
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Db(format!("upsert_row_winner: {e}")))?;
    Ok(())
}

async fn upsert_device(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    d: &WireDevice,
) -> Result<(), AppError> {
    let (der, fp) = match crate::sync::cert_validation::parse_and_canonicalize(&d.cert_pem) {
        Ok(v) => (Some(v.der), v.fingerprint),
        Err(_) => (None, String::new()),
    };
    sqlx::query_file!(
        "queries/sync/upsert_device.sql",
        d.device_id,
        d.cert_pem,
        der,
        fp,
        d.display_name
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Db(format!("upsert_device: {e}")))?;
    Ok(())
}

async fn upsert_space_device(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    sd: &WireSpaceDevice,
) -> Result<(), AppError> {
    sqlx::query_file!(
        "queries/sync/apply/upsert_space_device.sql",
        sd.space_id,
        sd.device_id,
        sd.trust_mode,
        sd.paired_at
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Db(format!("upsert_space_device: {e}")))?;
    Ok(())
}

async fn upsert_user(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    u: &WireUser,
) -> Result<(), AppError> {
    sqlx::query_file!(
        "queries/sync/apply/upsert_user.sql",
        u.id,
        u.name,
        u.pin_hash,
        u.created_at,
        u.updated_at
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Db(format!("upsert_user: {e}")))?;
    Ok(())
}

async fn upsert_space(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    s: &WireSpace,
) -> Result<(), AppError> {
    sqlx::query_file!(
        "queries/sync/apply/upsert_space.sql",
        s.id,
        s.name,
        s.created_at,
        s.updated_at
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Db(format!("upsert_space: {e}")))?;
    Ok(())
}

async fn upsert_space_member(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    m: &WireMember,
) -> Result<(), AppError> {
    sqlx::query_file!(
        "queries/sync/apply/upsert_space_member.sql",
        m.space_id,
        m.user_id,
        m.role,
        m.joined_at
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Db(format!("upsert_space_member: {e}")))?;
    Ok(())
}

async fn upsert_device_and_grant(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    space_id: &str,
    device_id: &str,
    display_name: &str,
    cert_pem: &str,
) -> Result<(), AppError> {
    // Step 30.2: validate the host's cert before persisting. Same
    // invariants as `pairing::upsert_device_and_grant`. The host's
    // cert is the trust anchor for every subsequent change in the
    // snapshot, so it must be rejected on any inconsistency.
    let validated = crate::sync::cert_validation::parse_and_canonicalize(cert_pem)
        .map_err(|e| AppError::InvalidInput(format!("snapshot host cert: {e}")))?;
    crate::sync::cert_validation::check_device_id_match(&validated, device_id)
        .map_err(|e| AppError::InvalidInput(format!("snapshot host cert: {e}")))?;

    // Fingerprint / device_id uniqueness against the in-tx devices
    // table. We can't easily look at *committed* state from inside a
    // transaction, but since the snapshot path always runs against
    // a fresh joiner DB the in-tx view is the only view that matters.
    let by_fp = sqlx::query_file!(
        "queries/sync/get_device_id_by_fingerprint.sql",
        validated.fingerprint.clone()
    )
    .fetch_optional(&mut **tx)
    .await
    .map_err(|e| AppError::Db(format!("snapshot fingerprint lookup: {e}")))?;
    if let Some(row) = by_fp
        && row.device_id != device_id
    {
        return Err(AppError::InvalidInput(format!(
            "snapshot host cert: fingerprint already mapped to {}",
            row.device_id
        )));
    }
    let by_id = sqlx::query_file!("queries/sync/get_fingerprint_by_device_id.sql", device_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|e| AppError::Db(format!("snapshot device lookup: {e}")))?;
    if let Some(row) = by_id {
        let existing = row.fingerprint;
        if !existing.is_empty() && existing != validated.fingerprint {
            return Err(AppError::InvalidInput(format!(
                "snapshot host cert: device_id {device_id} already has a different fingerprint"
            )));
        }
    }

    sqlx::query_file!(
        "queries/sync/upsert_device.sql",
        device_id,
        cert_pem,
        validated.der,
        validated.fingerprint,
        display_name
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Db(format!("upsert_device: {e}")))?;

    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    sqlx::query_file!(
        "queries/sync/upsert_space_device.sql",
        space_id,
        device_id,
        crate::sync::trust_mode::TrustMode::Active,
        now
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Db(format!("upsert_space_device: {e}")))?;
    Ok(())
}

async fn upsert_category(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    c: &WireCategory,
) -> Result<(), AppError> {
    sqlx::query_file!(
        "queries/sync/apply/upsert_category.sql",
        c.id,
        c.name,
        c.color,
        c.space_id
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Db(format!("upsert_category: {e}")))?;
    Ok(())
}

async fn upsert_account(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    a: &WireAccount,
) -> Result<(), AppError> {
    sqlx::query_file!(
        "queries/sync/apply/upsert_account.sql",
        a.id,
        a.name,
        a.currency,
        a.account_type,
        a.account_source,
        a.color,
        a.space_id
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Db(format!("upsert_account: {e}")))?;
    Ok(())
}

async fn upsert_transaction(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    t: &WireTransaction,
) -> Result<(), AppError> {
    let category = t.category.as_deref().unwrap_or("");
    sqlx::query_file!(
        "queries/sync/apply/upsert_transaction.sql",
        t.id,
        t.booking_date,
        t.value_date,
        t.reference,
        t.text,
        t.currency,
        t.amount,
        t.balance,
        t.approved,
        t.note,
        category,
        t.account_id
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Db(format!("upsert_transaction: {e}")))?;
    Ok(())
}

async fn upsert_account_summary(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    s: &WireAccountSummary,
) -> Result<(), AppError> {
    sqlx::query_file!(
        "queries/sync/apply/upsert_account_summary.sql",
        s.month,
        s.account_id,
        s.balance
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Db(format!("upsert_account_summary: {e}")))?;
    Ok(())
}

async fn upsert_space_setting(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    s: &WireSpaceSetting,
) -> Result<(), AppError> {
    sqlx::query_file!(
        "queries/sync/apply/upsert_space_setting.sql",
        s.space_id,
        s.key,
        s.value
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Db(format!("upsert_space_setting: {e}")))?;
    Ok(())
}

async fn upsert_model_version(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    m: &WireModelVersion,
) -> Result<(), AppError> {
    sqlx::query_file!(
        "queries/sync/apply/upsert_model_version.sql",
        m.space_id,
        m.version,
        m.weights_md5,
        m.card_md5,
        m.trained_at
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Db(format!("upsert_model_version snapshot: {e}")))?;
    Ok(())
}
