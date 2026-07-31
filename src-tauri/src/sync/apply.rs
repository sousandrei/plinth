use sqlx::{Sqlite, Transaction};
use uuid::Uuid;

use crate::error::AppError;
use crate::sync::conflict_detector::{self, HotFieldConflict, LocalSnapshot};
use crate::sync::payloads::{
    AccountPayload, AccountSummaryPayload, CategoryPayload, DeviceUserGrantPayload,
    DurableRevocationPayload, ModelVersionPayload, SpaceDevicePayload, SpaceMemberPayload,
    SpacePayload, SpaceSettingPayload, TablePayload, TransactionPayload, UserSnapshot,
};
use crate::sync::wire::ChangeRow;

/// Apply one `ChangeRow` to local SQLite. Must be called inside a
/// transaction opened by `apply_guard::run_as_device` so the
/// `applying_as_device` override is in scope — without it, the row
/// would be re-stamped with this device's id and echo back to the
/// network on the next session.
///
/// LWW adjudication for the row body lives in the SQL upsert (we
/// always overwrite). Hot-field conflicts on `transactions.category`
/// and `transactions.note` are detected and recorded in
/// `sync_conflicts` for the user to resolve, but the LWW upsert
/// still runs — the recorded conflict is the audit trail, not a
/// veto. See `data/PLAN.md §7.1`.
pub async fn apply_change(
    tx: &mut Transaction<'_, Sqlite>,
    row: &ChangeRow,
) -> Result<(), AppError> {
    match row.operation.as_str() {
        "insert" | "update" => apply_upsert(tx, row).await,
        "delete" => apply_delete(tx, row).await,
        other => Err(AppError::InvalidInput(format!(
            "apply_change: unknown operation {other:?} for table {}",
            row.table_name
        ))),
    }
}

async fn apply_upsert(tx: &mut Transaction<'_, Sqlite>, row: &ChangeRow) -> Result<(), AppError> {
    let Some(payload) = &row.payload else {
        return Err(AppError::InvalidInput(format!(
            "apply_upsert: missing payload for {}/{}",
            row.table_name, row.row_id
        )));
    };

    // Defense in depth: payload variant must match the row's declared
    // table_name. A mismatch indicates a deserialization bug or a
    // hostile peer; either way, refusing is the conservative move.
    if payload.as_table_name() != row.table_name {
        return Err(AppError::InvalidInput(format!(
            "apply_upsert: payload variant {} mismatches table_name {}",
            payload.as_table_name(),
            row.table_name
        )));
    }

    match payload {
        TablePayload::Space(p) => upsert_space(tx, p).await,
        TablePayload::SpaceMember(p) => upsert_space_member(tx, p).await,
        TablePayload::Account(p) => upsert_account(tx, p).await,
        TablePayload::Category(p) => upsert_category(tx, p).await,
        TablePayload::Transaction(p) => upsert_transaction(tx, row, p).await,
        TablePayload::AccountSummary(p) => upsert_account_summary(tx, p).await,
        TablePayload::SpaceSetting(p) => upsert_space_setting(tx, p).await,
        TablePayload::SpaceDevice(p) => upsert_space_device(tx, row, p).await,
        TablePayload::DeviceUserGrant(p) => upsert_device_user_grant(tx, p).await,
        TablePayload::DurableRevocation(p) => upsert_durable_revocation(tx, p).await,
        TablePayload::ModelVersion(p) => upsert_model_version(tx, p).await,
    }
}

