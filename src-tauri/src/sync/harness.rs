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
                let payload_json = match &row.payload {
                    Some(p) => Some(p.to_json().unwrap_or_default()),
                    None => None,
                };
                sqlx::query_file!(
                    "queries/sync/insert_remote_change_log.sql",
                    row.id,
                    row.space_id,
                    row.table_name,
                    row.row_id,
                    row.operation,
                    payload_json,
                    row.seq,
                    row.device_id,
                    row.device_id,
                    row.seq,
                )
                .execute(&mut **tx)
                .await
                .map_err(|e| AppError::Db(format!("harness insert_remote_change_log: {e}")))?;

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

// ---------------------------------------------------------------------------
// Verification tests for Step 28.2 — unsafe change-log GC disabled
// ---------------------------------------------------------------------------

/// A change remains after successful sync with a different peer when a
/// third trusted peer is offline. The old `all_peers_consumed` GC
/// interpreted A's *receive* cursors as remote *acknowledgments*: A's
/// cursor for C (advanced by a prior C→A sync) was misread as "C
/// consumed A's changes," allowing deletion of change_log rows C never
/// received.
#[tokio::test]
async fn gc_preserves_change_when_third_peer_offline() {
    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;
    let b = TestDevice::create("device-B", &dir).await;
    let c = TestDevice::create("device-C", &dir).await;

    let ts = "2024-01-01T00:00:00Z";

    // All three devices share space "s1".
    a.insert_space("s1", "original").await;
    sync_direct(&a, &b).await;
    sync_direct(&a, &c).await;

    // B and C each modify the space, then sync to A. This advances
    // A's receive cursors for B and C — the setup that tricks the old
    // GC into thinking B and C have consumed A's own changes.
    sqlx::query!(
        "UPDATE spaces SET name = 'B rename', updated_at = ?1 WHERE id = 's1'",
        ts
    )
    .execute(&b.pool)
    .await
    .unwrap();
    sync_direct(&b, &a).await;

    sqlx::query!(
        "UPDATE spaces SET name = 'C rename', updated_at = ?1 WHERE id = 's1'",
        ts
    )
    .execute(&c.pool)
    .await
    .unwrap();
    sync_direct(&c, &a).await;

    // Register B and C as trusted peers in space s1 on A so the old
    // all_peers_consumed query has peers to check against.
    for peer in &["device-B", "device-C"] {
        sqlx::query!(
            "INSERT INTO trusted_devices \
             (id, space_id, device_id, display_name, cert_pem, sync_enabled, paired_at) \
             VALUES (?1, 's1', ?2, ?2, 'CERT', 1, ?3)",
            format!("{peer}-td"),
            peer,
            ts
        )
        .execute(&a.pool)
        .await
        .unwrap();
    }

    // A's change_log for s1 now has rows from three origins:
    //   seq=1, device-A  (original insert)
    //   seq=2, device-B  (relayed rename from B)
    //   seq=3, device-C  (relayed rename from C)
    // A's receive cursors: for B = 2, for C = 2.
    //
    // Old all_peers_consumed would check: for every trusted peer, is
    // A's receive cursor >= the row's seq? For seq=2 (device-B):
    //   cursor for B = 2 >= 2 ✓, cursor for C = 2 >= 2 ✓ → DELETED.
    // But C never consumed B's rename! A's cursor for C means "A
    // consumed 2 of C's changes," not "C consumed 2 of ours."
    crate::sync::gc::run(&a.pool).await.unwrap();

    let origins: Vec<String> = sqlx::query_scalar!(
        "SELECT DISTINCT device_id FROM change_log \
         WHERE space_id = 's1' AND table_name = 'spaces' ORDER BY device_id"
    )
    .fetch_all(&a.pool)
    .await
    .unwrap();

    assert!(
        origins.contains(&"device-A".to_string()),
        "A's origin must survive GC"
    );
    assert!(
        origins.contains(&"device-B".to_string()),
        "B's relayed origin must survive GC — C never consumed it"
    );
    assert!(
        origins.contains(&"device-C".to_string()),
        "C's relayed origin must survive GC"
    );
}

