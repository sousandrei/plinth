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
        assert!(
            before >= 2,
            "expected at least 2 rows before compaction (got {before})"
        );

        compact(&pool).await.unwrap();

        let after: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM change_log WHERE row_id = 's1'")
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

    /// The `WHEN NEW.deleted = 0` guard on `change_log_spaces_au` must
    /// suppress the update trigger when `deleted` is flipped to 1.
    #[tokio::test]
    async fn soft_delete_creates_no_update_changelog() {
        let pool = fresh_pool().await;
        let ts = "2024-01-01T00:00:00Z";
        let s1 = "s1";

        sqlx::query_file!("queries/tests/insert_space_fixture.sql", s1, "test", ts, ts)
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query_file!("queries/spaces/soft_delete_space.sql", s1)
            .execute(&pool)
            .await
            .unwrap();

        let updates: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM change_log \
             WHERE table_name = 'spaces' AND operation = 'update' AND row_id = 's1'"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            updates, 0,
            "soft-delete must not create an update changelog entry"
        );

        let deleted: i64 = sqlx::query_scalar!("SELECT deleted FROM spaces WHERE id = 's1'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(deleted, 1);
    }

    /// Replicate the full `delete_space` SQL sequence and verify:
    /// space is soft-deleted, child data gone, trusted_devices preserved,
    /// change_log entries survive (including the manual space-delete entry).
    #[tokio::test]
    async fn delete_space_sequence_preserves_changelog_and_trusted_devices() {
        let pool = fresh_pool().await;
        let ts = "2024-01-01T00:00:00Z";
        let s1 = "s1";
        let u1 = "u1";
        let a1 = "a1";
        let td1 = "td1";

        sqlx::query_file!("queries/tests/insert_space_fixture.sql", s1, "test", ts, ts)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query!("INSERT INTO users (id, name) VALUES (?1, ?2)", u1, "Alice")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query!(
            "INSERT INTO space_members (space_id, user_id, role) VALUES (?1, ?2, 'owner')",
            s1,
            u1
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query!(
            "INSERT INTO accounts (id, name, currency, account_type, account_source, space_id) \
             VALUES (?1, 'Checking', 'SEK', 'checking', 'seb', ?2)",
            a1,
            s1
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query!(
            "INSERT INTO trusted_devices (id, space_id, device_id, display_name, cert_pem, sync_enabled, paired_at) \
             VALUES (?1, ?2, 'peer-1', 'Peer', 'CERT', 1, ?3)",
            td1,
            s1,
            ts
        )
        .execute(&pool)
        .await
            .unwrap();

        let mut tx = pool.begin().await.unwrap();
        sqlx::query_file!("queries/spaces/soft_delete_space.sql", s1)
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query_file!("queries/spaces/delete_space_members.sql", s1)
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query_file!("queries/spaces/delete_space_settings.sql", s1)
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query_file!("queries/spaces/delete_space_categories.sql", s1)
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query_file!("queries/spaces/delete_space_accounts.sql", s1)
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query_file!("queries/spaces/increment_sync_seq.sql")
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query_file!("queries/spaces/insert_space_delete_changelog.sql", s1)
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let deleted: i64 = sqlx::query_scalar!("SELECT deleted FROM spaces WHERE id = 's1'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(deleted, 1);

        let members: i64 =
            sqlx::query_scalar!("SELECT COUNT(*) FROM space_members WHERE space_id = 's1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(members, 0);

        let accounts: i64 =
            sqlx::query_scalar!("SELECT COUNT(*) FROM accounts WHERE space_id = 's1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(accounts, 0);

        let td: i64 =
            sqlx::query_scalar!("SELECT COUNT(*) FROM trusted_devices WHERE space_id = 's1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            td, 1,
            "trusted_devices must be preserved for sync propagation"
        );

        let cl: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM change_log WHERE space_id = 's1'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(
            cl > 0,
            "change_log entries must survive for sync propagation"
        );

        let space_deletes: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM change_log \
             WHERE table_name = 'spaces' AND operation = 'delete' AND row_id = 's1'"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            space_deletes, 1,
            "manual space-delete changelog entry must exist"
        );
    }

    /// `deleted_spaces` GC pass removes skeleton rows when no change_log
    /// entries remain for that space.
    #[tokio::test]
    async fn deleted_spaces_removes_skeleton_without_changelog() {
        let pool = fresh_pool().await;
        let ts = "2024-01-01T00:00:00Z";
        let s1 = "s1";

        sqlx::query_file!("queries/tests/insert_space_fixture.sql", s1, "test", ts, ts)
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query_file!("queries/spaces/soft_delete_space.sql", s1)
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query!("DELETE FROM change_log WHERE space_id = 's1'")
            .execute(&pool)
            .await
            .unwrap();

        deleted_spaces(&pool).await.unwrap();

        let count: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM spaces WHERE id = 's1'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            count, 0,
            "skeleton should be removed when no changelog remains"
        );
    }

    /// `deleted_spaces` GC pass keeps skeleton rows while change_log
    /// entries still exist (peers haven't consumed them yet).
    #[tokio::test]
    async fn deleted_spaces_keeps_skeleton_with_changelog() {
        let pool = fresh_pool().await;
        let ts = "2024-01-01T00:00:00Z";
        let s1 = "s1";

        sqlx::query_file!("queries/tests/insert_space_fixture.sql", s1, "test", ts, ts)
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query_file!("queries/spaces/soft_delete_space.sql", s1)
            .execute(&pool)
            .await
            .unwrap();

        deleted_spaces(&pool).await.unwrap();

        let count: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM spaces WHERE id = 's1'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            count, 1,
            "skeleton should remain while changelog entries exist"
        );
    }

    /// `orphan_users` GC pass removes users with no remaining space_members.
    #[tokio::test]
    async fn orphan_users_removes_users_with_no_memberships() {
        let pool = fresh_pool().await;
        let ts = "2024-01-01T00:00:00Z";
        let s1 = "s1";
        let u1 = "u1";

        sqlx::query_file!("queries/tests/insert_space_fixture.sql", s1, "test", ts, ts)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query!("INSERT INTO users (id, name) VALUES (?1, ?2)", u1, "Alice")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query!(
            "INSERT INTO space_members (space_id, user_id, role) VALUES (?1, ?2, 'owner')",
            s1,
            u1
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query!("DELETE FROM space_members WHERE user_id = ?1", u1)
            .execute(&pool)
            .await
            .unwrap();

        orphan_users(&pool).await.unwrap();

        let count: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM users WHERE id = ?1", u1)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0, "orphaned user should be removed");
    }

    /// `orphan_users` GC pass keeps users who still have memberships.
    #[tokio::test]
    async fn orphan_users_keeps_users_with_memberships() {
        let pool = fresh_pool().await;
        let ts = "2024-01-01T00:00:00Z";
        let s1 = "s1";
        let u1 = "u1";

        sqlx::query_file!("queries/tests/insert_space_fixture.sql", s1, "test", ts, ts)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query!("INSERT INTO users (id, name) VALUES (?1, ?2)", u1, "Alice")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query!(
            "INSERT INTO space_members (space_id, user_id, role) VALUES (?1, ?2, 'owner')",
            s1,
            u1
        )
        .execute(&pool)
        .await
        .unwrap();

        orphan_users(&pool).await.unwrap();

        let count: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM users WHERE id = ?1", u1)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1, "user with membership should be kept");
    }
}
