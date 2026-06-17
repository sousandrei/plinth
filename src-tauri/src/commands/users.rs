use getrandom::fill;
use serde::{Deserialize, Serialize};
use tauri::{Manager, State};
use uuid::Uuid;

use crate::{commands::spaces::create_space_for_user, db::DbPool, error::AppError, Session};

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::FromRow)]
pub struct User {
    pub id: String,
    pub name: String,
    pub has_pin: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[tauri::command]
pub async fn list_users(db: State<'_, DbPool>) -> Result<Vec<User>, AppError> {
    let users = sqlx::query_file_as!(User, "queries/users/list_users.sql")
        .fetch_all(db.inner())
        .await
        .map_err(|e| AppError::Db(format!("list_users: {e}")))?;

    Ok(users)
}

#[tauri::command]
pub async fn create_user(
    name: String,
    db: State<'_, DbPool>,
    session: State<'_, Session>,
) -> Result<User, AppError> {
    if name.trim().is_empty() {
        return Err(AppError::InvalidInput("name cannot be empty".into()));
    }

    let id = Uuid::new_v4().to_string();
    let name = name.trim().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    sqlx::query_file!("queries/users/create_user.sql", id, name, now, now)
        .execute(db.inner())
        .await
        .map_err(|e| AppError::Db(format!("create_user: {e}")))?;

    let space_name = format!("{name}'s Space");
    let space_id = create_space_for_user(&id, &space_name, db.inner()).await?;

    session.set(id.clone(), Some(space_id));

    let user = sqlx::query_file_as!(User, "queries/users/get_user.sql", id)
        .fetch_one(db.inner())
        .await
        .map_err(|e| AppError::Db(format!("create_user fetch: {e}")))?;

    Ok(user)
}

#[tauri::command]
pub async fn set_pin(user_id: String, pin: String, db: State<'_, DbPool>) -> Result<(), AppError> {
    if pin.len() < 4 {
        return Err(AppError::InvalidInput(
            "PIN must be at least 4 digits".into(),
        ));
    }

    let mut salt = [0u8; 16];
    fill(&mut salt).map_err(|e| AppError::Internal(format!("set_pin rng: {e}")))?;

    let hash = argon2::hash_encoded(pin.as_bytes(), &salt, &argon2::Config::default())
        .map_err(|e| AppError::Internal(format!("set_pin hash: {e}")))?;

    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let rows = sqlx::query_file!("queries/users/set_pin.sql", hash, now, user_id)
        .execute(db.inner())
        .await
        .map_err(|e| AppError::Db(format!("set_pin: {e}")))?
        .rows_affected();

    if rows == 0 {
        return Err(AppError::NotFound(format!("user {user_id}")));
    }

    Ok(())
}

#[tauri::command]
pub async fn verify_pin(
    user_id: String,
    pin: String,
    db: State<'_, DbPool>,
    session: State<'_, Session>,
) -> Result<bool, AppError> {
    let row = sqlx::query_file!("queries/users/verify_pin.sql", user_id)
        .fetch_optional(db.inner())
        .await
        .map_err(|e| AppError::Db(format!("verify_pin: {e}")))?;

    let hash_str = match row {
        None => return Err(AppError::NotFound(format!("user {user_id}"))),
        Some(r) => r.pin_hash.unwrap_or_default(),
    };

    if hash_str.is_empty() {
        return Ok(false);
    }

    let ok = argon2::verify_encoded(&hash_str, pin.as_bytes())
        .map_err(|e| AppError::Internal(format!("verify_pin: {e}")))?;

    if ok {
        let spaces = sqlx::query_file!("queries/spaces/list_spaces_for_user.sql", user_id)
            .fetch_all(db.inner())
            .await
            .map_err(|e| AppError::Db(format!("verify_pin spaces: {e}")))?;

        let auto_space = if spaces.len() == 1 {
            Some(spaces[0].id.clone())
        } else {
            None
        };

        session.set(user_id, auto_space);
    }

    Ok(ok)
}

#[tauri::command]
pub async fn update_user_name(
    user_id: String,
    name: String,
    db: State<'_, DbPool>,
) -> Result<User, AppError> {
    if name.trim().is_empty() {
        return Err(AppError::InvalidInput("name cannot be empty".into()));
    }

    let name = name.trim().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let rows = sqlx::query_file!("queries/users/update_user_name.sql", name, now, user_id)
        .execute(db.inner())
        .await
        .map_err(|e| AppError::Db(format!("update_user_name: {e}")))?
        .rows_affected();

    if rows == 0 {
        return Err(AppError::NotFound(format!("user {user_id}")));
    }

    let user = sqlx::query_file_as!(User, "queries/users/get_user.sql", user_id)
        .fetch_one(db.inner())
        .await
        .map_err(|e| AppError::Db(format!("update_user_name fetch: {e}")))?;

    Ok(user)
}

#[tauri::command]
pub async fn add_app_user(name: String, db: State<'_, DbPool>) -> Result<User, AppError> {
    if name.trim().is_empty() {
        return Err(AppError::InvalidInput("name cannot be empty".into()));
    }

    let id = Uuid::new_v4().to_string();
    let name = name.trim().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    sqlx::query_file!("queries/users/create_app_user.sql", id, name, now, now)
        .execute(db.inner())
        .await
        .map_err(|e| AppError::Db(format!("add_app_user: {e}")))?;

    let user = sqlx::query_file_as!(User, "queries/users/get_user.sql", id)
        .fetch_one(db.inner())
        .await
        .map_err(|e| AppError::Db(format!("add_app_user fetch: {e}")))?;

    Ok(user)
}

#[tauri::command]
pub async fn remove_user(user_id: String, db: State<'_, DbPool>) -> Result<(), AppError> {
    let user_count = sqlx::query_file!("queries/users/count_all_users.sql")
        .fetch_one(db.inner())
        .await
        .map_err(|e| AppError::Db(format!("remove_user count: {e}")))?
        .count;

    if user_count <= 1 {
        return Err(AppError::InvalidInput(
            "cannot remove the only user in the app".into(),
        ));
    }

    let sole_owner_spaces = sqlx::query_file!("queries/users/count_sole_owner_spaces.sql", user_id)
        .fetch_all(db.inner())
        .await
        .map_err(|e| AppError::Db(format!("remove_user sole owner check: {e}")))?;

    if !sole_owner_spaces.is_empty() {
        let space_names: Vec<String> = sole_owner_spaces
            .iter()
            .map(|r| r.space_name.clone())
            .collect();
        return Err(AppError::InvalidInput(format!(
            "user is the sole owner of: {}",
            space_names.join(", ")
        )));
    }

    let rows = sqlx::query_file!("queries/users/remove_user.sql", user_id)
        .execute(db.inner())
        .await
        .map_err(|e| AppError::Db(format!("remove_user: {e}")))?
        .rows_affected();

    if rows == 0 {
        return Err(AppError::NotFound(format!("user {user_id}")));
    }

    Ok(())
}

#[tauri::command]
pub async fn factory_reset(app: tauri::AppHandle, db: State<'_, DbPool>) -> Result<(), AppError> {
    // 1. Close the database pool so connections are terminated
    db.close().await;

    // 2. Resolve database files in the app data directory
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Internal(format!("factory_reset: resolve app path: {e}")))?;

    let db_path = data_dir.join("plinth.db");
    let db_wal = data_dir.join("plinth.db-wal");
    let db_shm = data_dir.join("plinth.db-shm");

    // 3. Delete database files
    if db_path.exists() {
        std::fs::remove_file(db_path)
            .map_err(|e| AppError::Io(format!("factory_reset: delete db: {e}")))?;
    }
    if db_wal.exists() {
        std::fs::remove_file(db_wal)
            .map_err(|e| AppError::Io(format!("factory_reset: delete db-wal: {e}")))?;
    }
    if db_shm.exists() {
        std::fs::remove_file(db_shm)
            .map_err(|e| AppError::Io(format!("factory_reset: delete db-shm: {e}")))?;
    }

    // 4. Restart the app
    app.restart();
}
