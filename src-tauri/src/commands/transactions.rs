use serde::{Deserialize, Serialize};
use sqlx::{Executor, QueryBuilder, Sqlite};
use tauri::State;

use crate::{Session, db::DbPool, error::AppError, sync_preview::SyncPreview};

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

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TransactionPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    approved: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<String>,
    #[serde(
        default,
        deserialize_with = "present_category",
        skip_serializing_if = "Option::is_none"
    )]
    category: Option<Option<String>>,
}

fn changed_fields(
    mut patch: TransactionPatch,
    approved: bool,
    note: &str,
    category: Option<&str>,
) -> Result<plinth_sync::Fields, AppError> {
    patch.approved = patch.approved.filter(|value| *value != approved);
    patch.note = patch.note.filter(|value| value != note);
    patch.category = patch
        .category
        .filter(|value| value.as_deref().unwrap_or_default() != category.unwrap_or_default());
    serde_json::to_value(patch)
        .and_then(serde_json::from_value)
        .map_err(|error| AppError::Internal(format!("encode transaction patch: {error}")))
}

pub(crate) fn valid_sync_fields(fields: &plinth_sync::Fields) -> bool {
    fields.iter().all(|(name, value)| match name.as_str() {
        "approved" => value.is_boolean(),
        "note" => value.is_string(),
        "category" => value.is_null() || value.is_string(),
        _ => false,
    })
}

fn present_category<'de, D>(value: D) -> Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    // An explicit null clears the category. An absent field leaves it alone.
    Option::deserialize(value).map(Some)
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
        let cleaned = search.trim();
        if cleaned.chars().count() < 3 {
            let term = format!("%{}%", cleaned);
            builder.push(" AND (t.text LIKE ");
            builder.push_bind(term.clone());
            builder.push(" OR CAST(t.amount AS TEXT) LIKE ");
            builder.push_bind(term.clone());
            builder.push(" OR printf('%.2f', t.amount / 100.0) LIKE ");
            builder.push_bind(term);
            builder.push(")");
        } else {
            let term = format!("\"{}\"", cleaned.replace('"', "\"\""));
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
    patch: TransactionPatch,
    session: State<'_, Session>,
    db: State<'_, DbPool>,
    preview: State<'_, SyncPreview>,
) -> Result<(), AppError> {
    let data = session.require()?;
    if !preview.active() {
        return apply_transaction_patch(db.inner(), &data.space_id, &id, &patch).await;
    }

    let db = db.inner().clone();
    let preview = preview.inner().clone();
    // Keep the database lease through acknowledgement even if the IPC caller disappears.
    tokio::spawn(async move { update_in_preview(&db, &preview, &data.space_id, &id, patch).await })
        .await
        .map_err(|error| AppError::Internal(format!("transaction preview task: {error}")))?
}

async fn update_in_preview(
    db: &DbPool,
    preview: &SyncPreview,
    space_id: &str,
    id: &str,
    patch: TransactionPatch,
) -> Result<(), AppError> {
    let ids = serde_json::to_string(&[id])
        .map_err(|error| AppError::Internal(format!("encode transaction ID: {error}")))?;
    if edit_in_preview(db, preview, space_id, &ids, patch).await? == 0 {
        return Err(AppError::NotFound(format!("transaction {id}")));
    }
    Ok(())
}

async fn edit_in_preview(
    db: &DbPool,
    preview: &SyncPreview,
    space_id: &str,
    ids_json: &str,
    patch: TransactionPatch,
) -> Result<u64, AppError> {
    let mut conn = db.acquire().await?;
    let rows = sqlx::query_file!(
        "queries/transactions/get_transaction_edit_fields.sql",
        ids_json,
        space_id
    )
    .fetch_all(&mut *conn)
    .await?;
    let matched = rows.len() as u64;
    let mut edits = Vec::new();
    for row in rows {
        let fields = changed_fields(
            patch.clone(),
            row.approved,
            &row.note,
            row.category.as_deref(),
        )?;
        if !fields.is_empty() {
            edits.push((row.id, fields));
        }
    }
    if !edits.is_empty() {
        preview.stage_batch(space_id.to_owned(), edits).await?;
        preview.replay(&mut conn).await?;
    }
    Ok(matched)
}

async fn bulk_edit_in_preview(
    db: &DbPool,
    preview: &SyncPreview,
    space_id: String,
    ids_json: String,
    patch: TransactionPatch,
) -> Result<u64, AppError> {
    let db = db.clone();
    let preview = preview.clone();
    tokio::spawn(async move { edit_in_preview(&db, &preview, &space_id, &ids_json, patch).await })
        .await
        .map_err(|error| AppError::Internal(format!("bulk transaction preview task: {error}")))?
}

pub(crate) async fn apply_pending_transaction_edit<'a>(
    db: impl Executor<'a, Database = Sqlite>,
    edit: &plinth_sync::PendingEdit,
) -> Result<(), AppError> {
    let patch = serde_json::to_value(&edit.fields)
        .and_then(serde_json::from_value)
        .map_err(|error| AppError::InvalidInput(format!("transaction patch: {error}")))?;
    match apply_transaction_patch(db, &edit.space_id, &edit.record_id, &patch).await {
        // Account and space deletion can remove the row before recovery.
        Err(AppError::NotFound(_)) => Ok(()),
        result => result,
    }
}

async fn apply_transaction_patch<'a>(
    db: impl Executor<'a, Database = Sqlite>,
    space_id: &str,
    id: &str,
    patch: &TransactionPatch,
) -> Result<(), AppError> {
    let note = patch.note.as_deref();
    let set_category = patch.category.is_some();
    let category = patch.category.as_ref().and_then(|value| value.as_deref());
    let rows = sqlx::query_file!(
        "queries/transactions/update_transaction.sql",
        patch.approved,
        note,
        set_category,
        category,
        id,
        space_id,
    )
    .execute(db)
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
    preview: State<'_, SyncPreview>,
) -> Result<u64, AppError> {
    let data = session.require()?;
    let ids_json = serde_json::to_string(&ids)
        .map_err(|e| AppError::Internal(format!("bulk_approve_transactions: {e}")))?;

    if preview.active() {
        return bulk_edit_in_preview(
            db.inner(),
            preview.inner(),
            data.space_id,
            ids_json,
            TransactionPatch {
                approved: Some(approved),
                note: None,
                category: None,
            },
        )
        .await;
    }

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
    preview: State<'_, SyncPreview>,
) -> Result<u64, AppError> {
    let data = session.require()?;
    let ids_json = serde_json::to_string(&ids)
        .map_err(|e| AppError::Internal(format!("bulk_categorize_transactions: {e}")))?;

    if preview.active() {
        return bulk_edit_in_preview(
            db.inner(),
            preview.inner(),
            data.space_id,
            ids_json,
            TransactionPatch {
                approved: None,
                note: None,
                category: Some(category),
            },
        )
        .await;
    }

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

#[cfg(test)]
#[path = "transaction_sync_tests.rs"]
mod sync_tests;