/// Rows from different origins are never compared by numeric sequence
/// alone. The old `compact` pass grouped by (space_id, table_name,
/// row_id) and kept only MAX(seq), collapsing independent origin
/// streams into one winner. Two devices creating the same row_id
/// independently would lose the lower-seq origin after compaction.
#[tokio::test]
async fn gc_never_compacts_across_origins() {
    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;
    let b = TestDevice::create("device-B", &dir).await;

    // Both devices independently insert the same space id.
    a.insert_space("s1", "A's version").await;
    b.insert_space("s1", "B's version").await;

    // Sync A→B: B applies A's space as an upsert (INSERT ON CONFLICT
    // DO UPDATE). The UPDATE trigger fires, creating a new change_log
    // row attributed to device-A (from the apply override) with B's
    // next seq value.
    sync_direct(&a, &b).await;

    // B's change_log for s1 now has two rows for the same row_id:
    //   seq=1, device-B  (original insert)
    //   seq=2, device-A  (relayed from A, upsert fired UPDATE trigger)
    //
    // Old compact would group by (s1, spaces, s1), keep MAX(seq)=2,
    // and delete seq=1 — losing device-B's origin entirely.
    crate::sync::gc::run(&b.pool).await.unwrap();

    let origins: Vec<String> = sqlx::query_scalar!(
        "SELECT DISTINCT device_id FROM change_log \
         WHERE space_id = 's1' AND table_name = 'spaces' ORDER BY device_id"
    )
    .fetch_all(&b.pool)
    .await
    .unwrap();

    assert!(
        origins.contains(&"device-A".to_string()),
        "A's origin must survive GC on B"
    );
    assert!(
        origins.contains(&"device-B".to_string()),
        "B's own origin must survive GC — compaction must not cross origins"
    );
}

// ---------------------------------------------------------------------------
// Verification tests for Step 28.3 — protect local users
// ---------------------------------------------------------------------------

/// Removing a user's last membership must not silently delete their
/// local profile. The old `orphan_users` GC pass ran `DELETE FROM users
/// WHERE id NOT IN (SELECT user_id FROM space_members)`, so the next GC
/// run after removing a membership would erase the user — even if the
/// removal was a remote sync operation the local user didn't initiate.
#[tokio::test]
async fn gc_preserves_user_after_last_membership_removed() {
    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;

    a.insert_space("s1", "test").await;
    sqlx::query!("INSERT INTO users (id, name) VALUES ('u1', 'Alice')")
        .execute(&a.pool)
        .await
        .unwrap();
    sqlx::query!(
        "INSERT INTO space_members (space_id, user_id, role) VALUES ('s1', 'u1', 'owner')"
    )
    .execute(&a.pool)
    .await
    .unwrap();

    // Remove the only membership.
    sqlx::query!("DELETE FROM space_members WHERE user_id = 'u1'")
        .execute(&a.pool)
        .await
        .unwrap();

    crate::sync::gc::run(&a.pool).await.unwrap();

    let count: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM users WHERE id = 'u1'")
        .fetch_one(&a.pool)
        .await
        .unwrap();
    assert_eq!(
        count, 1,
        "user must survive GC after losing last membership"
    );
}

/// A newly created user with no memberships yet must survive GC. The
/// old `orphan_users` pass could collect a user in the window between
/// profile creation and the first membership insert.
#[tokio::test]
async fn gc_preserves_user_with_no_memberships() {
    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;

    sqlx::query!("INSERT INTO users (id, name) VALUES ('u1', 'Alice')")
        .execute(&a.pool)
        .await
        .unwrap();

    crate::sync::gc::run(&a.pool).await.unwrap();

    let count: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM users WHERE id = 'u1'")
        .fetch_one(&a.pool)
        .await
        .unwrap();
    assert_eq!(
        count, 1,
        "user with no memberships must survive GC — no collection window"
    );
}

