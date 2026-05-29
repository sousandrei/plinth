use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::{db::DbPool, error::AppError, Session};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct AggregatedMonth {
    pub by_category: HashMap<String, i64>,
    pub balance: HashMap<String, i64>,
}

#[tauri::command]
pub async fn get_aggregations(
    session: State<'_, Session>,
    db: State<'_, DbPool>,
) -> Result<HashMap<String, AggregatedMonth>, AppError> {
    let data = session.require()?;

    let spend_rows = sqlx::query_file!(
        "queries/aggregations/get_category_spend_by_month.sql",
        data.space_id
    )
    .fetch_all(db.inner())
    .await
    .map_err(|e| AppError::Db(format!("get_aggregations spend: {e}")))?;

    let checking_balance_rows = sqlx::query_file!(
        "queries/aggregations/get_checking_balance_by_month.sql",
        data.space_id
    )
    .fetch_all(db.inner())
    .await
    .map_err(|e| AppError::Db(format!("get_aggregations checking balance: {e}")))?;

    let summary_rows = sqlx::query_file!(
        "queries/aggregations/get_account_summaries.sql",
        data.space_id
    )
    .fetch_all(db.inner())
    .await
    .map_err(|e| AppError::Db(format!("get_aggregations summaries: {e}")))?;

    let mut months: HashMap<String, AggregatedMonth> = HashMap::new();

    for row in spend_rows {
        let entry = months.entry(row.month).or_default();
        *entry.by_category.entry(row.category).or_insert(0) += row.amount;
    }

    for row in checking_balance_rows {
        let entry = months.entry(row.month).or_default();
        entry.balance.insert(row.account_id, row.balance);
    }

    for row in summary_rows {
        let entry = months.entry(row.month).or_default();
        entry.balance.insert(row.account_id, row.balance);
    }

    Ok(months)
}
