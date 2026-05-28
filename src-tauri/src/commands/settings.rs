use tauri::State;

use crate::{db::DbPool, error::AppError};

#[tauri::command]
pub async fn get_app_setting(
    key: String,
    db: State<'_, DbPool>,
) -> Result<Option<String>, AppError> {
    let row = sqlx::query_file!("queries/settings/get_setting.sql", key)
        .fetch_optional(&*db)
        .await
        .map_err(|e| AppError::Db(format!("get_app_setting: {e}")))?;

    Ok(row.map(|r| r.value))
}

#[tauri::command]
pub async fn set_app_setting(
    key: String,
    value: String,
    db: State<'_, DbPool>,
) -> Result<(), AppError> {
    sqlx::query_file!("queries/settings/set_setting.sql", key, value)
        .execute(&*db)
        .await
        .map_err(|e| AppError::Db(format!("set_app_setting: {e}")))?;

    Ok(())
}