// ---------------------------------------------------------------------------
// Verification tests for Step 28.5 — snapshot dependency order
// ---------------------------------------------------------------------------

/// Snapshot application succeeds on an empty database with foreign keys
/// enabled and multiple previously unknown users. Users must be applied
/// before memberships (`space_members.user_id` FK to `users.id`).
#[tokio::test]
async fn snapshot_applies_users_before_memberships() {
    use crate::sync::apply_guard::run_as_device;
    use crate::sync::snapshot::{SnapshotFrame, SpaceSnapshot, WireMember, WireSpace, WireUser};

    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;
    let b = TestDevice::create("device-B", &dir).await;

    let ts = "2024-01-01T00:00:00Z";

    // On A: create a space with two users and memberships.
    a.insert_space("s1", "shared").await;
    sqlx::query!("INSERT INTO users (id, name) VALUES ('u1', 'Alice')")
        .execute(&a.pool)
        .await
        .unwrap();
    sqlx::query!("INSERT INTO users (id, name) VALUES ('u2', 'Bob')")
        .execute(&a.pool)
        .await
        .unwrap();
    sqlx::query!(
        "INSERT INTO space_members (space_id, user_id, role, joined_at) VALUES ('s1', 'u1', 'owner', ?1)",
        ts
    )
    .execute(&a.pool)
    .await
    .unwrap();
    sqlx::query!(
        "INSERT INTO space_members (space_id, user_id, role, joined_at) VALUES ('s1', 'u2', 'member', ?1)",
        ts
    )
    .execute(&a.pool)
    .await
    .unwrap();

    // Build a snapshot skeleton (as the host would send it).
    let snapshot = SpaceSnapshot {
        space: WireSpace {
            id: "s1".into(),
            name: "shared".into(),
            created_at: ts.into(),
            updated_at: ts.into(),
        },
        members: vec![
            WireMember {
                space_id: "s1".into(),
                user_id: "u1".into(),
                role: "owner".into(),
                joined_at: ts.into(),
            },
            WireMember {
                space_id: "s1".into(),
                user_id: "u2".into(),
                role: "member".into(),
                joined_at: ts.into(),
            },
        ],
        users: vec![
            WireUser {
                id: "u1".into(),
                name: "Alice".into(),
                pin_hash: None,
                created_at: ts.into(),
                updated_at: ts.into(),
            },
            WireUser {
                id: "u2".into(),
                name: "Bob".into(),
                pin_hash: None,
                created_at: ts.into(),
                updated_at: ts.into(),
            },
        ],
        categories: vec![],
        accounts: vec![],
        transactions: vec![],
        account_summaries: vec![],
        space_settings: vec![],
        model_versions: vec![],
        host_device_id: "device-A".into(),
        host_device_name: "host".into(),
        host_cert_pem: "CERT".into(),
    };

    // Apply on B with FKs enabled, using the production dependency order:
    // Space → Users → Members → End.
    run_as_device(&b.pool, "device-A", move |tx| {
        let snap = snapshot.clone();
        Box::pin(async move {
            crate::sync::snapshot::apply_snapshot_frame(
                tx,
                &snap,
                &SnapshotFrame::Space(snap.space.clone()),
            )
            .await?;
            crate::sync::snapshot::apply_snapshot_frame(
                tx,
                &snap,
                &SnapshotFrame::Users(snap.users.clone()),
            )
            .await?;
            crate::sync::snapshot::apply_snapshot_frame(
                tx,
                &snap,
                &SnapshotFrame::Members(snap.members.clone()),
            )
            .await?;
            crate::sync::snapshot::apply_snapshot_frame(tx, &snap, &SnapshotFrame::End).await?;
            Ok::<(), crate::error::AppError>(())
        })
    })
    .await
    .unwrap();

    let member_count: i64 =
        sqlx::query_scalar!("SELECT COUNT(*) FROM space_members WHERE space_id = 's1'")
            .fetch_one(&b.pool)
            .await
            .unwrap();
    assert_eq!(member_count, 2, "both memberships must be applied");

    let user_count: i64 =
        sqlx::query_scalar!("SELECT COUNT(*) FROM users WHERE id IN ('u1', 'u2')")
            .fetch_one(&b.pool)
            .await
            .unwrap();
    assert_eq!(
        user_count, 2,
        "both users must be applied before memberships"
    );
}

