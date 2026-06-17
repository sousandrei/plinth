use sqlx::SqlitePool;

use crate::error::AppError;

/// Run all six GC passes in order. Called after every successful outbound
/// sync session. Safe to call concurrently — each pass is a single DELETE
/// statement that SQLite serialises internally through WAL.
///
/// Passes:
///   1. Compaction        — keep only the highest-seq row per (space, table, row_id)
///   2. All-consumed      — drop rows every enabled peer has already applied
///   3. 90-day hard cap   — drop rows older than 90 days unconditionally
///   4. Deleted spaces    — hard-delete soft-deleted space skeletons with no
///      remaining change_log entries
///   5. Orphan cleanup    — remove trusted_devices / evicted_devices for deleted spaces
///   6. Orphan users      — remove users with no remaining space_members rows
pub async fn run(db: &SqlitePool) -> Result<(), AppError> {
    compact(db).await?;
    all_peers_consumed(db).await?;
    cap_90_days(db).await?;
    deleted_spaces(db).await?;
    orphan_trusted_devices(db).await?;
    orphan_users(db).await?;
    Ok(())
}

async fn compact(db: &SqlitePool) -> Result<(), AppError> {
    sqlx::query_file!("queries/sync/gc_compact.sql")
        .execute(db)
        .await
        .map_err(|e| AppError::Db(format!("gc_compact: {e}")))?;
    Ok(())
}

async fn all_peers_consumed(db: &SqlitePool) -> Result<(), AppError> {
    sqlx::query_file!("queries/sync/gc_all_peers_consumed.sql")
        .execute(db)
        .await
        .map_err(|e| AppError::Db(format!("gc_all_peers_consumed: {e}")))?;
    Ok(())
}

async fn cap_90_days(db: &SqlitePool) -> Result<(), AppError> {
    sqlx::query_file!("queries/sync/gc_90day_cap.sql")
        .execute(db)
        .await
        .map_err(|e| AppError::Db(format!("gc_90day_cap: {e}")))?;
    Ok(())
}

async fn deleted_spaces(db: &SqlitePool) -> Result<(), AppError> {
    sqlx::query_file!("queries/sync/gc_deleted_spaces.sql")
        .execute(db)
        .await
        .map_err(|e| AppError::Db(format!("gc_deleted_spaces: {e}")))?;
    Ok(())
}

async fn orphan_trusted_devices(db: &SqlitePool) -> Result<(), AppError> {
    sqlx::query_file!("queries/sync/gc_orphan_trusted_devices.sql")
        .execute(db)
        .await
        .map_err(|e| AppError::Db(format!("gc_orphan_trusted_devices: {e}")))?;
    Ok(())
}

async fn orphan_users(db: &SqlitePool) -> Result<(), AppError> {
    sqlx::query_file!("queries/sync/gc_orphan_users.sql")
        .execute(db)
        .await
        .map_err(|e| AppError::Db(format!("gc_orphan_users: {e}")))?;
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

    /// Seed a space + two change_log rows for the same logical row,
    /// simulating two successive updates. After compaction only the
    /// highest-seq row should remain.
    #[tokio::test]
    async fn compaction_keeps_only_latest_row() {
        let pool = fresh_pool().await;
        let ts = "2024-01-01T00:00:00Z";

        // Insert two spaces to get two change_log rows for the same row_id.
        // We use space 's1' and update it twice via the trigger by inserting
        // then re-inserting (the trigger fires for any write).
        let s1 = "s1";
        let name1 = "first";
        let name2 = "second";
        sqlx::query_file!("queries/tests/insert_space_fixture.sql", s1, name1, ts, ts)
            .execute(&pool)
            .await
            .unwrap();
        // A second change_log entry for the same row_id via an UPDATE trigger.
        sqlx::query!(
            "UPDATE spaces SET name = ?1, updated_at = ?2 WHERE id = ?3",
            name2,
            ts,
            s1
        )
        .execute(&pool)
        .await
        .unwrap();

        let before: i64 =
            sqlx::query_scalar!("SELECT COUNT(*) FROM change_log WHERE row_id = 's1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(before >= 2, "expected at least 2 rows before compaction (got {before})");

        compact(&pool).await.unwrap();

        let after: i64 =
            sqlx::query_scalar!("SELECT COUNT(*) FROM change_log WHERE row_id = 's1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(after, 1, "expected 1 row after compaction");
    }

    /// 90-day cap should delete old rows regardless of cursor state.
    #[tokio::test]
    async fn cap_90_days_removes_old_rows() {
        let pool = fresh_pool().await;

        // Insert a space — triggers create a change_log row with changed_at = now.
        let ts = "2024-01-01T00:00:00Z";
        let s1 = "s1";
        sqlx::query_file!("queries/tests/insert_space_fixture.sql", s1, "test", ts, ts)
            .execute(&pool)
            .await
            .unwrap();

        // Backdate the change_log row to 91 days ago.
        sqlx::query!(
            "UPDATE change_log SET changed_at = datetime('now', '-91 days') WHERE row_id = 's1'"
        )
        .execute(&pool)
        .await
        .unwrap();

        let before: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM change_log")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(before > 0);

        cap_90_days(&pool).await.unwrap();

        let after: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM change_log")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(after, 0);
    }

    /// All-peers-consumed should not delete rows when no trusted peers exist.
    #[tokio::test]
    async fn all_peers_consumed_no_peers_is_noop() {
        let pool = fresh_pool().await;
        let ts = "2024-01-01T00:00:00Z";
        let s1 = "s1";
        sqlx::query_file!("queries/tests/insert_space_fixture.sql", s1, "test", ts, ts)
            .execute(&pool)
            .await
            .unwrap();

        let before: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM change_log")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(before > 0);

        all_peers_consumed(&pool).await.unwrap();

        let after: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM change_log")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(after, before, "GC should not run with no trusted peers");
    }

    /// Orphan trusted_devices rows (space deleted) should be removed.
    #[tokio::test]
    async fn orphan_trusted_devices_removes_orphans() {
        let pool = fresh_pool().await;

        // Insert a space and a trusted device for it.
        let s1 = "s1";
        let ts = "2024-01-01T00:00:00Z";
        sqlx::query_file!("queries/tests/insert_space_fixture.sql", s1, "test", ts, ts)
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query!(
            "INSERT INTO trusted_devices (id, space_id, device_id, display_name, cert_pem, sync_enabled, paired_at)
             VALUES ('dev-1', 's1', 'dev-1', 'my device', 'CERT', 1, ?1)",
            ts
        )
        .execute(&pool)
        .await
        .unwrap();

        // Delete the space — trusted_devices row becomes orphaned.
        sqlx::query!("DELETE FROM spaces WHERE id = 's1'")
            .execute(&pool)
            .await
            .unwrap();

        let before: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM trusted_devices")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(before, 1, "trusted_devices row exists before GC");

        orphan_trusted_devices(&pool).await.unwrap();

        let after: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM trusted_devices")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(after, 0, "orphan trusted_devices should be removed");
    }
}
