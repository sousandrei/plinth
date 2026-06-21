use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

use crate::{Session, db::DbPool, error::AppError, sync::gc};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Space {
    pub id: String,
    pub name: String,
    pub role: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SpaceMember {
    pub user_id: String,
    pub name: String,
    pub role: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::FromRow)]
pub struct ExportSpaceInfo {
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::FromRow)]
pub struct ExportCategory {
    pub id: String,
    pub name: String,
    pub color: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::FromRow)]
pub struct ExportAccount {
    pub id: String,
    pub name: String,
    pub currency: String,
    pub account_type: String,
    pub account_source: String,
    pub color: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::FromRow)]
pub struct ExportTransaction {
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
    pub category: String,
    pub account_id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::FromRow)]
pub struct ExportAccountSummary {
    pub month: String,
    pub account_id: String,
    pub balance: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExportPayload {
    pub version: u32,
    pub exported_at: String,
    pub space: ExportSpaceInfo,
    pub categories: Vec<ExportCategory>,
    pub accounts: Vec<ExportAccount>,
    pub transactions: Vec<ExportTransaction>,
    pub account_summaries: Vec<ExportAccountSummary>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExportResult {
    pub categories: usize,
    pub accounts: usize,
    pub transactions: usize,
    pub account_summaries: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ImportResult {
    pub categories: usize,
    pub accounts: usize,
    pub transactions: usize,
    pub account_summaries: usize,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub async fn seed_default_categories(space_id: &str, db: &DbPool) -> Result<(), AppError> {
    sqlx::query_file!("queries/spaces/seed_default_categories.sql", space_id)
        .execute(db)
        .await
        .map_err(|e| AppError::Db(format!("seed_default_categories: {e}")))?;
    Ok(())
}

pub async fn create_space_for_user(
    user_id: &str,
    space_name: &str,
    db: &DbPool,
) -> Result<String, AppError> {
    let space_id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    sqlx::query_file!("queries/spaces/create_space.sql", space_id, space_name, now)
        .execute(db)
        .await
        .map_err(|e| AppError::Db(format!("create_space: {e}")))?;

    sqlx::query_file!(
        "queries/spaces/add_space_member.sql",
        space_id,
        user_id,
        "owner",
        now
    )
    .execute(db)
    .await
    .map_err(|e| AppError::Db(format!("add_space_member (owner): {e}")))?;

    seed_default_categories(&space_id, db).await?;

    Ok(space_id)
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_my_spaces(
    session: State<'_, Session>,
    db: State<'_, DbPool>,
) -> Result<Vec<Space>, AppError> {
    let data = session.require_user()?;

    let rows = sqlx::query_file!("queries/spaces/list_spaces_for_user.sql", data.user_id)
        .fetch_all(db.inner())
        .await
        .map_err(|e| AppError::Db(format!("list_my_spaces: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|r| Space {
            id: r.id,
            name: r.name,
            role: r.role,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
        .collect())
}

#[tauri::command]
pub async fn create_space(
    name: String,
    session: State<'_, Session>,
    db: State<'_, DbPool>,
) -> Result<Space, AppError> {
    if name.trim().is_empty() {
        return Err(AppError::InvalidInput("space name cannot be empty".into()));
    }

    let data = session.require()?;
    let space_id = create_space_for_user(&data.user_id, name.trim(), db.inner()).await?;

    let rows = sqlx::query_file!("queries/spaces/list_spaces_for_user.sql", data.user_id)
        .fetch_all(db.inner())
        .await
        .map_err(|e| AppError::Db(format!("create_space fetch: {e}")))?;

    let space = rows
        .into_iter()
        .find(|r| r.id == space_id)
        .map(|r| Space {
            id: r.id,
            name: r.name,
            role: r.role,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
        .ok_or_else(|| AppError::Internal("create_space: not found after insert".into()))?;

    Ok(space)
}

#[tauri::command]
pub async fn set_active_space(
    space_id: String,
    session: State<'_, Session>,
    db: State<'_, DbPool>,
) -> Result<(), AppError> {
    let data = session.require_user()?;

    let is_member = sqlx::query_file!("queries/spaces/get_member_role.sql", space_id, data.user_id)
        .fetch_optional(db.inner())
        .await
        .map_err(|e| AppError::Db(format!("set_active_space membership check: {e}")))?
        .is_some();

    if !is_member {
        return Err(AppError::Forbidden);
    }

    session.set_space(space_id)?;
    Ok(())
}

#[tauri::command]
pub async fn logout(session: State<'_, Session>) -> Result<(), AppError> {
    session.clear()?;
    Ok(())
}

#[tauri::command]
pub async fn list_space_members(
    session: State<'_, Session>,
    db: State<'_, DbPool>,
) -> Result<Vec<SpaceMember>, AppError> {
    let data = session.require()?;

    let rows = sqlx::query_file!("queries/spaces/list_space_members.sql", data.space_id)
        .fetch_all(db.inner())
        .await
        .map_err(|e| AppError::Db(format!("list_space_members: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|r| SpaceMember {
            user_id: r.user_id,
            name: r.name,
            role: r.role,
        })
        .collect())
}

#[tauri::command]
pub async fn add_space_member(
    user_id: String,
    session: State<'_, Session>,
    db: State<'_, DbPool>,
) -> Result<(), AppError> {
    let data = session.require()?;
    require_owner(&data.space_id, &data.user_id, db.inner()).await?;

    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    sqlx::query_file!(
        "queries/spaces/add_space_member.sql",
        data.space_id,
        user_id,
        "member",
        now
    )
    .execute(db.inner())
    .await
    .map_err(|e| AppError::Db(format!("add_space_member: {e}")))?;

    Ok(())
}

#[tauri::command]
pub async fn remove_space_member(
    user_id: String,
    session: State<'_, Session>,
    db: State<'_, DbPool>,
) -> Result<(), AppError> {
    let data = session.require()?;
    require_owner(&data.space_id, &data.user_id, db.inner()).await?;

    if user_id == data.user_id {
        return Err(AppError::InvalidInput(
            "use leave_space to remove yourself".into(),
        ));
    }

    sqlx::query_file!(
        "queries/spaces/remove_space_member.sql",
        data.space_id,
        user_id
    )
    .execute(db.inner())
    .await
    .map_err(|e| AppError::Db(format!("remove_space_member: {e}")))?;

    Ok(())
}

#[tauri::command]
pub async fn leave_space(
    session: State<'_, Session>,
    db: State<'_, DbPool>,
) -> Result<(), AppError> {
    let data = session.require()?;

    let count = sqlx::query_file!("queries/spaces/count_space_members.sql", data.space_id)
        .fetch_one(db.inner())
        .await
        .map_err(|e| AppError::Db(format!("leave_space count: {e}")))?
        .count;

    if count <= 1 {
        return Err(AppError::InvalidInput(
            "you are the last member; delete the space instead".into(),
        ));
    }

    sqlx::query_file!(
        "queries/spaces/remove_space_member.sql",
        data.space_id,
        data.user_id
    )
    .execute(db.inner())
    .await
    .map_err(|e| AppError::Db(format!("leave_space: {e}")))?;

    session.clear_space()?;
    Ok(())
}

#[tauri::command]
pub async fn delete_space(
    session: State<'_, Session>,
    db: State<'_, DbPool>,
) -> Result<(), AppError> {
    let data = session.require()?;
    require_owner(&data.space_id, &data.user_id, db.inner()).await?;

    let space_id = &data.space_id;
    let mut tx = db
        .inner()
        .begin()
        .await
        .map_err(|e| AppError::Db(format!("delete_space begin: {e}")))?;

    // Soft-delete the space row (no change_log entry — the update trigger
    // has WHEN NEW.deleted = 0).
    sqlx::query_file!("queries/spaces/soft_delete_space.sql", space_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Db(format!("delete_space soft_delete: {e}")))?;

    // Delete child data. Triggers fire for each deletion, populating
    // change_log with entries that sync will ship to peers.
    // trusted_devices and evicted_devices are preserved so that sync
    // queries (which INNER JOIN spaces) continue to match the soft-deleted
    // space until all peers have consumed the deletion changes.
    sqlx::query_file!("queries/spaces/delete_space_members.sql", space_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Db(format!("delete_space members: {e}")))?;

    sqlx::query_file!("queries/spaces/delete_space_settings.sql", space_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Db(format!("delete_space settings: {e}")))?;

    sqlx::query_file!("queries/spaces/delete_space_categories.sql", space_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Db(format!("delete_space categories: {e}")))?;

    // Delete transactions and account_summaries before accounts — the
    // change_log triggers on those tables look up space_id from the accounts
    // row, which must still exist.
    sqlx::query_file!("queries/spaces/delete_space_transactions.sql", space_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Db(format!("delete_space transactions: {e}")))?;

    sqlx::query_file!(
        "queries/spaces/delete_space_account_summaries.sql",
        space_id
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Db(format!("delete_space summaries: {e}")))?;

    sqlx::query_file!("queries/spaces/delete_space_accounts.sql", space_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Db(format!("delete_space accounts: {e}")))?;

    // NOT deleted: trusted_devices, evicted_devices (needed for sync queries
    // to still match), sync_cursors (needed by GC all_peers_consumed pass),
    // sync_conflicts (harmless, cleaned up by 90-day cap if needed).

    // Increment sync_seq one more time so the space delete entry has a seq
    // higher than all the child-table deletes above.
    sqlx::query_file!("queries/spaces/increment_sync_seq.sql")
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Db(format!("delete_space increment_seq: {e}")))?;

    // Manually insert a 'delete' change_log entry for the space itself.
    sqlx::query_file!("queries/spaces/insert_space_delete_changelog.sql", space_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Db(format!("delete_space changelog: {e}")))?;

    tx.commit()
        .await
        .map_err(|e| AppError::Db(format!("delete_space commit: {e}")))?;

    gc::run(db.inner())
        .await
        .map_err(|e| AppError::Db(format!("delete_space gc: {e}")))?;

    session.clear_space()?;
    Ok(())
}

#[tauri::command]
pub async fn evict_space(
    space_id: String,
    session: State<'_, Session>,
    db: State<'_, DbPool>,
) -> Result<(), AppError> {
    sqlx::query_file!("queries/spaces/delete_space.sql", space_id)
        .execute(db.inner())
        .await
        .map_err(|e| AppError::Db(format!("evict_space delete_space: {e}")))?;

    sqlx::query_file!("queries/spaces/delete_change_log_for_space.sql", space_id)
        .execute(db.inner())
        .await
        .map_err(|e| AppError::Db(format!("evict_space change_log: {e}")))?;

    let session_active = if let Ok(data) = session.require() {
        data.space_id == space_id
    } else {
        false
    };
    if session_active {
        session.clear_space()?;
    }

    Ok(())
}

#[tauri::command]
pub async fn rename_space(
    name: String,
    session: State<'_, Session>,
    db: State<'_, DbPool>,
) -> Result<(), AppError> {
    let data = session.require()?;
    require_owner(&data.space_id, &data.user_id, db.inner()).await?;

    if name.trim().is_empty() {
        return Err(AppError::InvalidInput("space name cannot be empty".into()));
    }

    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    sqlx::query_file!("queries/spaces/rename_space.sql", name, now, data.space_id)
        .execute(db.inner())
        .await
        .map_err(|e| AppError::Db(format!("rename_space: {e}")))?;

    Ok(())
}

#[tauri::command]
pub async fn update_member_role(
    user_id: String,
    role: String,
    session: State<'_, Session>,
    db: State<'_, DbPool>,
) -> Result<(), AppError> {
    let data = session.require()?;
    require_owner(&data.space_id, &data.user_id, db.inner()).await?;

    if role != "owner" && role != "member" {
        return Err(AppError::InvalidInput(
            "role must be 'owner' or 'member'".into(),
        ));
    }

    sqlx::query_file!(
        "queries/spaces/update_member_role.sql",
        role,
        data.space_id,
        user_id
    )
    .execute(db.inner())
    .await
    .map_err(|e| AppError::Db(format!("update_member_role: {e}")))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Export / Import
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn export_space_data(
    space_id: String,
    path: String,
    session: State<'_, Session>,
    db: State<'_, DbPool>,
) -> Result<ExportResult, AppError> {
    let data = session.require_user()?;
    require_member(&space_id, &data.user_id, db.inner()).await?;

    let space = sqlx::query_file!("queries/spaces/get_space_name.sql", space_id)
        .fetch_optional(db.inner())
        .await
        .map_err(|e| AppError::Db(format!("export_space_data space: {e}")))?
        .ok_or_else(|| AppError::NotFound(format!("space {space_id}")))?;

    let categories = sqlx::query_file_as!(
        ExportCategory,
        "queries/spaces/export_categories.sql",
        space_id
    )
    .fetch_all(db.inner())
    .await
    .map_err(|e| AppError::Db(format!("export_space_data categories: {e}")))?;

    let accounts = sqlx::query_file_as!(
        ExportAccount,
        "queries/spaces/export_accounts.sql",
        space_id
    )
    .fetch_all(db.inner())
    .await
    .map_err(|e| AppError::Db(format!("export_space_data accounts: {e}")))?;

    let transactions = sqlx::query_file_as!(
        ExportTransaction,
        "queries/spaces/export_transactions.sql",
        space_id
    )
    .fetch_all(db.inner())
    .await
    .map_err(|e| AppError::Db(format!("export_space_data transactions: {e}")))?;

    let account_summaries = sqlx::query_file_as!(
        ExportAccountSummary,
        "queries/spaces/export_account_summaries.sql",
        space_id
    )
    .fetch_all(db.inner())
    .await
    .map_err(|e| AppError::Db(format!("export_space_data summaries: {e}")))?;

    let payload = ExportPayload {
        version: 1,
        exported_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        space: ExportSpaceInfo { name: space.name },
        categories: categories.clone(),
        accounts: accounts.clone(),
        transactions: transactions.clone(),
        account_summaries: account_summaries.clone(),
    };

    let json = serde_json::to_string_pretty(&payload)
        .map_err(|e| AppError::Internal(format!("export_space_data serialize: {e}")))?;

    std::fs::write(&path, json)
        .map_err(|e| AppError::Io(format!("export_space_data write: {e}")))?;

    Ok(ExportResult {
        categories: categories.len(),
        accounts: accounts.len(),
        transactions: transactions.len(),
        account_summaries: account_summaries.len(),
    })
}

#[tauri::command]
pub async fn import_space_data(
    space_id: String,
    path: String,
    session: State<'_, Session>,
    db: State<'_, DbPool>,
) -> Result<ImportResult, AppError> {
    let data = session.require_user()?;
    require_member(&space_id, &data.user_id, db.inner()).await?;

    let file_contents = std::fs::read_to_string(&path)
        .map_err(|e| AppError::Io(format!("import_space_data read: {e}")))?;

    let payload: ExportPayload = serde_json::from_str(&file_contents)
        .map_err(|e| AppError::InvalidInput(format!("invalid export file: {e}")))?;

    if payload.version != 1 {
        return Err(AppError::InvalidInput(format!(
            "unsupported export version: {} (expected 1)",
            payload.version
        )));
    }

    let mut tx = db
        .inner()
        .begin()
        .await
        .map_err(|e| AppError::Db(format!("import_space_data begin: {e}")))?;

    // Clear existing categories so the UNIQUE(space_id, name) constraint
    // doesn't collide with imported rows (e.g. default seeded categories).
    sqlx::query_file!("queries/spaces/delete_space_categories.sql", space_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Db(format!("import_space_data clear categories: {e}")))?;

    for c in &payload.categories {
        sqlx::query_file!(
            "queries/spaces/import_category.sql",
            c.id,
            c.name,
            c.color,
            space_id,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Db(format!("import_space_data category: {e}")))?;
    }

    for a in &payload.accounts {
        sqlx::query_file!(
            "queries/spaces/import_account.sql",
            a.id,
            a.name,
            a.currency,
            a.account_type,
            a.account_source,
            a.color,
            space_id,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Db(format!("import_space_data account: {e}")))?;
    }

    for t in &payload.transactions {
        sqlx::query_file!(
            "queries/spaces/import_transaction.sql",
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
            t.category,
            t.account_id,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Db(format!("import_space_data transaction: {e}")))?;
    }

    for s in &payload.account_summaries {
        sqlx::query_file!(
            "queries/spaces/import_account_summary.sql",
            s.month,
            s.account_id,
            s.balance,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Db(format!("import_space_data summary: {e}")))?;
    }

    tx.commit()
        .await
        .map_err(|e| AppError::Db(format!("import_space_data commit: {e}")))?;

    Ok(ImportResult {
        categories: payload.categories.len(),
        accounts: payload.accounts.len(),
        transactions: payload.transactions.len(),
        account_summaries: payload.account_summaries.len(),
    })
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

async fn require_owner(space_id: &str, user_id: &str, db: &DbPool) -> Result<(), AppError> {
    let row = sqlx::query_file!("queries/spaces/get_member_role.sql", space_id, user_id)
        .fetch_optional(db)
        .await
        .map_err(|e| AppError::Db(format!("require_owner: {e}")))?;

    match row {
        None => Err(AppError::Forbidden),
        Some(r) if r.role != "owner" => Err(AppError::Forbidden),
        _ => Ok(()),
    }
}

async fn require_member(space_id: &str, user_id: &str, db: &DbPool) -> Result<(), AppError> {
    let row = sqlx::query_file!("queries/spaces/get_member_role.sql", space_id, user_id)
        .fetch_optional(db)
        .await
        .map_err(|e| AppError::Db(format!("require_member: {e}")))?;

    if row.is_none() {
        return Err(AppError::Forbidden);
    }
    Ok(())
}