// ---------------------------------------------------------------------------
// Verification tests for Step 29.1 — ledger V2 state bootstrap
// ---------------------------------------------------------------------------

/// The V2 migration creates `origin_state`, `row_winners`, `peer_acks`,
/// and `v2_reconciliation` tables. When the migration runs on a
/// database with existing data, the bootstrap queries populate them
/// from the existing `change_log`.
#[tokio::test]
async fn ledger_v2_bootstrap_populates_from_existing_data() {
    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;

    // Insert a space and update it — creates two change_log rows.
    a.insert_space("s1", "original").await;
    sqlx::query!("UPDATE spaces SET name = 'updated' WHERE id = 's1'")
        .execute(&a.pool)
        .await
        .unwrap();

    // The migration already ran on this fresh database, but the
    // bootstrap tables were empty because no data existed at migration
    // time. Re-run the bootstrap queries to simulate a real upgrade.
    sqlx::query!("DELETE FROM origin_state")
        .execute(&a.pool)
        .await
        .unwrap();
    sqlx::query!("DELETE FROM row_winners")
        .execute(&a.pool)
        .await
        .unwrap();
    sqlx::query!("DELETE FROM v2_reconciliation")
        .execute(&a.pool)
        .await
        .unwrap();

    // Re-run the bootstrap INSERTs from migration 0004.
    sqlx::query!(
        "INSERT INTO origin_state (space_id, origin_device_id, high_water, retained_floor) \
         SELECT space_id, device_id, MAX(seq), MIN(seq) \
         FROM change_log GROUP BY space_id, device_id"
    )
    .execute(&a.pool)
    .await
    .unwrap();

    sqlx::query!(
        "INSERT INTO row_winners (space_id, table_name, row_id, winning_seq, winning_origin, deleted) \
         WITH ranked AS (\
             SELECT space_id, table_name, row_id, seq, device_id, \
                    CASE WHEN operation = 'delete' THEN 1 ELSE 0 END AS is_delete, \
                    ROW_NUMBER() OVER (PARTITION BY space_id, table_name, row_id ORDER BY seq DESC, device_id DESC) AS rn \
             FROM change_log\
         ) \
         SELECT space_id, table_name, row_id, seq, device_id, is_delete FROM ranked WHERE rn = 1"
    )
    .execute(&a.pool)
    .await
    .unwrap();

    sqlx::query!("INSERT INTO v2_reconciliation (space_id, required) SELECT id, 1 FROM spaces")
        .execute(&a.pool)
        .await
        .unwrap();

    // Verify origin_state: one origin (device-A), high_water = 3
    // (insert + name update + auto-updated_at trigger each create
    // a change_log row).
    let origins = sqlx::query!(
        "SELECT origin_device_id, high_water, retained_floor \
         FROM origin_state WHERE space_id = 's1'"
    )
    .fetch_all(&a.pool)
    .await
    .unwrap();
    assert_eq!(origins.len(), 1);
    assert_eq!(origins[0].origin_device_id, "device-A");
    assert_eq!(origins[0].high_water, 3);
    assert_eq!(origins[0].retained_floor, 1);

    // Verify row_winners: one winner for (s1, spaces, s1), deleted = 0.
    let winners = sqlx::query!(
        "SELECT table_name, row_id, winning_seq, winning_origin, deleted \
         FROM row_winners WHERE space_id = 's1'"
    )
    .fetch_all(&a.pool)
    .await
    .unwrap();
    assert_eq!(winners.len(), 1);
    assert_eq!(winners[0].table_name, "spaces");
    assert_eq!(winners[0].row_id, "s1");
    assert_eq!(winners[0].winning_seq, 3);
    assert_eq!(winners[0].winning_origin, "device-A");
    assert_eq!(winners[0].deleted, 0);

    // Verify v2_reconciliation: all spaces marked.
    let recon = sqlx::query!("SELECT required FROM v2_reconciliation WHERE space_id = 's1'")
        .fetch_one(&a.pool)
        .await
        .unwrap();
    assert_eq!(recon.required, 1);

    // Verify peer_acks is empty (no peer has acknowledged in V2 yet).
    let ack_count: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM peer_acks")
        .fetch_one(&a.pool)
        .await
        .unwrap();
    assert_eq!(ack_count, 0);
}

