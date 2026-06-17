use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

use crate::{db::DbPool, error::AppError, Session};

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
    let data = session.require()?;

    let is_member = sqlx::query_file!("queries/spaces/get_member_role.sql", space_id, data.user_id)
        .fetch_optional(db.inner())
        .await
        .map_err(|e| AppError::Db(format!("set_active_space membership check: {e}")))?
        .is_some();

    if !is_member {
        return Err(AppError::Forbidden);
    }

    session.set_space(space_id);
    Ok(())
}

#[tauri::command]
pub async fn logout(session: State<'_, Session>) -> Result<(), AppError> {
    session.clear();
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

    session.clear_space();
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
    let mut tx = db.inner().begin().await
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

    tx.commit().await
        .map_err(|e| AppError::Db(format!("delete_space commit: {e}")))?;

    session.clear_space();
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
        session.clear_space();
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
