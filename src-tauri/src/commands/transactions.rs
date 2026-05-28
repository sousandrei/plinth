use serde::{Deserialize, Serialize};
use sqlx::QueryBuilder;
use tauri::State;

use crate::{db::DbPool, error::AppError};

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
    pub user_id: String,
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

fn build_filter_query(prefix: &str, params: &ListTransactionsParams) -> QueryBuilder<sqlx::Sqlite> {
    let mut builder = QueryBuilder::new(prefix);
    builder.push_bind(&params.user_id);

    if let Some(ref search) = params.search {
        if !search.trim().is_empty() {
            let term = format!("\"{}\"*", search.trim().replace('"', "\"\""));
            builder.push(
                " AND t.id IN (SELECT id FROM transactions_fts WHERE transactions_fts MATCH ",
            );
            builder.push_bind(term);
            builder.push(")");
        }
    }

    if let Some(approved) = params.approved {
        builder.push(" AND t.approved = ");
        builder.push_bind(approved);
    }

    if let Some(ref category) = params.category {
        builder.push(" AND t.category = ");
        builder.push_bind(category);
    }

    if let Some(ref date_from) = params.date_from {
        builder.push(" AND t.value_date >= ");
        builder.push_bind(date_from);
    }

    if let Some(ref date_to) = params.date_to {
        builder.push(" AND t.value_date <= ");
        builder.push_bind(date_to);
    }

    builder
}

#[tauri::command]
pub async fn list_transactions(
    params: ListTransactionsParams,
    db: State<'_, DbPool>,
) -> Result<TransactionPage, AppError> {
    let page = params.page.unwrap_or(0).max(0);
    let limit = params.limit.unwrap_or(10).clamp(1, 200);
    let offset = page * limit;

    let mut count_builder: QueryBuilder<sqlx::Sqlite> = build_filter_query(
        "SELECT COUNT(*) FROM transactions t INNER JOIN accounts a ON a.id = t.account_id WHERE a.user_id = ",
        &params,
    );
    let total: i64 = count_builder
        .build_query_scalar::<i64>()
        .fetch_one(db.inner())
        .await
        .map_err(|e| AppError::Db(format!("list_transactions count: {e}")))?;

    let mut builder: QueryBuilder<sqlx::Sqlite> = build_filter_query(
        "SELECT t.id, t.booking_date, t.value_date, t.reference, t.text,
                t.currency, t.amount, t.balance, t.approved, t.note, t.category, t.account_id
         FROM transactions t
         INNER JOIN accounts a ON a.id = t.account_id
         WHERE a.user_id = ",
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
    db: State<'_, DbPool>,
) -> Result<(), AppError> {
    let rows = sqlx::query_file!(
        "queries/transactions/update_transaction.sql",
        approved,
        note,
        category,
        id
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
