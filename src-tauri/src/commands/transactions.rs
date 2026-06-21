use serde::{Deserialize, Serialize};
use sqlx::QueryBuilder;
use tauri::State;

use crate::{Session, db::DbPool, error::AppError};

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::FromRow)]
pub struct Transaction {
    pub id: String,
    pub booking_date: String,
    pub value_date: String,
    pub reference: String,
    pub text: String,
    pub currency: String,
    pub amount: i64,
    pub balance: i64,
    pub approved: bool,
    pub note: String,
    pub category: String,
    pub account_id: String,
}

#[derive(Debug, Deserialize)]
pub struct ListTransactionsParams {
    pub page: Option<i64>,
    pub limit: Option<i64>,
    pub search: Option<String>,
    pub approved: Option<bool>,
    pub category: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TransactionPage {
    pub transactions: Vec<Transaction>,
    pub page_count: i64,
}

fn build_filter_query(
    prefix: &str,
    space_id: &str,
    params: &ListTransactionsParams,
) -> QueryBuilder<sqlx::Sqlite> {
    let mut builder = QueryBuilder::new(prefix.to_owned());
    builder.push_bind(space_id.to_owned());

    if let Some(ref search) = params.search
        && !search.trim().is_empty()
    {
        let term = format!("\"{}\"*", search.trim().replace('"', "\"\""));
        builder.push(" AND t.id IN (SELECT id FROM transactions_fts WHERE transactions_fts MATCH ");
        builder.push_bind(term);
        builder.push(")");
    }

    if let Some(approved) = params.approved {
        builder.push(" AND t.approved = ");
        builder.push_bind(approved);
    }

    if let Some(ref category) = params.category {
        builder.push(" AND t.category = ");
        builder.push_bind(category.clone());
    }

    if let Some(ref date_from) = params.date_from {
        builder.push(" AND t.value_date >= ");
        builder.push_bind(date_from.clone());
    }

    if let Some(ref date_to) = params.date_to {
        builder.push(" AND t.value_date <= ");
        builder.push_bind(date_to.clone());
    }

    builder
}

#[tauri::command]
pub async fn list_transactions(
    params: ListTransactionsParams,
    session: State<'_, Session>,
    db: State<'_, DbPool>,
) -> Result<TransactionPage, AppError> {
    let data = session.require()?;
    let page = params.page.unwrap_or(0).max(0);
    let limit = params.limit.unwrap_or(10).clamp(1, 200);
    let offset = page * limit;

    let mut count_builder = build_filter_query(
        "SELECT COUNT(*) FROM transactions t INNER JOIN accounts a ON a.id = t.account_id WHERE a.space_id = ",
        &data.space_id,
        &params,
    );
    let total: i64 = count_builder
        .build_query_scalar::<i64>()
        .fetch_one(db.inner())
        .await
        .map_err(|e| AppError::Db(format!("list_transactions count: {e}")))?;

    let mut builder = build_filter_query(
        "SELECT t.id, t.booking_date, t.value_date, t.reference, t.text,
                t.currency, t.amount, t.balance, t.approved, t.note,
                COALESCE(t.category, '') AS category, t.account_id
         FROM transactions t
         INNER JOIN accounts a ON a.id = t.account_id
         WHERE a.space_id = ",
        &data.space_id,
        &params,
    );

    builder.push(" ORDER BY t.value_date DESC, t.text ASC LIMIT ");
    builder.push_bind(limit);
    builder.push(" OFFSET ");
    builder.push_bind(offset);

    let transactions = builder
        .build_query_as::<Transaction>()
        .fetch_all(db.inner())
        .await
        .map_err(|e| AppError::Db(format!("list_transactions: {e}")))?;

    let page_count = (total + limit - 1) / limit;

    Ok(TransactionPage {
        transactions,
        page_count,
    })
}

#[tauri::command]
pub async fn update_transaction(
    id: String,
    approved: bool,
    note: String,
    category: Option<String>,
    session: State<'_, Session>,
    db: State<'_, DbPool>,
) -> Result<(), AppError> {
    let data = session.require()?;

    let rows = sqlx::query_file!(
        "queries/transactions/update_transaction.sql",
        approved,
        note,
        category,
        id,
        data.space_id
    )
    .execute(db.inner())
    .await
    .map_err(|e| AppError::Db(format!("update_transaction: {e}")))?
    .rows_affected();

    if rows == 0 {
        return Err(AppError::NotFound(format!("transaction {id}")));
    }

    Ok(())
}

#[tauri::command]
pub async fn bulk_approve_transactions(
    ids: Vec<String>,
    approved: bool,
    session: State<'_, Session>,
    db: State<'_, DbPool>,
) -> Result<u64, AppError> {
    let data = session.require()?;
    let ids_json = serde_json::to_string(&ids)
        .map_err(|e| AppError::Internal(format!("bulk_approve_transactions: {e}")))?;

    let rows = sqlx::query_file!(
        "queries/transactions/bulk_approve.sql",
        approved,
        ids_json,
        data.space_id
    )
    .execute(db.inner())
    .await
    .map_err(|e| AppError::Db(format!("bulk_approve_transactions: {e}")))?
    .rows_affected();

    Ok(rows)
}

#[tauri::command]
pub async fn bulk_categorize_transactions(
    ids: Vec<String>,
    category: Option<String>,
    session: State<'_, Session>,
    db: State<'_, DbPool>,
) -> Result<u64, AppError> {
    let data = session.require()?;
    let ids_json = serde_json::to_string(&ids)
        .map_err(|e| AppError::Internal(format!("bulk_categorize_transactions: {e}")))?;

    let rows = sqlx::query_file!(
        "queries/transactions/bulk_categorize.sql",
        category,
        ids_json,
        data.space_id
    )
    .execute(db.inner())
    .await
    .map_err(|e| AppError::Db(format!("bulk_categorize_transactions: {e}")))?
    .rows_affected();

    Ok(rows)
}
