use sqlx::SqlitePool;

use crate::error::AppError;
use crate::sync::wire::ChangeRow;

/// Hard ceiling on rows returned by `read_since` in one call. A peer
/// behind by a lot will be fed in successive batches; this keeps any
/// single read bounded so the wire frame stays under
/// `frame::MAX_FRAME_BYTES`.
pub const DEFAULT_BATCH_LIMIT: i64 = 5_000;

/// Read up to `limit` change_log rows that this device authored for
/// `space_id`, with `seq > last_seq`, in ascending `seq` order. The
/// caller streams these to the peer; the peer applies them in the
/// same order and records the new high-water mark.
pub async fn read_since(
    db: &SqlitePool,
    space_id: &str,
    device_id: &str,
    last_seq: i64,
    limit: i64,
) -> Result<Vec<ChangeRow>, AppError> {
    let rows = sqlx::query_file!(
        "queries/sync/list_changes_since.sql",
        space_id,
        device_id,
        last_seq,
        limit
    )
    .fetch_all(db)
    .await
    .map_err(|e| AppError::Db(format!("read_since: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|r| ChangeRow {
            id: r.id,
            space_id: r.space_id,
            table_name: r.table_name,
            row_id: r.row_id,
            operation: r.operation,
            payload: r.payload,
            seq: r.seq,
            device_id: r.device_id,
            changed_at: r.changed_at,
        })
        .collect())
}

/// Highest `seq` currently stored for `(space_id, device_id)`, or 0
/// if the log is empty. Used as `ChangeBatch.final_seq` so the receiver
/// can advance its cursor past compaction gaps even when no rows ship.
pub async fn max_seq(
    db: &SqlitePool,
    space_id: &str,
    device_id: &str,
) -> Result<i64, AppError> {
    let row = sqlx::query_file!("queries/sync/max_change_seq.sql", space_id, device_id)
        .fetch_one(db)
        .await
        .map_err(|e| AppError::Db(format!("max_seq: {e}")))?;
    Ok(row.max_seq)
}

/// Lowest `seq` currently stored for `(space_id, device_id)`, or 0
/// if the log is empty. If a peer's cursor falls below this value the
/// delta path is impossible (rows have been GC'd) and the session must
/// fall back to a full-snapshot transfer — see PLAN.md §7.3.
pub async fn min_seq(
    db: &SqlitePool,
    space_id: &str,
    device_id: &str,
) -> Result<i64, AppError> {
    let row = sqlx::query_file!("queries/sync/min_change_seq.sql", space_id, device_id)
        .fetch_one(db)
        .await
        .map_err(|e| AppError::Db(format!("min_seq: {e}")))?;
    Ok(row.min_seq)
}