async fn apply_delete(tx: &mut Transaction<'_, Sqlite>, row: &ChangeRow) -> Result<(), AppError> {
    // Deletes carry only the row identity; composite-PK tables encode
    // both key parts in `row_id` as "a|b". This mirrors what the
    // change_log triggers emit for those tables.
    match row.table_name.as_str() {
        "spaces" => {
            sqlx::query_file!("queries/sync/apply/soft_delete_space.sql", row.row_id)
                .execute(&mut **tx)
                .await
                .map_err(|e| AppError::Db(format!("soft_delete_space: {e}")))?;
        }
        "space_members" => {
            // Trigger emits row_id as `space_id:user_id` (see
            // `change_log_space_members_ad` in the migration).
            let (space_id, user_id) = split_composite(&row.row_id, "space_members")?;
            sqlx::query_file!(
                "queries/sync/apply/delete_space_member.sql",
                space_id,
                user_id
            )
            .execute(&mut **tx)
            .await
            .map_err(|e| AppError::Db(format!("delete_space_member: {e}")))?;
        }
        "accounts" => {
            sqlx::query_file!("queries/sync/apply/delete_account.sql", row.row_id)
                .execute(&mut **tx)
                .await
                .map_err(|e| AppError::Db(format!("delete_account: {e}")))?;
        }
        "categories" => {
            sqlx::query_file!("queries/sync/apply/delete_category.sql", row.row_id)
                .execute(&mut **tx)
                .await
                .map_err(|e| AppError::Db(format!("delete_category: {e}")))?;
        }
        "transactions" => {
            sqlx::query_file!("queries/sync/apply/delete_transaction.sql", row.row_id)
                .execute(&mut **tx)
                .await
                .map_err(|e| AppError::Db(format!("delete_transaction: {e}")))?;
        }
        "account_summaries" => {
            // Trigger emits row_id as `account_id:month` (see
            // `change_log_account_summaries_ad` in the migration).
            let (account_id, month) = split_composite(&row.row_id, "account_summaries")?;
            sqlx::query_file!(
                "queries/sync/apply/delete_account_summary.sql",
                month,
                account_id
            )
            .execute(&mut **tx)
            .await
            .map_err(|e| AppError::Db(format!("delete_account_summary: {e}")))?;
        }
        "space_settings" => {
            // Trigger emits row_id as `space_id:key` (see
            // `change_log_space_settings_ad` in the migration).
            let (space_id, key) = split_composite(&row.row_id, "space_settings")?;
            sqlx::query_file!("queries/sync/apply/delete_space_setting.sql", space_id, key)
                .execute(&mut **tx)
                .await
                .map_err(|e| AppError::Db(format!("delete_space_setting: {e}")))?;
        }
        "space_devices" => {
            let (space_id, device_id) = split_composite(&row.row_id, "space_devices")?;
            sqlx::query_file!(
                "queries/sync/apply/delete_space_device.sql",
                space_id,
                device_id
            )
            .execute(&mut **tx)
            .await
            .map_err(|e| AppError::Db(format!("delete_space_device: {e}")))?;
        }
        "device_user_grants" => {
            let (space_id, device_id, user_id) =
                split_triple_composite(&row.row_id, "device_user_grants")?;
            sqlx::query_file!(
                "queries/sync/apply/delete_device_user_grant.sql",
                space_id,
                device_id,
                user_id
            )
            .execute(&mut **tx)
            .await
            .map_err(|e| AppError::Db(format!("delete_device_user_grant: {e}")))?;
        }
        "durable_revocations" => {
            sqlx::query_file!(
                "queries/sync/apply/delete_durable_revocation.sql",
                row.row_id
            )
            .execute(&mut **tx)
            .await
            .map_err(|e| AppError::Db(format!("delete_durable_revocation: {e}")))?;
        }
        "model_versions" => {
            // Trigger emits row_id as `space_id:version` (see
            // `change_log_model_versions_ad` in `0003_model_versions.sql`).
            // Files for that version are intentionally NOT deleted here —
            // the model-sync phase GC handles filesystem cleanup so the
            // apply path stays free of `AppHandle`.
            let (space_id, version) = split_composite(&row.row_id, "model_versions")?;
            sqlx::query_file!(
                "queries/sync/apply/delete_model_version.sql",
                space_id,
                version
            )
            .execute(&mut **tx)
            .await
            .map_err(|e| AppError::Db(format!("delete_model_version: {e}")))?;
        }
        other => {
            return Err(AppError::InvalidInput(format!(
                "apply_delete: unknown table {other}"
            )));
        }
    }
    Ok(())
}

