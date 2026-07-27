#![cfg(test)]

//! Multi-device test harness for the sync engine.
//!
//! Each [`TestDevice`] owns an independent file-backed SQLite database
//! with a deterministic device ID, real migrations, and the same
//! `init_device_id` / `init_sync_seq` seeding the production startup
//! performs. File-backed pools (as opposed to `sqlite::memory:`) let a
//! fixture drop its pool and reopen the same path to exercise restart
//! semantics — identity, cursors, and synchronized data must survive.
//!
//! [`sync_direct`] replicates the production apply path without TLS or
//! an `AppHandle`: it reads one origin's change_log stream from the
//! sender, ships it as a `ChangeBatch`, and applies it on the receiver
//! through `run_as_device` + `apply_change` + `cursors::advance` — the
//! same primitives `session::apply_batch` uses. Relay (A→B→C) is just
//! two `sync_direct` calls; disconnection is modelled by simply not
//! calling it.

use std::path::{Path, PathBuf};

use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use tempfile::TempDir;

use crate::error::AppError;
use crate::sync::apply;
use crate::sync::apply_guard::run_as_device;
use crate::sync::changelog;
use crate::sync::cursors;
use crate::sync::payloads::{SpacePayload, TablePayload};
use crate::sync::wire::{ChangeBatch, ChangeRow};

/// One independent installation in the test mesh.
#[allow(dead_code)]
pub struct TestDevice {
    pub pool: SqlitePool,
    pub device_id: String,
    pub db_path: PathBuf,
}

impl TestDevice {
    /// Create a fresh file-backed database inside `dir`, run migrations,
    /// and seed `device_id` + `sync_seq` the same way `db::setup` does
    /// on first launch.
    pub async fn create(device_id: &str, dir: &TempDir) -> Self {
        let db_path = dir.path().join(format!("{device_id}.db"));
        Self::open_at(device_id, &db_path).await
    }

    /// Reopen an existing database file, retaining all prior state.
    /// Used to prove a restart preserves identity, cursors, and data.
    pub async fn reopen(db_path: &Path) -> Self {
        let device_id = read_device_id_from_path(db_path).await;
        Self::open_at(&device_id, db_path).await
    }

    async fn open_at(device_id: &str, db_path: &Path) -> Self {
        let opts = SqliteConnectOptions::new()
            .filename(db_path)
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .unwrap_or_else(|e| panic!("harness: connect {db_path:?}: {e}"));
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .unwrap_or_else(|e| panic!("harness: migrate: {e}"));
        sqlx::query_file!("queries/settings/init_device_id.sql", device_id)
            .execute(&pool)
            .await
            .unwrap_or_else(|e| panic!("harness: init_device_id: {e}"));
        sqlx::query_file!("queries/settings/init_sync_seq.sql")
            .execute(&pool)
            .await
            .unwrap_or_else(|e| panic!("harness: init_sync_seq: {e}"));
        Self {
            pool,
            device_id: device_id.to_string(),
            db_path: db_path.to_path_buf(),
        }
    }

    /// Insert a space directly (bypassing sync) so local writes produce
    /// change_log rows this device can ship to a peer.
    pub async fn insert_space(&self, id: &str, name: &str) {
        let ts = "2024-01-01T00:00:00Z";
        sqlx::query_file!("queries/tests/insert_space_fixture.sql", id, name, ts, ts)
            .execute(&self.pool)
            .await
            .unwrap_or_else(|e| panic!("harness: insert_space: {e}"));
    }

    /// Read the local device_id back from the database.
    pub async fn device_id(&self) -> String {
        read_device_id(&self.pool).await
    }

    /// Current cursor this device holds for `origin_device_id`'s stream
    /// in `space_id`.
    pub async fn cursor_for(&self, space_id: &str, origin_device_id: &str) -> i64 {
        cursors::get(&self.pool, space_id, origin_device_id)
            .await
            .unwrap_or_else(|e| panic!("harness: cursor_for: {e}"))
    }
}

async fn read_device_id(pool: &SqlitePool) -> String {
    sqlx::query_file_scalar!("queries/settings/get_setting.sql", "device_id")
        .fetch_one(pool)
        .await
        .unwrap_or_else(|e| panic!("harness: read device_id: {e}"))
}

async fn read_device_id_from_path(path: &Path) -> String {
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await
        .unwrap_or_else(|e| panic!("harness: reopen connect: {e}"));
    read_device_id(&pool).await
}

/// Replicate one direction of a sync session: ship every origin stream
/// `from` holds to `to`, applying each through the production
/// `run_as_device` + `apply_change` path and advancing `to`'s receive
/// cursors. Returns the total number of rows applied.
///
/// This mirrors `session::apply_batch` without the `AppHandle` event
/// emission or eviction handling — those are policy concerns layered on
/// later; the ledger harness exists to prove convergence invariants.
pub async fn sync_direct(from: &TestDevice, to: &TestDevice) -> usize {
    let origins = sqlx::query_file!("queries/tests/list_change_origins.sql")
        .fetch_all(&from.pool)
        .await
        .unwrap_or_else(|e| panic!("harness: list origins: {e}"));

    let mut total = 0usize;
    for o in origins {
        let space_id = o.space_id;
        let origin_device = o.device_id;
        let last_seq = cursors::get(&to.pool, &space_id, &origin_device)
            .await
            .unwrap_or_else(|e| panic!("harness: get cursor: {e}"));
        let rows = changelog::read_since(&from.pool, &space_id, &origin_device, last_seq, 5_000)
            .await
            .unwrap_or_else(|e| panic!("harness: read_since: {e}"));
        let final_seq = changelog::max_seq(&from.pool, &space_id, &origin_device)
            .await
            .unwrap_or_else(|e| panic!("harness: max_seq: {e}"));

        let batch = ChangeBatch {
            space_id: space_id.clone(),
            device_id: origin_device.clone(),
            rows,
            final_seq,
        };
        let count = batch.rows.len();
        apply_batch(to, &batch).await;
        total += count;
    }
    total
}

