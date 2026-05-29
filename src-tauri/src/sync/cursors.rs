use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::error::AppError;

/// Returns how far we have applied `peer_device_id`'s change_log for
/// `space_id`. Returns 0 when no cursor row exists yet, which is the
/// semantic "we have never synced from this peer in this space" —
/// callers ship that as `last_seq = 0` to request the full backlog.
pub async fn get(
    db: &SqlitePool,
    space_id: &str,
    peer_device_id: &str,
) -> Result<i64, AppError> {
    let row = sqlx::query_file!("queries/sync/get_cursor.sql", space_id, peer_device_id)
        .fetch_optional(db)
        .await
        .map_err(|e| AppError::Db(format!("cursors::get: {e}")))?;
    Ok(row.map(|r| r.last_seq).unwrap_or(0))
}

/// Advance our cursor for `(space_id, peer_device_id)` to `last_seq`.
/// The SQL clamps with `MAX(...)` so an out-of-order batch can never
/// regress progress — the cursor only ever moves forward.
///
/// Takes `&mut Transaction` so the cursor write happens atomically
/// with the change application inside the same `apply_guard` tx. If
/// the apply fails and rolls back, the cursor advance rolls back too,
/// which is exactly what we want.
pub async fn advance(
    tx: &mut Transaction<'_, Sqlite>,
    space_id: &str,
    peer_device_id: &str,
    last_seq: i64,
) -> Result<(), AppError> {
    sqlx::query_file!(
        "queries/sync/upsert_cursor.sql",
        space_id,
        peer_device_id,
        last_seq
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Db(format!("cursors::advance: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn fresh_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        // The change_log triggers fired by any synced-table write read
        // `sync_seq` from `app_settings`; seed both keys the same way
        // `db::init_sync_settings` does on first launch.
        let device_id = "local-device";
        sqlx::query_file!("queries/settings/init_device_id.sql", device_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query_file!("queries/settings/init_sync_seq.sql")
            .execute(&pool)
            .await
            .unwrap();
        // sync_cursors has an FK to spaces — seed a parent row first.
        let id = "s1";
        let name = "test";
        let ts = "2024-01-01T00:00:00Z";
        sqlx::query_file!("queries/tests/insert_space_fixture.sql", id, name, ts, ts)
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    #[tokio::test]
    async fn get_returns_zero_when_absent() {
        let pool = fresh_pool().await;
        let n = get(&pool, "s1", "peer-1").await.unwrap();
        assert_eq!(n, 0);
    }

    #[tokio::test]
    async fn advance_and_get_round_trip() {
        let pool = fresh_pool().await;
        let mut tx = pool.begin().await.unwrap();
        advance(&mut tx, "s1", "peer-1", 42).await.unwrap();
        tx.commit().await.unwrap();

        let n = get(&pool, "s1", "peer-1").await.unwrap();
        assert_eq!(n, 42);
    }

    #[tokio::test]
    async fn advance_never_regresses() {
        let pool = fresh_pool().await;
        let mut tx = pool.begin().await.unwrap();
        advance(&mut tx, "s1", "peer-1", 100).await.unwrap();
        // An out-of-order older batch tries to set the cursor to 50.
        advance(&mut tx, "s1", "peer-1", 50).await.unwrap();
        tx.commit().await.unwrap();

        let n = get(&pool, "s1", "peer-1").await.unwrap();
        assert_eq!(n, 100, "cursor regressed despite MAX clamp");
    }

    #[tokio::test]
    async fn cursors_are_per_peer_and_per_space() {
        let pool = fresh_pool().await;
        // Second space.
        let id = "s2";
        let name = "other";
        let ts = "2024-01-01T00:00:00Z";
        sqlx::query_file!("queries/tests/insert_space_fixture.sql", id, name, ts, ts)
            .execute(&pool)
            .await
            .unwrap();

        let mut tx = pool.begin().await.unwrap();
        advance(&mut tx, "s1", "peer-A", 10).await.unwrap();
        advance(&mut tx, "s1", "peer-B", 20).await.unwrap();
        advance(&mut tx, "s2", "peer-A", 30).await.unwrap();
        tx.commit().await.unwrap();

        assert_eq!(get(&pool, "s1", "peer-A").await.unwrap(), 10);
        assert_eq!(get(&pool, "s1", "peer-B").await.unwrap(), 20);
        assert_eq!(get(&pool, "s2", "peer-A").await.unwrap(), 30);
        assert_eq!(get(&pool, "s2", "peer-B").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn advance_rolls_back_with_its_tx() {
        let pool = fresh_pool().await;
        let mut tx = pool.begin().await.unwrap();
        advance(&mut tx, "s1", "peer-1", 7).await.unwrap();
        // Don't commit — drop rolls back.
        drop(tx);
        assert_eq!(get(&pool, "s1", "peer-1").await.unwrap(), 0);
    }
}
