use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tauri::AppHandle;

use crate::error::AppError;

// ---------------------------------------------------------------------------
// Wire types — one shape per synced table. Used by both the pairing
// transfer (encrypted TCP) and the sync-session snapshot transfer
// (`Frame::Snapshot` chunks). The on-the-wire format is identical in both
// transports.
// ---------------------------------------------------------------------------

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
/// table's rows, and the host's device identity (for `trusted_devices`).
/// Constructed via `collect_snapshot`, applied via `apply_snapshot`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceSnapshot {
    pub space: WireSpace,
    pub members: Vec<WireMember>,
    pub users: Vec<WireUser>,
    pub categories: Vec<WireCategory>,
    pub accounts: Vec<WireAccount>,
    pub transactions: Vec<WireTransaction>,
    pub account_summaries: Vec<WireAccountSummary>,
    pub space_settings: Vec<WireSpaceSetting>,
    pub model_versions: Vec<WireModelVersion>,
    pub host_device_id: String,
    pub host_device_name: String,
    pub host_cert_pem: String,
}

/// Tagged envelope sent over both the pairing transport (encrypted TCP)
/// and the sync session (`Frame::Snapshot(SnapshotFrame)`). Each variant
/// is independently serialized + framed; the transport layer is responsible
/// for chunking large vectors before encoding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SnapshotFrame {
    Space(WireSpace),
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

/// Gather every row needed to reconstruct `space_id` on a fresh device.
/// Includes the active finetuned-model version (read from
/// `space_settings`), so receivers that already have a finetuned model
/// can advance their active version after a successful snapshot.
pub async fn collect_snapshot(
    db: &SqlitePool,
    app: &AppHandle,
    space_id: &str,
    host_device_id: String,
    host_device_name: String,
    host_cert_pem: String,
) -> Result<SpaceSnapshot, AppError> {
    let space = sqlx::query_file_as!(WireSpace, "queries/snapshots/list_space.sql", space_id)
        .fetch_optional(db)
        .await
        .map_err(|e| AppError::Db(format!("collect_snapshot space: {e}")))?
        .ok_or_else(|| AppError::NotFound(format!("space {space_id}")))?;

    let members: Vec<WireMember> =
        sqlx::query_file_as!(WireMember, "queries/snapshots/list_members.sql", space_id)
            .fetch_all(db)
            .await
            .map_err(|e| AppError::Db(format!("collect_snapshot members: {e}")))?;

    let users: Vec<WireUser> = sqlx::query_file_as!(
        WireUser,
        "queries/snapshots/list_users_for_space.sql",
        space_id
    )
    .fetch_all(db)
    .await
    .map_err(|e| AppError::Db(format!("collect_snapshot users: {e}")))?;

    let categories: Vec<WireCategory> = sqlx::query_file_as!(
        WireCategory,
        "queries/sync/list_pairing_categories.sql",
        space_id
    )
    .fetch_all(db)
    .await
    .map_err(|e| AppError::Db(format!("collect_snapshot categories: {e}")))?;

    let accounts: Vec<WireAccount> = sqlx::query_file_as!(
        WireAccount,
        "queries/sync/list_pairing_accounts.sql",
        space_id
    )
    .fetch_all(db)
    .await
    .map_err(|e| AppError::Db(format!("collect_snapshot accounts: {e}")))?;

    let transactions: Vec<WireTransaction> = sqlx::query_file_as!(
        WireTransaction,
        "queries/snapshots/list_transactions.sql",
        space_id
    )
    .fetch_all(db)
    .await
    .map_err(|e| AppError::Db(format!("collect_snapshot transactions: {e}")))?;

    let account_summaries: Vec<WireAccountSummary> = sqlx::query_file_as!(
        WireAccountSummary,
        "queries/snapshots/list_account_summaries.sql",
        space_id
    )
    .fetch_all(db)
    .await
    .map_err(|e| AppError::Db(format!("collect_snapshot account_summaries: {e}")))?;

    let mut space_settings: Vec<WireSpaceSetting> = sqlx::query_file_as!(
        WireSpaceSetting,
        "queries/sync/list_space_settings.sql",
        space_id
    )
    .fetch_all(db)
    .await
    .map_err(|e| AppError::Db(format!("collect_snapshot space_settings: {e}")))?;

    // Make sure the active model version is included in the snapshot even
    // if no other settings exist for this space — a fresh joiner that
    // hasn't trained yet still needs to know the host's version so it can
    // pull the weights in the model-sync phase.
    let active_model_version: Option<String> = sqlx::query_file!(
        "queries/training/get_setting.sql",
        space_id,
        "active_model_version"
    )
    .fetch_optional(db)
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
    .fetch_all(db)
    .await
    .map_err(|e| AppError::Db(format!("collect_snapshot model_versions: {e}")))?;

    let _ = app; // reserved for future "include model files in snapshot"

    Ok(SpaceSnapshot {
        space,
        members,
        users,
        categories,
        accounts,
        transactions,
        account_summaries,
        space_settings,
        model_versions,
        host_device_id,
        host_device_name,
        host_cert_pem,
    })
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
            upsert_trusted_device(
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

async fn upsert_trusted_device(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    space_id: &str,
    device_id: &str,
    display_name: &str,
    cert_pem: &str,
) -> Result<(), AppError> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    sqlx::query_file!(
        "queries/sync/apply/upsert_trusted_device.sql",
        id,
        space_id,
        device_id,
        display_name,
        cert_pem,
        1_i64,
        now
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Db(format!("upsert_trusted_device: {e}")))?;
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