/// The unique index on `(space_id, origin_device_id, origin_seq)`
/// prevents duplicate immutable origin entries.
#[tokio::test]
async fn ledger_v2_unique_origin_key_rejects_duplicates() {
    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;

    // Insert a change_log row with explicit origin columns.
    sqlx::query!(
        "INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id, origin_device_id, origin_seq) \
         VALUES ('cl-1', 's1', 'spaces', 's1', 'insert', NULL, 1, 'device-A', 'device-A', 1)"
    )
    .execute(&a.pool)
    .await
    .unwrap();

    // Same origin key must fail.
    let result = sqlx::query!(
        "INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id, origin_device_id, origin_seq) \
         VALUES ('cl-2', 's1', 'spaces', 's1', 'update', NULL, 2, 'device-A', 'device-A', 1)"
    )
    .execute(&a.pool)
    .await;
    assert!(
        result.is_err(),
        "duplicate origin key must be rejected by the unique index"
    );

    // Different origin seq is fine.
    sqlx::query!(
        "INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id, origin_device_id, origin_seq) \
         VALUES ('cl-3', 's1', 'spaces', 's1', 'update', NULL, 3, 'device-A', 'device-A', 2)"
    )
    .execute(&a.pool)
    .await
    .unwrap();
}

/// Row winners correctly tracks tombstones for deleted rows.
#[tokio::test]
async fn ledger_v2_row_winners_track_tombstones() {
    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;

    a.insert_space("s1", "test").await;
    // Soft-delete the space (sets deleted=1; the _au trigger is
    // suppressed by WHEN NEW.deleted = 0, so no update change_log row
    // is created). Then manually insert the delete change_log entry,
    // exactly like the delete_space command does.
    sqlx::query_file!("queries/spaces/soft_delete_space.sql", "s1")
        .execute(&a.pool)
        .await
        .unwrap();
    sqlx::query_file!("queries/spaces/increment_sync_seq.sql")
        .execute(&a.pool)
        .await
        .unwrap();
    sqlx::query_file!("queries/spaces/insert_space_delete_changelog.sql", "s1")
        .execute(&a.pool)
        .await
        .unwrap();

    // Bootstrap row_winners from the current change_log.
    sqlx::query!(
        "INSERT INTO row_winners (space_id, table_name, row_id, winning_seq, winning_origin, deleted) \
         WITH ranked AS (\
             SELECT space_id, table_name, row_id, seq, device_id, \
                    CASE WHEN operation = 'delete' THEN 1 ELSE 0 END AS is_delete, \
                    ROW_NUMBER() OVER (PARTITION BY space_id, table_name, row_id ORDER BY seq DESC, device_id DESC) AS rn \
             FROM change_log\
         ) \
         SELECT space_id, table_name, row_id, seq, device_id, is_delete FROM ranked WHERE rn = 1"
    )
    .execute(&a.pool)
    .await
    .unwrap();

    let winner = sqlx::query!(
        "SELECT winning_seq, winning_origin, deleted \
         FROM row_winners WHERE space_id = 's1' AND table_name = 'spaces'"
    )
    .fetch_one(&a.pool)
    .await
    .unwrap();

    assert_eq!(
        winner.deleted, 1,
        "tombstone must be tracked for deleted row"
    );
    assert_eq!(winner.winning_origin, "device-A");
}

