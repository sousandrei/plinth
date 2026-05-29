use serde::{Deserialize, Serialize};
use tauri::State;

use crate::{db::DbPool, error::AppError, Session};

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::FromRow)]
pub struct Account {
    pub id: String,
    pub name: String,
    pub currency: String,
    pub account_type: String,
    pub account_source: String,
    pub color: String,
    pub space_id: String,
}

#[tauri::command]
pub async fn list_accounts(
    session: State<'_, Session>,
    db: State<'_, DbPool>,
) -> Result<Vec<Account>, AppError> {
    let data = session.require()?;

    let accounts =
        sqlx::query_file_as!(Account, "queries/accounts/list_accounts.sql", data.space_id)
            .fetch_all(db.inner())
            .await
            .map_err(|e| AppError::Db(format!("list_accounts: {e}")))?;

    Ok(accounts)
}

#[tauri::command]
pub async fn update_account(
    id: String,
    name: String,
    color: String,
    session: State<'_, Session>,
    db: State<'_, DbPool>,
) -> Result<Account, AppError> {
    if name.trim().is_empty() {
        return Err(AppError::InvalidInput("name cannot be empty".into()));
    }
    if color.trim().is_empty() {
        return Err(AppError::InvalidInput("color cannot be empty".into()));
    }

    let data = session.require()?;
    let name = name.trim().to_string();
    let color = color.trim().to_string();

    let rows = sqlx::query_file!(
        "queries/accounts/update_account.sql",
        name,
        color,
        id,
        data.space_id
    )
    .execute(db.inner())
    .await
    .map_err(|e| AppError::Db(format!("update_account: {e}")))?
    .rows_affected();

    if rows == 0 {
        return Err(AppError::NotFound(format!("account {id}")));
    }

    let account = sqlx::query_file_as!(Account, "queries/accounts/get_account.sql", id)
        .fetch_one(db.inner())
        .await
        .map_err(|e| AppError::Db(format!("update_account fetch: {e}")))?;

    Ok(account)
}