/// Delete a tombstoned row from its source table during snapshot apply.
/// Dispatch is identical to `apply_delete` but takes the row identity
/// directly (no `ChangeRow` wrapping). Used by the snapshot merge path
/// after `row_winners` with `deleted = 1` are installed — the row
/// materialization step may have re-created a previously-deleted row,
/// so this pass is required to clean up stale state on the receiver.
///
/// Like `apply_delete`, this function is idempotent on non-existent rows
/// and is safe to call inside `apply_guard::run_as_device` (triggers are
/// suppressed, so no echo change_log entries are written).
pub async fn apply_tombstone(
    tx: &mut Transaction<'_, Sqlite>,
    table_name: &str,
    row_id: &str,
) -> Result<(), AppError> {
    match table_name {
        "spaces" => {
            sqlx::query_file!("queries/sync/apply/soft_delete_space.sql", row_id)
                .execute(&mut **tx)
                .await
                .map_err(|e| AppError::Db(format!("tombstone spaces: {e}")))?;
        }
        "space_members" => {
            let (space_id, user_id) = split_composite(row_id, "space_members")?;
            sqlx::query_file!(
                "queries/sync/apply/delete_space_member.sql",
                space_id,
                user_id
            )
            .execute(&mut **tx)
            .await
            .map_err(|e| AppError::Db(format!("tombstone space_members: {e}")))?;
        }
        "accounts" => {
            sqlx::query_file!("queries/sync/apply/delete_account.sql", row_id)
                .execute(&mut **tx)
                .await
                .map_err(|e| AppError::Db(format!("tombstone accounts: {e}")))?;
        }
        "categories" => {
            sqlx::query_file!("queries/sync/apply/delete_category.sql", row_id)
                .execute(&mut **tx)
                .await
                .map_err(|e| AppError::Db(format!("tombstone categories: {e}")))?;
        }
        "transactions" => {
            sqlx::query_file!("queries/sync/apply/delete_transaction.sql", row_id)
                .execute(&mut **tx)
                .await
                .map_err(|e| AppError::Db(format!("tombstone transactions: {e}")))?;
        }
        "account_summaries" => {
            let (account_id, month) = split_composite(row_id, "account_summaries")?;
            sqlx::query_file!(
                "queries/sync/apply/delete_account_summary.sql",
                month,
                account_id
            )
            .execute(&mut **tx)
            .await
            .map_err(|e| AppError::Db(format!("tombstone account_summaries: {e}")))?;
        }
        "space_settings" => {
            let (space_id, key) = split_composite(row_id, "space_settings")?;
            sqlx::query_file!("queries/sync/apply/delete_space_setting.sql", space_id, key)
                .execute(&mut **tx)
                .await
                .map_err(|e| AppError::Db(format!("tombstone space_settings: {e}")))?;
        }
        "space_devices" => {
            let (space_id, device_id) = split_composite(row_id, "space_devices")?;
            sqlx::query_file!(
                "queries/sync/apply/delete_space_device.sql",
                space_id,
                device_id
            )
            .execute(&mut **tx)
            .await
            .map_err(|e| AppError::Db(format!("tombstone space_devices: {e}")))?;
        }
        "device_user_grants" => {
            let (space_id, device_id, user_id) =
                split_triple_composite(row_id, "device_user_grants")?;
            sqlx::query_file!(
                "queries/sync/apply/delete_device_user_grant.sql",
                space_id,
                device_id,
                user_id
            )
            .execute(&mut **tx)
            .await
            .map_err(|e| AppError::Db(format!("tombstone device_user_grants: {e}")))?;
        }
        "durable_revocations" => {
            sqlx::query_file!("queries/sync/apply/delete_durable_revocation.sql", row_id)
                .execute(&mut **tx)
                .await
                .map_err(|e| AppError::Db(format!("tombstone durable_revocations: {e}")))?;
        }
        "model_versions" => {
            let (space_id, version) = split_composite(row_id, "model_versions")?;
            sqlx::query_file!(
                "queries/sync/apply/delete_model_version.sql",
                space_id,
                version
            )
            .execute(&mut **tx)
            .await
            .map_err(|e| AppError::Db(format!("tombstone model_versions: {e}")))?;
        }
        other => {
            return Err(AppError::InvalidInput(format!(
                "apply_tombstone: unknown table {other}"
            )));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Per-table upserts. Each one is a thin wrapper around the corresponding
// `queries/sync/apply/upsert_*.sql` file. Kept as separate functions to
// give every table an obvious grep target and to localize binding
// arguments (which differ in count and order).
// ---------------------------------------------------------------------------

async fn upsert_space(tx: &mut Transaction<'_, Sqlite>, p: &SpacePayload) -> Result<(), AppError> {
    sqlx::query_file!(
        "queries/sync/apply/upsert_space.sql",
        p.id,
        p.name,
        p.created_at,
        p.updated_at
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Db(format!("upsert_space: {e}")))?;
    Ok(())
}

async fn upsert_space_member(
    tx: &mut Transaction<'_, Sqlite>,
    p: &SpaceMemberPayload,
) -> Result<(), AppError> {
    // Embedded user rides along with every space_members insert/update
    // so a newly added member becomes loginable on this peer. See
    // `data/PLAN.md §4`. Apply the user first to satisfy the FK on
    // space_members.user_id.
    if let Some(user) = &p.user {
        upsert_embedded_user(tx, user).await?;
    }
    sqlx::query_file!(
        "queries/sync/apply/upsert_space_member.sql",
        p.space_id,
        p.user_id,
        p.role,
        p.joined_at
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Db(format!("upsert_space_member: {e}")))?;
    Ok(())
}

async fn upsert_embedded_user(
    tx: &mut Transaction<'_, Sqlite>,
    u: &UserSnapshot,
) -> Result<(), AppError> {
    sqlx::query_file!(
        "queries/sync/apply/upsert_user.sql",
        u.id,
        u.name,
        u.created_at,
        u.updated_at
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Db(format!("upsert_user: {e}")))?;
    Ok(())
}

async fn upsert_account(
    tx: &mut Transaction<'_, Sqlite>,
    p: &AccountPayload,
) -> Result<(), AppError> {
    sqlx::query_file!(
        "queries/sync/apply/upsert_account.sql",
        p.id,
        p.name,
        p.currency,
        p.account_type,
        p.account_source,
        p.color,
        p.space_id
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Db(format!("upsert_account: {e}")))?;
    Ok(())
}

async fn upsert_category(
    tx: &mut Transaction<'_, Sqlite>,
    p: &CategoryPayload,
) -> Result<(), AppError> {
    sqlx::query_file!(
        "queries/sync/apply/upsert_category.sql",
        p.id,
        p.name,
        p.color,
        p.space_id
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Db(format!("upsert_category: {e}")))?;
    Ok(())
}

async fn upsert_transaction(
    tx: &mut Transaction<'_, Sqlite>,
    row: &ChangeRow,
    p: &TransactionPayload,
) -> Result<(), AppError> {
    // Hot-field conflict detection runs before the upsert so the
    // detector sees the pre-overwrite local state. The detector is
    // pure; we fetch its inputs here.
    let conflicts = detect_transaction_conflicts(tx, row, p).await?;
    for c in &conflicts {
        record_conflict(tx, &row.space_id, &row.row_id, c).await?;
    }

    sqlx::query_file!(
        "queries/sync/apply/upsert_transaction.sql",
        p.id,
        p.booking_date,
        p.value_date,
        p.reference,
        p.text,
        p.currency,
        p.amount,
        p.balance,
        p.approved,
        p.note,
        p.category,
        p.account_id
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Db(format!("upsert_transaction: {e}")))?;
    Ok(())
}

async fn upsert_account_summary(
    tx: &mut Transaction<'_, Sqlite>,
    p: &AccountSummaryPayload,
) -> Result<(), AppError> {
    sqlx::query_file!(
        "queries/sync/apply/upsert_account_summary.sql",
        p.month,
        p.account_id,
        p.balance
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Db(format!("upsert_account_summary: {e}")))?;
    Ok(())
}

async fn upsert_space_setting(
    tx: &mut Transaction<'_, Sqlite>,
    p: &SpaceSettingPayload,
) -> Result<(), AppError> {
    sqlx::query_file!(
        "queries/sync/apply/upsert_space_setting.sql",
        p.space_id,
        p.key,
        p.value
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Db(format!("upsert_space_setting: {e}")))?;
    Ok(())
}

async fn upsert_space_device(
    tx: &mut Transaction<'_, Sqlite>,
    row: &ChangeRow,
    p: &SpaceDevicePayload,
) -> Result<(), AppError> {
    // Step 30.2: change_log only carries the device_id (no cert).
    // Validation against a real cert happens in the pairing /
    // snapshot paths, not here. We accept the stub and let the
    // pairing-time gate enforce fingerprint uniqueness when the
    // actual cert arrives.
    sqlx::query_file!("queries/sync/apply/upsert_device_stub.sql", p.device_id)
        .execute(&mut **tx)
        .await
        .map_err(|e| AppError::Db(format!("upsert_device_stub: {e}")))?;

    sqlx::query_file!(
        "queries/sync/apply/upsert_space_device_guarded.sql",
        p.space_id,
        p.device_id,
        p.trust_mode,
        p.paired_at,
        row.seq
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Db(format!("upsert_space_device: {e}")))?;
    Ok(())
}

async fn upsert_device_user_grant(
    tx: &mut Transaction<'_, Sqlite>,
    p: &DeviceUserGrantPayload,
) -> Result<(), AppError> {
    sqlx::query_file!(
        "queries/sync/apply/upsert_device_user_grant.sql",
        p.space_id,
        p.device_id,
        p.user_id,
        p.granted_at
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Db(format!("upsert_device_user_grant: {e}")))?;
    Ok(())
}

async fn upsert_durable_revocation(
    tx: &mut Transaction<'_, Sqlite>,
    p: &DurableRevocationPayload,
) -> Result<(), AppError> {
    sqlx::query_file!(
        "queries/sync/apply/upsert_durable_revocation.sql",
        p.revocation_id,
        p.space_id,
        p.target_device_id,
        p.certificate_fingerprint,
        p.winning_revision,
        p.requesting_owner_id,
        p.status,
        p.target_acknowledged,
        p.created_at
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Db(format!("upsert_durable_revocation: {e}")))?;
    Ok(())
}

async fn upsert_model_version(
    tx: &mut Transaction<'_, Sqlite>,
    p: &ModelVersionPayload,
) -> Result<(), AppError> {
    sqlx::query_file!(
        "queries/sync/apply/upsert_model_version.sql",
        p.space_id,
        p.version,
        p.weights_md5,
        p.card_md5,
        p.trained_at
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Db(format!("upsert_model_version: {e}")))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Conflict detection plumbing — fetches the two local inputs the pure
// detector needs and forwards to it. Recording happens here too so the
// transaction stays cohesive.
// ---------------------------------------------------------------------------

async fn detect_transaction_conflicts(
    tx: &mut Transaction<'_, Sqlite>,
    row: &ChangeRow,
    p: &TransactionPayload,
) -> Result<Vec<HotFieldConflict>, AppError> {
    // Current row state for the hot fields. None if this is the first
    // time we're seeing this transaction — no local-vs-remote disagreement
    // is possible in that case.
    let local_row = sqlx::query_file!(
        "queries/sync/apply/get_transaction_for_conflict.sql",
        row.row_id
    )
    .fetch_optional(&mut **tx)
    .await
    .map_err(|e| AppError::Db(format!("get_transaction_for_conflict: {e}")))?;
    let Some(local_row) = local_row else {
        return Ok(Vec::new());
    };

    // Most recent change_log entry for this row authored by a peer
    // other than the sender. Pre-existing pure-local writes count
    // here — that's by design: a local hand edit two seconds before
    // a remote edit lands is exactly the case worth surfacing.
    let last_remote = sqlx::query_file!(
        "queries/sync/apply/last_remote_change.sql",
        row.table_name,
        row.row_id,
        row.device_id
    )
    .fetch_optional(&mut **tx)
    .await
    .map_err(|e| AppError::Db(format!("last_remote_change: {e}")))?;

    let (last_changed_at, last_device_id) = match last_remote {
        Some(r) => (Some(r.changed_at), Some(r.device_id)),
        None => (None, None),
    };

    let snapshot = LocalSnapshot {
        category: local_row.category.as_deref(),
        note: local_row.note.as_str(),
        last_local_changed_at: last_changed_at.as_deref(),
        last_local_device_id: last_device_id.as_deref(),
    };

    Ok(conflict_detector::detect_hot_field_conflict(
        &snapshot,
        &row.device_id,
        &row.changed_at,
        p,
    ))
}

async fn record_conflict(
    tx: &mut Transaction<'_, Sqlite>,
    space_id: &str,
    row_id: &str,
    c: &HotFieldConflict,
) -> Result<(), AppError> {
    let id = Uuid::new_v4().to_string();
    let table_name = "transactions";
    let field = c.field.as_str();
    sqlx::query_file!(
        "queries/sync/apply/insert_sync_conflict.sql",
        id,
        space_id,
        table_name,
        row_id,
        field,
        c.local_value,
        c.remote_value,
        c.local_changed_at,
        c.remote_changed_at
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Db(format!("insert_sync_conflict: {e}")))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Composite-key splitter. The change_log triggers for `space_members`,
// `account_summaries`, `space_settings`, and `model_versions` emit
// `row_id` as the two key parts joined by ':'. We only need to parse
// it on the delete path; upserts get their keys from the payload
// struct directly.
// ---------------------------------------------------------------------------

fn split_composite<'a>(row_id: &'a str, table: &str) -> Result<(&'a str, &'a str), AppError> {
    // The change_log triggers concatenate the two key columns with ':'
    // (see `change_log_*_ad` triggers in `0001_initial_schema.sql`).
    // Per-table key column order is documented at each call site.
    row_id.split_once(':').ok_or_else(|| {
        AppError::InvalidInput(format!(
            "split_composite: malformed composite row_id {row_id:?} for table {table}"
        ))
    })
}

fn split_triple_composite<'a>(
    row_id: &'a str,
    table: &str,
) -> Result<(&'a str, &'a str, &'a str), AppError> {
    let mut parts = row_id.split(':');
    let (Some(first), Some(second), Some(third), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(AppError::InvalidInput(format!(
            "split_triple_composite: malformed row_id {row_id:?} for table {table}"
        )));
    };
    Ok((first, second, third))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::wire::ChangeRow;
    use sqlx::SqlitePool;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn fresh_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let device_id = "local-device";
        sqlx::query_file!("queries/settings/init_device_id.sql", device_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query_file!("queries/settings/init_sync_seq.sql")
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    /// Applying a `spaces` delete from a peer must soft-delete the space
    /// (set `deleted = 1`), not hard-delete it — the skeleton row is
    /// needed so sync queries with `INNER JOIN spaces` still match until
    /// all peers have consumed the deletion. See PLAN.md §9.6.
    #[tokio::test]
    async fn apply_spaces_delete_soft_deletes() {
        let pool = fresh_pool().await;
        let ts = "2024-01-01T00:00:00Z";
        let s1 = "s1";

        sqlx::query_file!("queries/tests/insert_space_fixture.sql", s1, "test", ts, ts)
            .execute(&pool)
            .await
            .unwrap();

        let row = ChangeRow {
            id: "cl-1".into(),
            space_id: s1.into(),
            table_name: "spaces".into(),
            row_id: s1.into(),
            operation: "delete".into(),
            payload: None,
            seq: 99,
            device_id: "peer-1".into(),
            changed_at: ts.into(),
        };

        let mut tx = pool.begin().await.unwrap();
        apply_change(&mut tx, &row).await.unwrap();
        tx.commit().await.unwrap();

        let deleted: i64 = sqlx::query_scalar!("SELECT deleted FROM spaces WHERE id = 's1'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            deleted, 1,
            "apply spaces-delete should soft-delete, not hard-delete"
        );

        let still_exists: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM spaces WHERE id = 's1'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            still_exists, 1,
            "space row must still exist for sync queries"
        );
    }

    /// Applying a space_device grant with the same `(space_id, device_id)`
    /// as an existing local grant must upsert (converge to one logical
    /// grant) instead of failing. The new schema has no random id column —
    /// the PK is `(space_id, device_id)`. See data/PLAN.md Step 30.1.
    #[tokio::test]
    async fn apply_space_device_upsert_converges_to_one_grant() {
        let pool = fresh_pool().await;
        let ts = "2024-01-01T00:00:00Z";
        let s1 = "s1";

        sqlx::query_file!("queries/tests/insert_space_fixture.sql", s1, "test", ts, ts)
            .execute(&pool)
            .await
            .unwrap();

        // Register a device and create a local space_device grant.
        sqlx::query!("INSERT INTO devices (device_id, cert_pem, display_name) VALUES ('peer-1', 'CERT', 'Local Name')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query!(
            "INSERT INTO space_devices (space_id, device_id, trust_mode, paired_at) \
             VALUES ('s1', 'peer-1', 'active', ?1)",
            ts
        )
        .execute(&pool)
        .await
        .unwrap();

        // Apply a changelog row from the peer for the same (space_id, device_id).
        let row = ChangeRow {
            id: "cl-1".into(),
            space_id: s1.into(),
            table_name: "space_devices".into(),
            row_id: format!("{s1}:peer-1"),
            operation: "insert".into(),
            payload: Some(TablePayload::SpaceDevice(SpaceDevicePayload {
                space_id: s1.into(),
                device_id: "peer-1".into(),
                trust_mode: crate::sync::trust_mode::TrustMode::Active,
                paired_at: ts.into(),
            })),
            seq: 1,
            device_id: "peer-1".into(),
            changed_at: ts.into(),
        };

        let mut tx = pool.begin().await.unwrap();
        apply_change(&mut tx, &row).await.unwrap();
        tx.commit().await.unwrap();

        // Only one grant exists for (s1, peer-1).
        let count: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM space_devices WHERE space_id = 's1' AND device_id = 'peer-1'"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 1, "converge to one logical grant");
    }

    #[tokio::test]
    async fn durable_revocation_blocks_older_space_device_grant() {
        let pool = fresh_pool().await;
        let ts = "2024-01-01T00:00:00Z";

        sqlx::query_file!(
            "queries/tests/insert_space_fixture.sql",
            "s1",
            "test",
            ts,
            ts
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query_file!("queries/users/create_user.sql", "owner", "Owner", ts, ts)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query_file!(
            "queries/spaces/add_space_member.sql",
            "s1",
            "owner",
            "owner",
            ts
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query!(
            "INSERT INTO devices (device_id, cert_pem, fingerprint, display_name) VALUES ('peer-1', 'CERT', 'fp-1', 'Peer')"
        )
        .execute(&pool)
        .await
        .unwrap();

        let revocation = ChangeRow {
            id: "revoke-1".into(),
            space_id: "s1".into(),
            table_name: "durable_revocations".into(),
            row_id: "revoke-1".into(),
            operation: "insert".into(),
            payload: Some(TablePayload::DurableRevocation(DurableRevocationPayload {
                revocation_id: "revoke-1".into(),
                space_id: "s1".into(),
                target_device_id: "peer-1".into(),
                certificate_fingerprint: "fp-1".into(),
                winning_revision: 6,
                requesting_owner_id: "owner".into(),
                status: "pending".into(),
                target_acknowledged: 0,
                created_at: ts.into(),
            })),
            seq: 6,
            device_id: "owner-device".into(),
            changed_at: ts.into(),
        };
        let mut tx = pool.begin().await.unwrap();
        apply_change(&mut tx, &revocation).await.unwrap();
        tx.commit().await.unwrap();

        let grant = ChangeRow {
            id: "grant-1".into(),
            space_id: "s1".into(),
            table_name: "space_devices".into(),
            row_id: "s1:peer-1".into(),
            operation: "insert".into(),
            payload: Some(TablePayload::SpaceDevice(SpaceDevicePayload {
                space_id: "s1".into(),
                device_id: "peer-1".into(),
                trust_mode: crate::sync::trust_mode::TrustMode::Active,
                paired_at: ts.into(),
            })),
            seq: 5,
            device_id: "peer-1".into(),
            changed_at: ts.into(),
        };
        let mut tx = pool.begin().await.unwrap();
        apply_change(&mut tx, &grant).await.unwrap();
        tx.commit().await.unwrap();

        let count: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM space_devices WHERE space_id = 's1' AND device_id = 'peer-1'"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 0);
        let tombstone_count: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM durable_revocations WHERE revocation_id = 'revoke-1'"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(tombstone_count, 1);
    }

    #[tokio::test]
    async fn removing_one_user_preserves_shared_device_access() {
        let pool = fresh_pool().await;
        let ts = "2024-01-01T00:00:00Z";

        sqlx::query_file!(
            "queries/tests/insert_space_fixture.sql",
            "s1",
            "test",
            ts,
            ts
        )
        .execute(&pool)
        .await
        .unwrap();
        for user_id in ["u1", "u2"] {
            sqlx::query_file!("queries/users/create_user.sql", user_id, user_id, ts, ts)
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query_file!(
                "queries/spaces/add_space_member.sql",
                "s1",
                user_id,
                "member",
                ts
            )
            .execute(&pool)
            .await
            .unwrap();
        }
        sqlx::query!(
            "INSERT INTO devices (device_id, cert_pem, display_name) VALUES ('device-1', 'CERT', 'Shared')"
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query!(
            "INSERT INTO space_devices (space_id, device_id, trust_mode, paired_at) \
             VALUES ('s1', 'device-1', 'active', ?1)",
            ts
        )
        .execute(&pool)
        .await
        .unwrap();

        for user_id in ["u1", "u2"] {
            sqlx::query_file!(
                "queries/sync/upsert_device_user_grant.sql",
                "s1",
                "device-1",
                user_id,
                ts
            )
            .execute(&pool)
            .await
            .unwrap();
        }

        sqlx::query_file!("queries/spaces/remove_space_member.sql", "s1", "u1")
            .execute(&pool)
            .await
            .unwrap();

        let device_count: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM space_devices WHERE space_id = 's1' AND device_id = 'device-1'"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let remaining_grants: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM device_user_grants \
             WHERE space_id = 's1' AND device_id = 'device-1' AND user_id = 'u2'"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let removed_grants: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM device_user_grants \
             WHERE space_id = 's1' AND device_id = 'device-1' AND user_id = 'u1'"
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(device_count, 1);
        assert_eq!(remaining_grants, 1);
        assert_eq!(removed_grants, 0);
    }
}
