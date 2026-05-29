use serde::{Deserialize, Serialize};
use tauri::State;

use crate::{db::DbPool, error::AppError, Session};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AccountSummaryRow {
    pub month: String,
    pub account_id: String,
    pub balance: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AccountSummaryPage {
    pub rows: Vec<AccountSummaryRow>,
    pub page_count: i64,
}

#[tauri::command]
pub async fn list_account_summaries(
    page: i64,
    limit: i64,
    session: State<'_, Session>,
    db: State<'_, DbPool>,
) -> Result<AccountSummaryPage, AppError> {
    let data = session.require()?;

    let total = sqlx::query_file_scalar!(
        "queries/account_summaries/count_account_summaries.sql",
        data.space_id
    )
    .fetch_one(db.inner())
    .await
    .map_err(|e| AppError::Db(format!("list_account_summaries count: {e}")))?;

    let page_count = ((total + limit - 1) / limit).max(1);
    let offset = page * limit;

    let rows = sqlx::query_file_as!(
        AccountSummaryRow,
        "queries/account_summaries/list_account_summaries.sql",
        data.space_id,
        limit,
        offset
    )
    .fetch_all(db.inner())
    .await
    .map_err(|e| AppError::Db(format!("list_account_summaries: {e}")))?;

    Ok(AccountSummaryPage { rows, page_count })
}

#[tauri::command]
pub async fn upsert_account_summary(
    month: String,
    account_id: String,
    balance: i64,
    session: State<'_, Session>,
    db: State<'_, DbPool>,
) -> Result<(), AppError> {
    let data = session.require()?;

    let owned = sqlx::query_file_scalar!(
        "queries/accounts/count_account_in_space.sql",
        account_id,
        data.space_id
    )
    .fetch_one(db.inner())
    .await
    .map_err(|e| AppError::Db(format!("upsert_account_summary ownership: {e}")))?;

    if owned == 0 {
        return Err(AppError::Forbidden);
    }

    sqlx::query_file!(
        "queries/account_summaries/upsert_account_summary.sql",
        month,
        account_id,
        balance
    )
    .execute(db.inner())
    .await
    .map_err(|e| AppError::Db(format!("upsert_account_summary: {e}")))?;

    Ok(())
}

#[tauri::command]
pub async fn delete_account_summary(
    month: String,
    account_id: String,
    session: State<'_, Session>,
    db: State<'_, DbPool>,
) -> Result<(), AppError> {
    let data = session.require()?;

    let owned = sqlx::query_file_scalar!(
        "queries/accounts/count_account_in_space.sql",
        account_id,
        data.space_id
    )
    .fetch_one(db.inner())
    .await
    .map_err(|e| AppError::Db(format!("delete_account_summary ownership: {e}")))?;

    if owned == 0 {
        return Err(AppError::Forbidden);
    }

    sqlx::query_file!(
        "queries/account_summaries/delete_account_summary.sql",
        month,
        account_id
    )
    .execute(db.inner())
    .await
    .map_err(|e| AppError::Db(format!("delete_account_summary: {e}")))?;

    Ok(())
}
