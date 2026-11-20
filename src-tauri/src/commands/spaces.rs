use std::{collections::HashSet, io::Write};

use serde::{Deserialize, Serialize};
use sqlx::{Executor, Sqlite, SqliteConnection};
use tauri::{AppHandle, State};
use uuid::Uuid;

use crate::{
    ClassifierState, Session, commands::training::load_active_classifier, db::DbPool,
    error::AppError, sync_preview::SyncPreview,
};

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::FromRow)]
pub struct UserRecord {
    pub id: String,
    pub name: String,
    pub has_pin: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SessionSnapshot {
    pub user: Option<UserRecord>,
    pub space_id: Option<String>,
}

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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct SyncSeed {
    pub(crate) space_id: String,
    snapshot: ExportPayload,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub async fn create_space_for_user(
    user_id: &str,
    space_name: &str,
    conn: &mut SqliteConnection,
    preview: &SyncPreview,
) -> Result<String, AppError> {
    let space_id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    sqlx::query_file!("queries/spaces/create_space.sql", space_id, space_name, now)
        .execute(&mut *conn)
        .await
        .map_err(|e| AppError::Db(format!("create_space: {e}")))?;

    sqlx::query_file!(
        "queries/spaces/add_space_member.sql",
        space_id,
        user_id,
        "owner",
        now
    )
    .execute(&mut *conn)
    .await
    .map_err(|e| AppError::Db(format!("add_space_member (owner): {e}")))?;

    sqlx::query_file!("queries/spaces/seed_default_categories.sql", space_id)
        .execute(conn)
        .await
        .map_err(|e| AppError::Db(format!("seed_default_categories: {e}")))?;
    preview.initialize_local_space(&space_id, user_id).await?;

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
    preview: State<'_, SyncPreview>,
) -> Result<Space, AppError> {
    if name.trim().is_empty() {
        return Err(AppError::InvalidInput("space name cannot be empty".into()));
    }

    let data = session.require()?;
    let mut tx = db.begin().await?;
    let space_id =
        create_space_for_user(&data.user_id, name.trim(), &mut tx, preview.inner()).await?;
    tx.commit().await?;

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
    app: AppHandle,
    session: State<'_, Session>,
    db: State<'_, DbPool>,
    classifier: State<'_, ClassifierState>,
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

    session.set_space(space_id.clone())?;
    load_active_classifier(&app, db.inner(), &classifier, &space_id).await?;

    Ok(())
}

#[tauri::command]
pub async fn logout(session: State<'_, Session>) -> Result<(), AppError> {
    session.clear()?;
    Ok(())
}

#[tauri::command]
pub async fn get_session(
    session: State<'_, Session>,
    db: State<'_, DbPool>,
) -> Result<SessionSnapshot, AppError> {
    let (user_id, space_id) = {
        let guard = session.lock()?;
        match &*guard {
            None => (None, None),
            Some(data) => (Some(data.user_id.clone()), data.space_id.clone()),
        }
    };

    let user = match user_id {
        None => None,
        Some(uid) => Some(
            sqlx::query_file_as!(UserRecord, "queries/users/get_user.sql", uid)
                .fetch_optional(db.inner())
                .await
                .map_err(|e| AppError::Db(format!("get_session: {e}")))?
                .ok_or_else(|| AppError::NotFound(format!("user {uid}")))?,
        ),
    };

    Ok(SessionSnapshot { user, space_id })
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

    sqlx::query_file!("queries/spaces/delete_space.sql", data.space_id)
        .execute(db.inner())
        .await
        .map_err(|e| AppError::Db(format!("delete_space: {e}")))?;

    session.clear_space()?;
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
    let mut tx = db.begin().await?;
    let payload = read_space_export(&mut tx, &space_id).await?;
    tx.commit().await?;
    let json = serde_json::to_string_pretty(&payload)
        .map_err(|e| AppError::Internal(format!("export_space_data serialize: {e}")))?;
    std::fs::write(&path, json)
        .map_err(|e| AppError::Io(format!("export_space_data write: {e}")))?;

    Ok(ExportResult {
        categories: payload.categories.len(),
        accounts: payload.accounts.len(),
        transactions: payload.transactions.len(),
        account_summaries: payload.account_summaries.len(),
    })
}

async fn read_space_export(
    conn: &mut SqliteConnection,
    space_id: &str,
) -> Result<ExportPayload, AppError> {
    let space = sqlx::query_file!("queries/spaces/get_space_name.sql", space_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(|e| AppError::Db(format!("export_space_data space: {e}")))?
        .ok_or_else(|| AppError::NotFound(format!("space {space_id}")))?;

    let categories = sqlx::query_file_as!(
        ExportCategory,
        "queries/spaces/export_categories.sql",
        space_id
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| AppError::Db(format!("export_space_data categories: {e}")))?;

    let accounts = sqlx::query_file_as!(
        ExportAccount,
        "queries/spaces/export_accounts.sql",
        space_id
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| AppError::Db(format!("export_space_data accounts: {e}")))?;

    let transactions = sqlx::query_file_as!(
        ExportTransaction,
        "queries/spaces/export_transactions.sql",
        space_id
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| AppError::Db(format!("export_space_data transactions: {e}")))?;

    let account_summaries = sqlx::query_file_as!(
        ExportAccountSummary,
        "queries/spaces/export_account_summaries.sql",
        space_id
    )
    .fetch_all(conn)
    .await
    .map_err(|e| AppError::Db(format!("export_space_data summaries: {e}")))?;

    Ok(ExportPayload {
        version: 1,
        exported_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        space: ExportSpaceInfo { name: space.name },
        categories,
        accounts,
        transactions,
        account_summaries,
    })
}

#[tauri::command]
pub async fn import_space_data(
    space_id: String,
    path: String,
    session: State<'_, Session>,
    db: State<'_, DbPool>,
    preview: State<'_, SyncPreview>,
    app: AppHandle,
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

    // Restoring a backup into an existing preview preserves its shared edits.
    let preserve_edits = if app.config().identifier == crate::sync_preview::IDENTIFIER {
        true
    } else {
        let scope = space_id.clone();
        preview
            .with_store(move |store| {
                Ok(store
                    .paired_devices()?
                    .iter()
                    .any(|peer| peer.space_id == scope))
            })
            .await?
    };
    write_space_export(&mut tx, &space_id, &payload, preserve_edits).await?;
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

async fn write_space_export(
    conn: &mut SqliteConnection,
    space_id: &str,
    payload: &ExportPayload,
    preserve_edits: bool,
) -> Result<(), AppError> {
    for c in &payload.categories {
        sqlx::query_file!(
            "queries/spaces/import_category.sql",
            c.id,
            c.name,
            c.color,
            space_id,
        )
        .execute(&mut *conn)
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
        .execute(&mut *conn)
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
            preserve_edits,
        )
        .execute(&mut *conn)
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
        .execute(&mut *conn)
        .await
        .map_err(|e| AppError::Db(format!("import_space_data summary: {e}")))?;
    }

    Ok(())
}

#[tauri::command]
pub async fn export_sync_seed(
    space_id: String,
    path: String,
    session: State<'_, Session>,
    db: State<'_, DbPool>,
) -> Result<(), AppError> {
    let user = session.require_user()?;
    let seed = make_sync_seed(db.inner(), &space_id, &user.user_id).await?;
    tokio::task::spawn_blocking(move || {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options
            .open(path)
            .map_err(|e| AppError::Io(format!("create sync seed: {e}")))?;
        let mut writer = std::io::BufWriter::new(file);
        serde_json::to_writer(&mut writer, &seed)
            .map_err(|e| AppError::Io(format!("write sync seed: {e}")))?;
        writer
            .flush()
            .map_err(|e| AppError::Io(format!("flush sync seed: {e}")))
    })
    .await
    .map_err(|e| AppError::Internal(format!("export sync seed: {e}")))?
}

pub(crate) async fn make_sync_seed(
    db: &DbPool,
    space_id: &str,
    user_id: &str,
) -> Result<SyncSeed, AppError> {
    let (tx, seed) = prepare_sync_seed_export(db, space_id, user_id).await?;
    tx.commit().await?;
    Ok(seed)
}

pub(crate) async fn prepare_sync_seed_export(
    db: &DbPool,
    space_id: &str,
    user_id: &str,
) -> Result<(sqlx::Transaction<'static, Sqlite>, SyncSeed), AppError> {
    let mut tx = db.begin().await?;
    require_owner(space_id, user_id, &mut *tx).await?;
    let snapshot = read_space_export(&mut tx, space_id).await?;
    Ok((
        tx,
        SyncSeed {
            space_id: space_id.to_owned(),
            snapshot,
        },
    ))
}

#[tauri::command]
pub async fn import_sync_seed(
    path: String,
    session: State<'_, Session>,
    db: State<'_, DbPool>,
    preview: State<'_, SyncPreview>,
    app: AppHandle,
) -> Result<String, AppError> {
    if !preview.active() || app.config().identifier != crate::sync_preview::IDENTIFIER {
        return Err(AppError::Forbidden);
    }
    let user = session.require_user()?;
    let seed = tokio::task::spawn_blocking(move || {
        let file =
            std::fs::File::open(path).map_err(|e| AppError::Io(format!("open sync seed: {e}")))?;
        serde_json::from_reader(std::io::BufReader::new(file))
            .map_err(|e| AppError::InvalidInput(format!("invalid sync seed: {e}")))
    })
    .await
    .map_err(|e| AppError::Internal(format!("read sync seed: {e}")))??;
    install_sync_seed(db.inner(), &user.user_id, seed).await
}

pub(crate) async fn install_sync_seed(
    db: &DbPool,
    user_id: &str,
    seed: SyncSeed,
) -> Result<String, AppError> {
    let space_id = seed.space_id.clone();
    prepare_sync_seed(db, user_id, seed).await?.commit().await?;
    Ok(space_id)
}

pub(crate) async fn prepare_sync_seed(
    db: &DbPool,
    user_id: &str,
    seed: SyncSeed,
) -> Result<sqlx::Transaction<'static, Sqlite>, AppError> {
    let mut tx = db.begin_with("BEGIN IMMEDIATE").await?;
    write_sync_seed(&mut tx, user_id, seed).await?;
    Ok(tx)
}

pub(crate) async fn write_sync_seed(
    conn: &mut SqliteConnection,
    user_id: &str,
    seed: SyncSeed,
) -> Result<(), AppError> {
    let SyncSeed { space_id, snapshot } = seed;
    if snapshot.version != 1 || space_id.trim().is_empty() {
        return Err(AppError::InvalidInput(
            "unsupported sync seed or missing space ID".into(),
        ));
    }
    let accounts: HashSet<_> = snapshot
        .accounts
        .iter()
        .map(|account| account.id.as_str())
        .collect();
    let categories: HashSet<_> = snapshot
        .categories
        .iter()
        .map(|category| category.id.as_str())
        .collect();
    let transactions: HashSet<_> = snapshot
        .transactions
        .iter()
        .map(|transaction| transaction.id.as_str())
        .collect();
    let summaries: HashSet<_> = snapshot
        .account_summaries
        .iter()
        .map(|summary| (summary.account_id.as_str(), summary.month.as_str()))
        .collect();
    if accounts.len() != snapshot.accounts.len()
        || categories.len() != snapshot.categories.len()
        || transactions.len() != snapshot.transactions.len()
        || summaries.len() != snapshot.account_summaries.len()
        || snapshot
            .transactions
            .iter()
            .any(|row| !accounts.contains(row.account_id.as_str()))
        || snapshot
            .account_summaries
            .iter()
            .any(|row| !accounts.contains(row.account_id.as_str()))
    {
        return Err(AppError::InvalidInput(
            "sync seed has duplicate IDs or missing accounts".into(),
        ));
    }

    let records = serde_json::to_string(&snapshot)
        .map_err(|e| AppError::Internal(format!("encode sync seed: {e}")))?;
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let conflict = sqlx::query_file!(
        "queries/spaces/has_sync_seed_conflict.sql",
        space_id,
        records
    )
    .fetch_one(&mut *conn)
    .await?;
    if conflict.exists {
        return Err(AppError::InvalidInput(
            "sync seed overlaps with existing space data".into(),
        ));
    }
    sqlx::query_file!(
        "queries/spaces/create_space.sql",
        space_id,
        snapshot.space.name,
        now
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query_file!(
        "queries/spaces/add_space_member.sql",
        space_id,
        user_id,
        "owner",
        now
    )
    .execute(&mut *conn)
    .await?;
    write_space_export(conn, &space_id, &snapshot, false).await
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

pub(crate) async fn require_owner<'a>(
    space_id: &str,
    user_id: &str,
    db: impl Executor<'a, Database = Sqlite>,
) -> Result<(), AppError> {
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

pub(crate) async fn require_member(
    space_id: &str,
    user_id: &str,
    db: &DbPool,
) -> Result<(), AppError> {
    let row = sqlx::query_file!("queries/spaces/get_member_role.sql", space_id, user_id)
        .fetch_optional(db)
        .await
        .map_err(|e| AppError::Db(format!("require_member: {e}")))?;

    if row.is_none() {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

#[cfg(test)]
mod sync_seed_tests {
    use super::*;
    use std::{env, fs, path::Path, time::Duration};

    use crate::{
        commands::transactions::update_in_preview,
        sync_preview::{PeerConnection, resolve_peer},
    };
    use plinth_sync::transport::Identity;

    const DATE: &str = "2026-11-19";
    const RECORD: &str = "household-transaction";
    const OFFLINE_NOTE: &str = "Edited offline on Windows";

    async fn create_user(db: &DbPool, user: &str) {
        sqlx::query_file!("queries/users/create_user.sql", user, user, DATE, DATE)
            .execute(db)
            .await
            .unwrap();
    }

    async fn sample_seed() -> SyncSeed {
        let source_dir = tempfile::tempdir().unwrap();
        let source = crate::db::init(
            &source_dir.path().join("finance.sqlite"),
            &SyncPreview::default(),
        )
        .await
        .unwrap();
        create_user(&source, "source-owner").await;
        let mut create = source.begin().await.unwrap();
        let space_id = create_space_for_user(
            "source-owner",
            "Household",
            &mut create,
            &SyncPreview::default(),
        )
        .await
        .unwrap();
        create.commit().await.unwrap();
        let account_id = "household-account";
        sqlx::query_file!(
            "queries/spaces/import_account.sql",
            account_id,
            "Checking",
            "SEK",
            "checking",
            "manual",
            "#111111",
            space_id
        )
        .execute(&source)
        .await
        .unwrap();
        sqlx::query_file!(
            "queries/spaces/import_transaction.sql",
            RECORD,
            DATE,
            DATE,
            "",
            "Market",
            "SEK",
            -1500_i64,
            9000_i64,
            1_i64,
            "Paid",
            "Groceries",
            account_id,
            false
        )
        .execute(&source)
        .await
        .unwrap();
        sqlx::query_file!(
            "queries/spaces/import_account_summary.sql",
            "2026-11",
            account_id,
            9000_i64
        )
        .execute(&source)
        .await
        .unwrap();

        assert!(matches!(
            make_sync_seed(&source, &space_id, "local-owner").await,
            Err(AppError::Forbidden)
        ));
        let seed = make_sync_seed(&source, &space_id, "source-owner")
            .await
            .unwrap();
        source.close().await;
        seed
    }

    async fn open_preview(directory: &Path) -> (SyncPreview, DbPool) {
        let preview = SyncPreview::open(directory.to_owned()).await.unwrap();
        let db = crate::db::init(&directory.join("finance.sqlite"), &preview)
            .await
            .unwrap();
        create_user(&db, "local-owner").await;
        (preview, db)
    }

    #[tokio::test]
    async fn bootstraps_a_fresh_preview_with_stable_ids_and_rejects_overlap() {
        let destination_dir = tempfile::tempdir().unwrap();
        let (_preview, destination) = open_preview(destination_dir.path()).await;
        let seed = sample_seed().await;
        let space_id = seed.space_id.clone();
        assert!(!seed.snapshot.categories.is_empty());
        assert_eq!(seed.snapshot.transactions[0].id, "household-transaction");
        let mut invalid = seed.clone();
        invalid.snapshot.categories[1].name = invalid.snapshot.categories[0].name.clone();
        assert!(matches!(
            install_sync_seed(&destination, "local-owner", invalid).await,
            Err(AppError::Db(_))
        ));
        let contents = serde_json::to_vec(&seed).unwrap();
        let decoded: SyncSeed = serde_json::from_slice(&contents).unwrap();
        assert_eq!(
            install_sync_seed(&destination, "local-owner", decoded)
                .await
                .unwrap(),
            space_id
        );
        let actual = make_sync_seed(&destination, &space_id, "local-owner")
            .await
            .unwrap();
        let mut expected = seed.clone();
        expected.snapshot.exported_at = actual.snapshot.exported_at.clone();
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            serde_json::to_value(expected).unwrap()
        );
        assert!(matches!(
            require_member(&space_id, "source-owner", &destination).await,
            Err(AppError::Forbidden)
        ));
        for id in [&space_id, "colliding-space"] {
            let mut overlapping = seed.clone();
            overlapping.space_id = id.to_owned();
            assert!(matches!(
                install_sync_seed(&destination, "local-owner", overlapping).await,
                Err(AppError::InvalidInput(_))
            ));
        }
        let existing = sqlx::query_file!("queries/spaces/get_space_name.sql", "colliding-space")
            .fetch_optional(&destination)
            .await
            .unwrap();
        assert!(existing.is_none());
        destination.close().await;
    }

    #[tokio::test]
    #[ignore = "requires two machines and PLINTH_PHYSICAL_SYNC_* settings"]
    async fn exchange_bootstrapped_space_across_devices() {
        let directory = std::path::PathBuf::from(env::var_os("PLINTH_PHYSICAL_SYNC_DIR").unwrap());
        let role = env::var("PLINTH_PHYSICAL_SYNC_ROLE").unwrap();
        let identity_dir = directory.join("peer");
        let seed_path = directory.join("space-seed.json");
        if role == "prepare" {
            Identity::init(&identity_dir).unwrap();
            fs::write(seed_path, serde_json::to_vec(&sample_seed().await).unwrap()).unwrap();
            return;
        }

        let seed: SyncSeed = serde_json::from_slice(&fs::read(seed_path).unwrap()).unwrap();
        let profile = tempfile::tempdir_in(&directory).unwrap();
        let (preview, db) = open_preview(profile.path()).await;
        let space_id = install_sync_seed(&db, "local-owner", seed).await.unwrap();
        let address = env::var("PLINTH_PHYSICAL_SYNC_RELAY").unwrap();
        let addresses = resolve_peer(address).await.unwrap();
        let exchange = || {
            preview.exchange_peer(
                &db,
                space_id.clone(),
                Identity::load(&identity_dir).unwrap(),
                PeerConnection::Connect(addresses.clone()),
            )
        };

        match role.as_str() {
            "publish" => {
                let patch =
                    serde_json::from_value(serde_json::json!({ "note": OFFLINE_NOTE })).unwrap();
                update_in_preview(&db, &preview, &space_id, RECORD, patch)
                    .await
                    .unwrap();
                exchange().await.unwrap();
                println!("PUBLISHED {space_id} {RECORD}");
            }
            "receive" => {
                let initial = make_sync_seed(&db, &space_id, "local-owner").await.unwrap();
                assert_eq!(initial.snapshot.transactions[0].note, "Paid");
                tokio::time::timeout(Duration::from_secs(300), async {
                    loop {
                        exchange().await.unwrap();
                        let current = make_sync_seed(&db, &space_id, "local-owner").await.unwrap();
                        let row = &current.snapshot.transactions[0];
                        if row.note == OFFLINE_NOTE {
                            assert_eq!(row.id, RECORD);
                            assert_eq!(row.category, "Groceries");
                            assert_eq!(row.approved, 1);
                            println!("RECEIVED {space_id} {RECORD}: {}", row.note);
                            break;
                        }
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                })
                .await
                .expect("timed out waiting for the offline edit");
            }
            _ => panic!("unknown physical sync role: {role}"),
        }
        db.close().await;
    }
}