async fn apply_batch(to: &TestDevice, batch: &ChangeBatch) {
    let space_id = batch.space_id.clone();
    let origin_override = batch.device_id.clone();
    let origin = batch.device_id.clone();
    let final_seq = batch.final_seq;
    let rows = batch.rows.clone();

    run_as_device(&to.pool, &origin_override, move |tx| {
        let space_id = space_id.clone();
        let origin = origin.clone();
        let rows = rows.clone();
        Box::pin(async move {
            for row in &rows {
                apply::apply_change(tx, row).await?;
            }
            cursors::advance(tx, &space_id, &origin, final_seq).await?;
            Ok::<(), AppError>(())
        })
    })
    .await
    .unwrap_or_else(|e| panic!("harness: apply_batch: {e}"));
}

/// Convenience: build a `spaces` insert `ChangeRow` authored by
/// `device_id`. Mirrors what the SQLite trigger would produce.
#[allow(dead_code)]
pub fn space_insert_row(
    change_id: &str,
    space_id: &str,
    name: &str,
    device_id: &str,
    seq: i64,
) -> ChangeRow {
    let ts = "2024-01-01T00:00:00Z";
    ChangeRow {
        id: change_id.into(),
        space_id: space_id.into(),
        table_name: "spaces".into(),
        row_id: space_id.into(),
        operation: "insert".into(),
        payload: Some(TablePayload::Space(SpacePayload {
            id: space_id.into(),
            name: name.into(),
            created_at: ts.into(),
            updated_at: ts.into(),
        })),
        seq,
        device_id: device_id.into(),
        changed_at: ts.into(),
    }
}

// ---------------------------------------------------------------------------
// Verification tests for Step 28.1
// ---------------------------------------------------------------------------

/// A baseline local change on A must reach B through `sync_direct`.
#[tokio::test]
async fn baseline_local_change_reaches_second_database() {
    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;
    let b = TestDevice::create("device-B", &dir).await;

    a.insert_space("s1", "shared space").await;
    assert_eq!(sync_direct(&a, &b).await, 1, "one space row should ship");

    let name: String = sqlx::query_scalar!("SELECT name FROM spaces WHERE id = 's1'")
        .fetch_one(&b.pool)
        .await
        .unwrap();
    assert_eq!(name, "shared space");
    assert_eq!(b.cursor_for("s1", "device-A").await, 1);
}

/// Independent databases never share a connection pool or sequence
/// counter — a write on A must not produce change_log on B.
#[tokio::test]
async fn independent_databases_share_nothing() {
    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;
    let b = TestDevice::create("device-B", &dir).await;

    a.insert_space("s1", "only on A").await;

    let b_count: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM change_log")
        .fetch_one(&b.pool)
        .await
        .unwrap();
    assert_eq!(b_count, 0, "B has change_log rows it never authored");

    let a_count: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM change_log")
        .fetch_one(&a.pool)
        .await
        .unwrap();
    assert_eq!(a_count, 1);
    assert_ne!(a.device_id().await, b.device_id().await);
}

/// A restarted fixture retains identity, cursors, and synchronized data.
#[tokio::test]
async fn restarted_fixture_retains_state() {
    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;
    let b = TestDevice::create("device-B", &dir).await;
    let b_path = b.db_path.clone();

    a.insert_space("s1", "persisted space").await;
    sync_direct(&a, &b).await;
    assert_eq!(b.cursor_for("s1", "device-A").await, 1);

    drop(b);

    let b2 = TestDevice::reopen(&b_path).await;
    assert_eq!(
        b2.device_id().await,
        "device-B",
        "identity lost across restart"
    );
    assert_eq!(
        b2.cursor_for("s1", "device-A").await,
        1,
        "receive cursor lost across restart"
    );
    let name: String = sqlx::query_scalar!("SELECT name FROM spaces WHERE id = 's1'")
        .fetch_one(&b2.pool)
        .await
        .unwrap();
    assert_eq!(
        name, "persisted space",
        "synchronized data lost across restart"
    );
}

/// A→B→C relay delivers A's change to C.
#[tokio::test]
async fn relay_delivers_change_to_third_device() {
    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;
    let b = TestDevice::create("device-B", &dir).await;
    let c = TestDevice::create("device-C", &dir).await;

    a.insert_space("s1", "relayed space").await;
    sync_direct(&a, &b).await;
    sync_direct(&b, &c).await;

    let name: String = sqlx::query_scalar!("SELECT name FROM spaces WHERE id = 's1'")
        .fetch_one(&c.pool)
        .await
        .unwrap();
    assert_eq!(name, "relayed space");
}
