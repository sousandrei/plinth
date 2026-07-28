use sqlx::SqlitePool;

use crate::error::AppError;

/// Run the safe GC passes only. Called after every successful outbound
/// sync session and after `delete_space`.
///
/// Disabled passes:
///   - `compact`, `all_peers_consumed`, `cap_90_days` — can discard
///     changes that offline peers have not yet consumed. Replaced by
///     acknowledgment-based collection in Step 29.7.
///   - `orphan_users` — deleting a local profile solely because it has
///     no current `space_members` row is a user-facing policy decision,
///     not automatic cleanup. Explicit local profile deletion is
///     defined in Phase 32. See `data/PLAN.md` Step 28.3.
///
/// Safe passes:
///   1. Deleted spaces       — hard-delete soft-deleted space skeletons
///   2. Orphan cleanup       — remove space_devices for deleted spaces
///   3. Collect acked        — delete change_log rows acknowledged by
///      all required devices; update origin_state.retained_floor
pub async fn run(db: &SqlitePool) -> Result<(), AppError> {
    deleted_spaces(db).await?;
    orphan_space_devices(db).await?;
    collect_acked(db).await?;
    Ok(())
}

#[cfg(test)]
async fn compact(db: &SqlitePool) -> Result<(), AppError> {
    sqlx::query_file!("queries/sync/gc_compact.sql")
        .execute(db)
        .await
        .map_err(|e| AppError::Db(format!("gc_compact: {e}")))?;
    Ok(())
}

#[cfg(test)]
async fn all_peers_consumed(db: &SqlitePool) -> Result<(), AppError> {
    sqlx::query_file!("queries/sync/gc_all_peers_consumed.sql")
        .execute(db)
        .await
        .map_err(|e| AppError::Db(format!("gc_all_peers_consumed: {e}")))?;
    Ok(())
}

#[cfg(test)]
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

async fn orphan_space_devices(db: &SqlitePool) -> Result<(), AppError> {
    sqlx::query_file!("queries/sync/gc_orphan_space_devices.sql")
        .execute(db)
        .await
        .map_err(|e| AppError::Db(format!("gc_orphan_space_devices: {e}")))?;
    Ok(())
}

/// Delete change_log rows acknowledged by every required active
/// device (plus any pending-revocation target), and update
/// origin_state.retained_floor in the same transaction. See
/// data/PLAN.md Step 29.7.
async fn collect_acked(db: &SqlitePool) -> Result<(), AppError> {
    let mut tx = db
        .begin()
        .await
        .map_err(|e| AppError::Db(format!("gc_collect begin: {e}")))?;

    sqlx::query_file!("queries/sync/gc_ensure_origin_state.sql")
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Db(format!("gc_ensure_origin_state: {e}")))?;

    sqlx::query_file!("queries/sync/gc_collect_acked.sql")
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Db(format!("gc_collect_acked: {e}")))?;

    sqlx::query_file!("queries/sync/gc_update_origin_state.sql")
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Db(format!("gc_update_origin_state: {e}")))?;

    tx.commit()
        .await
        .map_err(|e| AppError::Db(format!("gc_collect commit: {e}")))?;
    Ok(())
}

#[cfg(test)]
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

    /// Orphan space_devices rows (space deleted) should be removed.
    #[tokio::test]
    async fn orphan_space_devices_removes_orphans() {
        let pool = fresh_pool().await;

        let s1 = "s1";
        let ts = "2024-01-01T00:00:00Z";
        sqlx::query_file!("queries/tests/insert_space_fixture.sql", s1, "test", ts, ts)
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query!("INSERT INTO devices (device_id, cert_pem, display_name) VALUES ('dev-1', 'CERT', 'my device')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query!(
            "INSERT INTO space_devices (space_id, device_id, sync_enabled, paired_at) \
             VALUES ('s1', 'dev-1', 1, ?1)",
            ts
        )
        .execute(&pool)
        .await
        .unwrap();

        // Temporarily disable FK enforcement to simulate an orphan
        // (in production, space deletion must call gc_orphan_space_devices
        // BEFORE deleting the space, to emit change_log entries).
        sqlx::query!("PRAGMA foreign_keys = OFF")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query!("DELETE FROM spaces WHERE id = 's1'")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query!("PRAGMA foreign_keys = ON")
            .execute(&pool)
            .await
            .unwrap();

        let before: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM space_devices")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(before, 1, "space_devices row exists before GC");

        orphan_space_devices(&pool).await.unwrap();

        let after: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM space_devices")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(after, 0, "orphan space_devices should be removed");
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
    /// space is soft-deleted, child data gone, space_devices preserved,
    /// change_log entries survive (including the manual space-delete entry).
    #[tokio::test]
    async fn delete_space_sequence_preserves_changelog_and_space_devices() {
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
        sqlx::query!("INSERT INTO devices (device_id, cert_pem, display_name) VALUES ('peer-1', 'CERT', 'Peer')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query!(
            "INSERT INTO space_devices (space_id, device_id, sync_enabled, paired_at) \
             VALUES (?1, 'peer-1', 1, ?2)",
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

        let sd: i64 =
            sqlx::query_scalar!("SELECT COUNT(*) FROM space_devices WHERE space_id = 's1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            sd, 1,
            "space_devices must be preserved for sync propagation"
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
