use std::future::Future;
use std::pin::Pin;

use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::error::AppError;

/// A boxed future returned by the closure passed to `run_as_device`.
/// Type-erasing the return like this lets `async move { ... }` blocks
/// compose cleanly with the `for<'c>` HRTB over the transaction borrow.
pub type GuardedFuture<'c, T> = Pin<Box<dyn Future<Output = Result<T, AppError>> + Send + 'c>>;

/// Run `body` inside a SQLite transaction in which every change_log
/// trigger is suppressed and the `applying_as_device` override is set
/// so any trigger that does fire stamps `peer_device_id`.
///
/// Both `applying_remote = 1` (suppresses all change_log triggers and
/// auto-updated_at triggers) and `applying_as_device = <peer>` are set
/// as the first statements of the transaction and cleared as the last;
/// rollback on any error leaves `app_settings` exactly as it was before
/// the call.
///
/// The caller must insert the incoming change_log row directly (with
/// its original origin_device_id and origin_seq) before materializing
/// the row body, since the triggers will not do it. See Step 29.2.
///
/// The closure receives `&mut Transaction` so its writes happen on the
/// same connection as the override; otherwise the trigger would see
/// the steady-state device_id.
pub async fn run_as_device<F, T>(
    pool: &SqlitePool,
    peer_device_id: &str,
    body: F,
) -> Result<T, AppError>
where
    F: for<'c> FnOnce(&'c mut Transaction<'_, Sqlite>) -> GuardedFuture<'c, T>,
{
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AppError::Db(format!("apply_guard begin: {e}")))?;

    sqlx::query_file!("queries/sync/set_apply_override.sql", peer_device_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Db(format!("apply_guard set override: {e}")))?;
    sqlx::query_file!("queries/sync/set_applying_remote.sql")
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Db(format!("apply_guard set remote: {e}")))?;

    let out = body(&mut tx).await?;

    sqlx::query_file!("queries/sync/clear_apply_override.sql")
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Db(format!("apply_guard clear override: {e}")))?;
    sqlx::query_file!("queries/sync/clear_applying_remote.sql")
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Db(format!("apply_guard clear remote: {e}")))?;

    tx.commit()
        .await
        .map_err(|e| AppError::Db(format!("apply_guard commit: {e}")))?;

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    /// Spin up an in-memory SQLite pool that runs the real migration —
    /// proves the override lookup actually wires through the triggers.
    async fn fresh_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        // Seed the device_id/sync_seq keys the same way db.rs does on
        // first launch. Reusing the production init queries keeps the
        // test surface aligned with real startup behavior.
        let device_id = "local-device";
        sqlx::query_file!("queries/settings/init_device_id.sql", device_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query_file!("queries/settings/init_sync_seq.sql")
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    #[tokio::test]
    async fn override_is_cleared_after_success() {
        let pool = fresh_pool().await;
        run_as_device(&pool, "peer-1", |_tx| {
            Box::pin(async { Ok::<(), AppError>(()) })
        })
        .await
        .unwrap();

        let row = sqlx::query_file!("queries/tests/get_apply_override.sql")
            .fetch_optional(&pool)
            .await
            .unwrap();
        assert!(row.is_none(), "override leaked after successful apply");
    }

    #[tokio::test]
    async fn override_is_cleared_after_failure() {
        let pool = fresh_pool().await;
        let result: Result<(), AppError> = run_as_device(&pool, "peer-2", |_tx| {
            Box::pin(async { Err(AppError::Internal("boom".into())) })
        })
        .await;
        assert!(result.is_err());

        let row = sqlx::query_file!("queries/tests/get_apply_override.sql")
            .fetch_optional(&pool)
            .await
            .unwrap();
        assert!(row.is_none(), "override leaked after rolled-back apply");
    }

    #[tokio::test]
    async fn writes_inside_guard_do_not_echo() {
        let pool = fresh_pool().await;

        // Insert a space without the guard — triggers fire normally,
        // creating a change_log row attributed to the local device.
        let ts = "2024-01-01T00:00:00Z";
        sqlx::query_file!(
            "queries/tests/insert_space_fixture.sql",
            "s1",
            "local space",
            ts,
            ts
        )
        .execute(&pool)
        .await
        .unwrap();

        // Insert a space inside the guard — triggers are suppressed
        // (applying_remote = 1), so no change_log row is created. The
        // space row IS materialized. The caller is responsible for
        // inserting the change_log entry directly if relay is needed.
        run_as_device(&pool, "peer-xyz", |tx| {
            Box::pin(async move {
                let ts = "2024-01-01T00:00:00Z";
                sqlx::query_file!(
                    "queries/tests/insert_space_fixture.sql",
                    "s2",
                    "peer space",
                    ts,
                    ts
                )
                .execute(&mut **tx)
                .await
                .map_err(|e| AppError::Db(e.to_string()))?;
                Ok(())
            })
        })
        .await
        .unwrap();

        let rows = sqlx::query_file!("queries/tests/list_spaces_change_log.sql")
            .fetch_all(&pool)
            .await
            .unwrap();

        // Only the local write created a change_log row; the guarded
        // write was suppressed.
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].row_id, "s1");
        assert_eq!(rows[0].device_id, "local-device");

        // But the space row WAS materialized.
        let s2_exists: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM spaces WHERE id = 's2'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(s2_exists, 1, "guarded write must materialize the row");
    }
}