// ---------------------------------------------------------------------------
// Verification tests for Step 29.2 — suppress triggers, preserve origin
// ---------------------------------------------------------------------------

/// A change authored as A/10 remains A/10 on B and C through relay.
/// The old triggers re-stamped relayed changes with the local seq
/// counter, destroying origin identity. The new apply path inserts
/// the change_log row directly with the original origin.
#[tokio::test]
async fn relay_preserves_origin_identity() {
    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;
    let b = TestDevice::create("device-B", &dir).await;
    let c = TestDevice::create("device-C", &dir).await;

    a.insert_space("s1", "original").await;

    // A's change_log should have origin_device_id = device-A, origin_seq = 1.
    let a_row = sqlx::query!(
        "SELECT device_id, seq, origin_device_id, origin_seq \
         FROM change_log WHERE space_id = 's1' AND table_name = 'spaces' \
         ORDER BY seq LIMIT 1"
    )
    .fetch_one(&a.pool)
    .await
    .unwrap();
    assert_eq!(a_row.device_id, "device-A");
    assert_eq!(a_row.seq, 1);
    assert_eq!(a_row.origin_device_id.as_deref(), Some("device-A"));
    assert_eq!(a_row.origin_seq, Some(1));

    // Sync A→B→C.
    sync_direct(&a, &b).await;
    sync_direct(&b, &c).await;

    // B's change_log must preserve the original origin.
    let b_row = sqlx::query!(
        "SELECT device_id, seq, origin_device_id, origin_seq \
         FROM change_log WHERE space_id = 's1' AND table_name = 'spaces' \
         AND origin_device_id = 'device-A'"
    )
    .fetch_one(&b.pool)
    .await
    .unwrap();
    assert_eq!(b_row.device_id, "device-A", "device_id must be preserved");
    assert_eq!(b_row.seq, 1, "seq must be the original origin seq");
    assert_eq!(b_row.origin_device_id.as_deref(), Some("device-A"));
    assert_eq!(b_row.origin_seq, Some(1));

    // C's change_log must also preserve the original origin.
    let c_row = sqlx::query!(
        "SELECT device_id, seq, origin_device_id, origin_seq \
         FROM change_log WHERE space_id = 's1' AND table_name = 'spaces' \
         AND origin_device_id = 'device-A'"
    )
    .fetch_one(&c.pool)
    .await
    .unwrap();
    assert_eq!(c_row.device_id, "device-A");
    assert_eq!(c_row.seq, 1, "seq must remain A/1 after two-hop relay");
    assert_eq!(c_row.origin_device_id.as_deref(), Some("device-A"));
    assert_eq!(c_row.origin_seq, Some(1));
}

/// Receiving the same immutable change through two relay paths is
/// idempotent. The `ON CONFLICT DO NOTHING` on the unique origin key
/// prevents duplicates.
#[tokio::test]
async fn duplicate_relay_is_idempotent() {
    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;
    let b = TestDevice::create("device-B", &dir).await;
    let c = TestDevice::create("device-C", &dir).await;

    a.insert_space("s1", "original").await;

    // C receives the change through two paths: A→C directly, and
    // A→B→C.
    sync_direct(&a, &b).await;
    sync_direct(&a, &c).await;
    sync_direct(&b, &c).await;

    // C must have exactly one change_log row for (s1, device-A, seq=1).
    let count: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM change_log \
         WHERE space_id = 's1' AND table_name = 'spaces' \
         AND origin_device_id = 'device-A' AND origin_seq = 1"
    )
    .fetch_one(&c.pool)
    .await
    .unwrap();
    assert_eq!(
        count, 1,
        "duplicate relay must be idempotent — exactly one row for the origin"
    );
}
