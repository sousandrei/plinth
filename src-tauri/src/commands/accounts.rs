use serde::{Deserialize, Serialize};
use tauri::State;

use crate::{Session, db::DbPool, error::AppError, sync::debounce::DebounceSender};

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
    debounce: State<'_, DebounceSender>,
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

    debounce.notify_mutation();
    Ok(account)
}

#[tauri::command]
pub async fn delete_account(
    id: String,
    session: State<'_, Session>,
    db: State<'_, DbPool>,
    debounce: State<'_, DebounceSender>,
) -> Result<(), AppError> {
    let data = session.require()?;

    // Verify the account belongs to the active space before touching anything.
    let exists = sqlx::query_file_as!(Account, "queries/accounts/get_account.sql", id)
        .fetch_optional(db.inner())
        .await
        .map_err(|e| AppError::Db(format!("delete_account fetch: {e}")))?;

    let account = exists.ok_or_else(|| AppError::NotFound(format!("account {id}")))?;
    if account.space_id != data.space_id {
        return Err(AppError::NotFound(format!("account {id}")));
    }

    // The change_log triggers on transactions and account_summaries resolve
    // space_id by looking up accounts.id at trigger-fire time.  If we let the
    // FK cascade do the deletions the account row is already gone, so the
    // subquery returns NULL and violates the NOT NULL constraint.  Delete
    // children first (while the parent row still exists), then the account.
    let mut tx = db
        .inner()
        .begin()
        .await
        .map_err(|e| AppError::Db(format!("delete_account begin tx: {e}")))?;

    sqlx::query_file!("queries/accounts/delete_account_transactions.sql", id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Db(format!("delete_account_transactions: {e}")))?;

    sqlx::query_file!("queries/accounts/delete_account_summaries.sql", id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Db(format!("delete_account_summaries: {e}")))?;

    sqlx::query_file!("queries/accounts/delete_account.sql", id, data.space_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Db(format!("delete_account: {e}")))?;

    tx.commit()
        .await
        .map_err(|e| AppError::Db(format!("delete_account commit: {e}")))?;

    debounce.notify_mutation();
    Ok(())
}
