use plinth_sync::{Fields, PendingEdit};
use serde::{Deserialize, Serialize};
use sqlx::{Connection, SqliteConnection};

use crate::{
    AppError,
    commands::{
        accounts::Account, spaces::ExportTransaction, transactions::apply_pending_transaction_edit,
    },
};

pub(crate) const CREATED: &str = "created";

#[derive(Serialize, Deserialize)]
struct ImportedTransaction {
    account: Account,
    transaction: ExportTransaction,
}

pub(crate) fn encode(
    account: Account,
    transactions: Vec<ExportTransaction>,
) -> Result<Vec<(String, Fields)>, AppError> {
    transactions
        .into_iter()
        .map(|transaction| {
            let id = transaction.id.clone();
            let data = serde_json::to_value(ImportedTransaction {
                account: account.clone(),
                transaction,
            })
            .map_err(|error| AppError::Internal(format!("encode sync import: {error}")))?;
            Ok((id, [(CREATED.into(), data)].into()))
        })
        .collect()
}

pub(crate) fn valid_creation(data: &serde_json::Value) -> bool {
    serde_json::from_value::<ImportedTransaction>(data.clone()).is_ok_and(|imported| {
        !imported.account.id.is_empty()
            && imported.account.id == imported.transaction.account_id
            && !imported.transaction.id.is_empty()
    })
}

pub(crate) async fn project(
    conn: &mut SqliteConnection,
    edit: &PendingEdit,
) -> Result<(), AppError> {
    let Some(data) = edit.fields.get(CREATED) else {
        return apply_pending_transaction_edit(conn, edit).await;
    };
    let imported: ImportedTransaction = serde_json::from_value(data.clone())
        .map_err(|error| AppError::InvalidInput(format!("invalid sync import: {error}")))?;
    let account = imported.account;
    let record = imported.transaction;
    let space = &edit.space_id;
    if account.space_id != *space || record.id != edit.record_id || record.account_id != account.id
    {
        return Ok(());
    }
    let mut tx = conn.begin().await?;
    let existing = sqlx::query_file_as!(Account, "queries/accounts/get_account.sql", account.id)
        .fetch_optional(&mut *tx)
        .await?;
    match existing {
        Some(row) if row.space_id != *space => return Ok(()),
        Some(_) => {}
        None => {
            if sqlx::query_file!("queries/spaces/get_space_name.sql", space)
                .fetch_optional(&mut *tx)
                .await?
                .is_none()
            {
                return Ok(());
            }
            sqlx::query_file!(
                "queries/spaces/import_account.sql",
                account.id,
                account.name,
                account.currency,
                account.account_type,
                account.account_source,
                account.color,
                space
            )
            .execute(&mut *tx)
            .await?;
        }
    }
    sqlx::query_file!(
        "queries/upload/insert_transaction.sql",
        record.id,
        record.booking_date,
        record.value_date,
        record.reference,
        record.text,
        record.currency,
        record.amount,
        record.balance,
        record.category,
        account.id
    )
    .execute(&mut *tx)
    .await?;
    if edit.fields.len() > 1 {
        apply_pending_transaction_edit(&mut *tx, edit).await?;
    }
    tx.commit().await?;
    Ok(())
}
