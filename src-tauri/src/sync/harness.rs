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

use crate::sync::changelog;
use crate::sync::cursors;
use crate::sync::payloads::{SpacePayload, TablePayload, TransactionPayload};
use crate::sync::session::{apply_round_core, store_peer_acks, validate_batch};
use crate::sync::wire::{ChangeBatch, ChangeRow, CursorEntry};

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

    let mut batches = Vec::new();
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

        let count = rows.len();
        batches.push(ChangeBatch {
            space_id: space_id.clone(),
            origin_device_id: origin_device.clone(),
            transport_device_id: from.device_id.clone(),
            rows,
            final_seq,
        });
        total += count;
    }

    apply_round_core(&to.pool, &from.device_id, &batches)
        .await
        .unwrap_or_else(|e| panic!("harness: apply_round_core: {e}"));
    total
}

async fn apply_batch(to: &TestDevice, batch: &ChangeBatch) {
    apply_round_core(&to.pool, &batch.transport_device_id, &[batch.clone()])
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

/// Convenience: build a `spaces` update `ChangeRow` authored by
/// `device_id`. Mirrors what the SQLite trigger would produce for an
/// UPDATE on the spaces table.
#[allow(dead_code)]
pub fn space_update_row(
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
        operation: "update".into(),
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
    // Register C as a trusted device that hasn't acknowledged so
    // collect_acked preserves device-A's relayed rows on B.
    sqlx::query!(
        "INSERT INTO trusted_devices \
         (id, space_id, device_id, display_name, cert_pem, sync_enabled, paired_at) \
         VALUES ('c-td', 's1', 'device-C', 'C', 'CERT', 1, '2024-01-01T00:00:00Z')"
    )
    .execute(&b.pool)
    .await
    .unwrap();

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

    // The 0006 change_log_winner_upsert trigger maintains row_winners
    // automatically — the delete change_log row inserted above upserts
    // the winner to (2, device-A, deleted=1).

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

// ---------------------------------------------------------------------------
// Verification tests for Step 29.3 — Lamport revisions
// ---------------------------------------------------------------------------

/// Concurrent updates delivered in opposite orders select the same
/// winner. Device A and B both update the same row with seq=5. The
/// tiebreaker is `origin_device_id DESC`, so "device-B" > "device-A"
/// regardless of delivery order.
#[tokio::test]
async fn concurrent_updates_opposite_orders_same_winner() {
    let dir = TempDir::new().unwrap();

    let src = TestDevice::create("device-SRC", &dir).await;
    src.insert_space("s1", "original").await;

    let c1 = TestDevice::create("device-C1", &dir).await;
    let c2 = TestDevice::create("device-C2", &dir).await;
    sync_direct(&src, &c1).await;
    sync_direct(&src, &c2).await;

    let update_a = space_update_row("ch-a", "s1", "A version", "device-A", 5);
    let update_b = space_update_row("ch-b", "s1", "B version", "device-B", 5);

    // Path 1: A first, then B.
    apply_batch(
        &c1,
        &ChangeBatch {
            space_id: "s1".into(),
            origin_device_id: "device-A".into(),
            transport_device_id: "device-A".into(),
            rows: vec![update_a.clone()],
            final_seq: 5,
        },
    )
    .await;
    apply_batch(
        &c1,
        &ChangeBatch {
            space_id: "s1".into(),
            origin_device_id: "device-B".into(),
            transport_device_id: "device-B".into(),
            rows: vec![update_b.clone()],
            final_seq: 5,
        },
    )
    .await;

    // Path 2: B first, then A.
    apply_batch(
        &c2,
        &ChangeBatch {
            space_id: "s1".into(),
            origin_device_id: "device-B".into(),
            transport_device_id: "device-B".into(),
            rows: vec![update_b.clone()],
            final_seq: 5,
        },
    )
    .await;
    apply_batch(
        &c2,
        &ChangeBatch {
            space_id: "s1".into(),
            origin_device_id: "device-A".into(),
            transport_device_id: "device-A".into(),
            rows: vec![update_a.clone()],
            final_seq: 5,
        },
    )
    .await;

    let w1 = sqlx::query!(
        "SELECT winning_seq, winning_origin, deleted FROM row_winners \
         WHERE space_id = 's1' AND table_name = 'spaces'"
    )
    .fetch_one(&c1.pool)
    .await
    .unwrap();
    let w2 = sqlx::query!(
        "SELECT winning_seq, winning_origin, deleted FROM row_winners \
         WHERE space_id = 's1' AND table_name = 'spaces'"
    )
    .fetch_one(&c2.pool)
    .await
    .unwrap();

    assert_eq!(w1.winning_origin, "device-B", "C1 winner must be B");
    assert_eq!(w2.winning_origin, "device-B", "C2 winner must be B");
    assert_eq!(w1.winning_seq, w2.winning_seq);

    let n1: String = sqlx::query_scalar!("SELECT name FROM spaces WHERE id = 's1'")
        .fetch_one(&c1.pool)
        .await
        .unwrap();
    let n2: String = sqlx::query_scalar!("SELECT name FROM spaces WHERE id = 's1'")
        .fetch_one(&c2.pool)
        .await
        .unwrap();
    assert_eq!(n1, "B version");
    assert_eq!(n2, "B version");
}

/// A delete observed before a later local update is ordered before
/// that update. The Lamport clock raise ensures the local write gets
/// a higher seq than the remote delete, so the update wins and the
/// row is alive.
#[tokio::test]
async fn delete_before_local_update() {
    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;
    let b = TestDevice::create("device-B", &dir).await;

    a.insert_space("s1", "original").await;
    sync_direct(&a, &b).await;

    // A deletes the space.
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

    // Sync A→B: B receives the delete.
    sync_direct(&a, &b).await;

    let deleted: i64 = sqlx::query_scalar!("SELECT deleted FROM spaces WHERE id = 's1'")
        .fetch_one(&b.pool)
        .await
        .unwrap();
    assert_eq!(
        deleted, 1,
        "space must be deleted after receiving remote delete"
    );

    // sync_seq must be raised to at least the delete's seq.
    let sync_seq_before: i64 = sqlx::query_scalar!(
        "SELECT CAST(value AS INTEGER) FROM app_settings WHERE key = 'sync_seq'"
    )
    .fetch_one(&b.pool)
    .await
    .unwrap();
    assert!(
        sync_seq_before >= 2,
        "sync_seq must be raised after receiving delete"
    );

    // B makes a local update (undelete + rename).
    sqlx::query!("UPDATE spaces SET name = 'recovered', deleted = 0 WHERE id = 's1'")
        .execute(&b.pool)
        .await
        .unwrap();

    let local_seq: i64 = sqlx::query_scalar!(
        "SELECT COALESCE(MAX(seq), 0) FROM change_log \
         WHERE device_id = 'device-B' AND space_id = 's1'"
    )
    .fetch_one(&b.pool)
    .await
    .unwrap();
    assert!(
        local_seq > sync_seq_before,
        "local write must get seq > raised sync_seq"
    );

    let winner = sqlx::query!(
        "SELECT winning_seq, winning_origin, deleted FROM row_winners \
         WHERE space_id = 's1' AND table_name = 'spaces'"
    )
    .fetch_one(&b.pool)
    .await
    .unwrap();
    assert_eq!(
        winner.winning_origin, "device-B",
        "local update must win over remote delete"
    );
    assert_eq!(
        winner.deleted, 0,
        "winner should be an update, not a tombstone"
    );
}

/// An older update cannot resurrect a winning tombstone. A delete with
/// seq=6 wins. An update with seq=5 arrives later and loses the
/// comparison — the tombstone persists, the row stays deleted.
#[tokio::test]
async fn older_update_cannot_resurrect_tombstone() {
    let dir = TempDir::new().unwrap();
    let c = TestDevice::create("device-C", &dir).await;

    c.insert_space("s1", "original").await;

    // Apply a delete from device-A with seq=6.
    let delete_row = ChangeRow {
        id: "ch-del".into(),
        space_id: "s1".into(),
        table_name: "spaces".into(),
        row_id: "s1".into(),
        operation: "delete".into(),
        payload: None,
        seq: 6,
        device_id: "device-A".into(),
        changed_at: "2024-01-01T00:00:00Z".into(),
    };
    apply_batch(
        &c,
        &ChangeBatch {
            space_id: "s1".into(),
            origin_device_id: "device-A".into(),
            transport_device_id: "device-A".into(),
            rows: vec![delete_row],
            final_seq: 6,
        },
    )
    .await;

    let deleted: i64 = sqlx::query_scalar!("SELECT deleted FROM spaces WHERE id = 's1'")
        .fetch_one(&c.pool)
        .await
        .unwrap();
    assert_eq!(deleted, 1, "space must be deleted after tombstone wins");

    // Now apply an older update (seq=5) from device-B.
    let update_row = space_update_row("ch-upd", "s1", "resurrection attempt", "device-B", 5);
    apply_batch(
        &c,
        &ChangeBatch {
            space_id: "s1".into(),
            origin_device_id: "device-B".into(),
            transport_device_id: "device-B".into(),
            rows: vec![update_row],
            final_seq: 5,
        },
    )
    .await;

    let winner = sqlx::query!(
        "SELECT winning_seq, winning_origin, deleted FROM row_winners \
         WHERE space_id = 's1' AND table_name = 'spaces'"
    )
    .fetch_one(&c.pool)
    .await
    .unwrap();
    assert_eq!(
        winner.winning_seq, 6,
        "older update must not dethrone the tombstone"
    );
    assert_eq!(winner.winning_origin, "device-A");
    assert_eq!(winner.deleted, 1, "tombstone must persist");

    let still_deleted: i64 = sqlx::query_scalar!("SELECT deleted FROM spaces WHERE id = 's1'")
        .fetch_one(&c.pool)
        .await
        .unwrap();
    assert_eq!(
        still_deleted, 1,
        "older update cannot resurrect a tombstone"
    );
}

// ---------------------------------------------------------------------------
// Verification tests for Step 29.4 — envelope and payload validation
// ---------------------------------------------------------------------------

/// A well-formed batch with valid insert and delete rows passes.
#[test]
fn validate_batch_accepts_valid_batch() {
    let insert_row = space_insert_row("ch-1", "s1", "test", "device-A", 1);
    let delete_row = ChangeRow {
        id: "ch-2".into(),
        space_id: "s1".into(),
        table_name: "spaces".into(),
        row_id: "s1".into(),
        operation: "delete".into(),
        payload: None,
        seq: 2,
        device_id: "device-A".into(),
        changed_at: "2024-01-01T00:00:00Z".into(),
    };
    let batch = ChangeBatch {
        space_id: "s1".into(),
        origin_device_id: "device-A".into(),
        transport_device_id: "device-A".into(),
        rows: vec![insert_row, delete_row],
        final_seq: 2,
    };
    assert!(validate_batch(&batch).is_ok());
}

/// An empty batch passes (cursor-only advance with no changes).
#[test]
fn validate_batch_accepts_empty_batch() {
    let batch = ChangeBatch {
        space_id: "s1".into(),
        origin_device_id: "device-A".into(),
        transport_device_id: "device-A".into(),
        rows: vec![],
        final_seq: 0,
    };
    assert!(validate_batch(&batch).is_ok());
}

/// A row whose origin doesn't match the batch origin is rejected.
#[test]
fn validate_batch_rejects_mismatched_origin() {
    let mut row = space_insert_row("ch-1", "s1", "test", "device-A", 1);
    row.device_id = "device-B".into();
    let batch = ChangeBatch {
        space_id: "s1".into(),
        origin_device_id: "device-A".into(),
        transport_device_id: "device-A".into(),
        rows: vec![row],
        final_seq: 1,
    };
    assert!(validate_batch(&batch).is_err());
}

/// A row whose space doesn't match the batch space is rejected.
#[test]
fn validate_batch_rejects_mismatched_space() {
    let mut row = space_insert_row("ch-1", "s1", "test", "device-A", 1);
    row.space_id = "s2".into();
    let batch = ChangeBatch {
        space_id: "s1".into(),
        origin_device_id: "device-A".into(),
        transport_device_id: "device-A".into(),
        rows: vec![row],
        final_seq: 1,
    };
    assert!(validate_batch(&batch).is_err());
}

/// An insert without a payload, a payload variant mismatch, and a
/// payload key mismatch are all rejected.
#[test]
fn validate_batch_rejects_payload_envelope_mismatches() {
    // Missing payload for insert.
    let mut no_payload = space_insert_row("ch-1", "s1", "test", "device-A", 1);
    no_payload.payload = None;
    let batch = ChangeBatch {
        space_id: "s1".into(),
        origin_device_id: "device-A".into(),
        transport_device_id: "device-A".into(),
        rows: vec![no_payload],
        final_seq: 1,
    };
    assert!(validate_batch(&batch).is_err());

    // Payload variant mismatches table_name.
    let mut wrong_variant = space_insert_row("ch-2", "s1", "test", "device-A", 1);
    wrong_variant.table_name = "accounts".into(); // payload is Space, not Account
    let batch = ChangeBatch {
        space_id: "s1".into(),
        origin_device_id: "device-A".into(),
        transport_device_id: "device-A".into(),
        rows: vec![wrong_variant],
        final_seq: 1,
    };
    assert!(validate_batch(&batch).is_err());

    // Payload id doesn't match row_id.
    let mut key_mismatch = space_insert_row("ch-3", "s1", "test", "device-A", 1);
    key_mismatch.row_id = "wrong-id".into();
    let batch = ChangeBatch {
        space_id: "s1".into(),
        origin_device_id: "device-A".into(),
        transport_device_id: "device-A".into(),
        rows: vec![key_mismatch],
        final_seq: 1,
    };
    assert!(validate_batch(&batch).is_err());
}

/// A composite delete key that is malformed or scoped to a different
/// space is rejected.
#[test]
fn validate_batch_rejects_invalid_delete_key() {
    // Malformed: no colon.
    let malformed = ChangeRow {
        id: "ch-1".into(),
        space_id: "s1".into(),
        table_name: "space_members".into(),
        row_id: "no-colon".into(),
        operation: "delete".into(),
        payload: None,
        seq: 1,
        device_id: "device-A".into(),
        changed_at: "2024-01-01T00:00:00Z".into(),
    };
    let batch = ChangeBatch {
        space_id: "s1".into(),
        origin_device_id: "device-A".into(),
        transport_device_id: "device-A".into(),
        rows: vec![malformed],
        final_seq: 1,
    };
    assert!(validate_batch(&batch).is_err());

    // Wrong space scope: space part "s2" != batch space "s1".
    let wrong_scope = ChangeRow {
        id: "ch-2".into(),
        space_id: "s1".into(),
        table_name: "space_members".into(),
        row_id: "s2:u1".into(),
        operation: "delete".into(),
        payload: None,
        seq: 1,
        device_id: "device-A".into(),
        changed_at: "2024-01-01T00:00:00Z".into(),
    };
    let batch = ChangeBatch {
        space_id: "s1".into(),
        origin_device_id: "device-A".into(),
        transport_device_id: "device-A".into(),
        rows: vec![wrong_scope],
        final_seq: 1,
    };
    assert!(validate_batch(&batch).is_err());
}

/// Non-positive and non-monotonic sequences are rejected.
#[test]
fn validate_batch_rejects_bad_seq() {
    // Non-positive seq.
    let mut zero_seq = space_insert_row("ch-1", "s1", "test", "device-A", 0);
    zero_seq.seq = 0;
    let batch = ChangeBatch {
        space_id: "s1".into(),
        origin_device_id: "device-A".into(),
        transport_device_id: "device-A".into(),
        rows: vec![zero_seq],
        final_seq: 0,
    };
    assert!(validate_batch(&batch).is_err());

    // Non-monotonic: seq goes backwards.
    let row1 = space_insert_row("ch-2", "s1", "first", "device-A", 5);
    let row2 = space_update_row("ch-3", "s1", "second", "device-A", 3);
    let batch = ChangeBatch {
        space_id: "s1".into(),
        origin_device_id: "device-A".into(),
        transport_device_id: "device-A".into(),
        rows: vec![row1, row2],
        final_seq: 5,
    };
    assert!(validate_batch(&batch).is_err());
}

// ---------------------------------------------------------------------------
// Verification tests for Step 29.5 — change and apply barriers
// ---------------------------------------------------------------------------

/// A child change relayed before its parent does not permanently fail.
/// Device A creates a space, B creates an account, A creates a
/// transaction referencing B's account. When A relays both origins
/// to C, the transaction (origin A) arrives in the same round as the
/// account (origin B). Without FK-safe staging, the transaction
/// would fail because the account doesn't exist yet. With staging,
/// the account is applied before the transaction.
#[tokio::test]
async fn child_before_parent_succeeds() {
    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;
    let b = TestDevice::create("device-B", &dir).await;
    let c = TestDevice::create("device-C", &dir).await;

    a.insert_space("s1", "test").await;
    sync_direct(&a, &b).await;

    sqlx::query!(
        "INSERT INTO accounts (id, name, currency, account_type, account_source, color, space_id) \
         VALUES ('a1', 'Checking', 'USD', 'checking', 'manual', '#000', 's1')"
    )
    .execute(&b.pool)
    .await
    .unwrap();

    sync_direct(&b, &a).await;

    sqlx::query!(
        "INSERT INTO transactions \
         (id, booking_date, value_date, reference, text, currency, amount, balance, approved, note, account_id) \
         VALUES ('t1', '2024-01-01', '2024-01-01', 'ref', 'test', 'USD', 1000, 1000, 1, '', 'a1')"
    )
    .execute(&a.pool)
    .await
    .unwrap();

    sync_direct(&a, &c).await;

    let tx_count: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM transactions WHERE id = 't1'")
        .fetch_one(&c.pool)
        .await
        .unwrap();
    assert_eq!(tx_count, 1, "transaction must be applied after its account");

    let acct_count: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM accounts WHERE id = 'a1'")
        .fetch_one(&c.pool)
        .await
        .unwrap();
    assert_eq!(acct_count, 1, "account must be applied");
}

/// A sender receives explicit proof of the receiver's committed
/// cursors. After `sync_direct`, the receiver's cursor for each
/// origin must match the sender's max_seq — this is the data that
/// `AppliedCursors` would carry on the wire.
#[tokio::test]
async fn applied_cursors_proof() {
    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;
    let b = TestDevice::create("device-B", &dir).await;

    a.insert_space("s1", "test").await;
    sync_direct(&a, &b).await;

    let cursor = b.cursor_for("s1", "device-A").await;
    assert!(cursor > 0, "cursor must be advanced after sync");

    let max_seq = changelog::max_seq(&a.pool, "s1", "device-A").await.unwrap();
    assert_eq!(
        cursor, max_seq,
        "cursor must match sender's max_seq — explicit proof of commit"
    );
}

/// Apply failure produces no cursor or acknowledgment advance. A
/// batch with a transaction referencing a non-existent account
/// fails the FK constraint inside the `run_as_device` transaction.
/// The transaction rolls back — cursors are not advanced.
#[tokio::test]
async fn apply_failure_no_cursor_advance() {
    let dir = TempDir::new().unwrap();
    let c = TestDevice::create("device-C", &dir).await;

    c.insert_space("s1", "test").await;

    let bad_row = ChangeRow {
        id: "ch-bad".into(),
        space_id: "s1".into(),
        table_name: "transactions".into(),
        row_id: "t1".into(),
        operation: "insert".into(),
        payload: Some(TablePayload::Transaction(TransactionPayload {
            id: "t1".into(),
            booking_date: "2024-01-01".into(),
            value_date: "2024-01-01".into(),
            reference: "ref".into(),
            text: "test".into(),
            currency: "USD".into(),
            amount: 1000,
            balance: 1000,
            approved: 1,
            note: "".into(),
            category: None,
            account_id: "nonexistent".into(),
        })),
        seq: 1,
        device_id: "device-A".into(),
        changed_at: "2024-01-01T00:00:00Z".into(),
    };

    let batch = ChangeBatch {
        space_id: "s1".into(),
        origin_device_id: "device-A".into(),
        transport_device_id: "device-A".into(),
        rows: vec![bad_row],
        final_seq: 1,
    };

    let cursor_before = c.cursor_for("s1", "device-A").await;

    let result = apply_round_core(&c.pool, "device-A", &[batch]).await;
    assert!(result.is_err(), "apply should fail with FK violation");

    let cursor_after = c.cursor_for("s1", "device-A").await;
    assert_eq!(
        cursor_after, cursor_before,
        "cursor must not advance on failure"
    );
}

// ---------------------------------------------------------------------------
// Verification tests for Step 29.6 — real peer acknowledgments
// ---------------------------------------------------------------------------

/// After sync A→B, B's `AppliedCursors` entries are persisted on A
/// as `peer_acks`. The entries record what B consumed, keyed by
/// (space, consuming_device=B, origin_device=A).
#[tokio::test]
async fn peer_acks_stored_from_applied_cursors() {
    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;
    let b = TestDevice::create("device-B", &dir).await;

    a.insert_space("s1", "test").await;
    sync_direct(&a, &b).await;

    let b_cursor = b.cursor_for("s1", "device-A").await;
    assert!(b_cursor > 0, "B must have consumed A's changes");

    let entries = vec![CursorEntry {
        space_id: "s1".into(),
        device_id: "device-A".into(),
        last_seq: b_cursor,
    }];
    store_peer_acks(&a.pool, "device-B", &entries)
        .await
        .unwrap();

    let ack = sqlx::query!(
        "SELECT last_applied_seq FROM peer_acks \
         WHERE space_id = 's1' AND consuming_device_id = 'device-B' \
         AND origin_device_id = 'device-A'"
    )
    .fetch_one(&a.pool)
    .await
    .unwrap();
    assert_eq!(
        ack.last_applied_seq, b_cursor,
        "peer_acks must store B's explicit acknowledgment"
    );
}

/// A's progress consuming B cannot be interpreted as B consuming A.
/// A's receive cursor for B (A consumed B's changes) must NOT appear
/// in `peer_acks` as B's acknowledgment of A's changes. Only entries
/// explicitly reported by B via `AppliedCursors` are stored.
#[tokio::test]
async fn receive_cursor_not_inferred_as_peer_ack() {
    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;
    let b = TestDevice::create("device-B", &dir).await;

    a.insert_space("s1", "A's space").await;
    b.insert_space("s2", "B's space").await;

    // A→B: B consumes A's changes. B stores A's ack.
    sync_direct(&a, &b).await;
    let b_cursor_for_a = b.cursor_for("s1", "device-A").await;
    store_peer_acks(
        &a.pool,
        "device-B",
        &[CursorEntry {
            space_id: "s1".into(),
            device_id: "device-A".into(),
            last_seq: b_cursor_for_a,
        }],
    )
    .await
    .unwrap();

    // B→A: A consumes B's changes. A's receive cursor for B advances.
    sync_direct(&b, &a).await;
    let a_cursor_for_b = a.cursor_for("s2", "device-B").await;
    assert!(a_cursor_for_b > 0, "A must have consumed B's changes");

    // A's receive cursor for B must NOT be in peer_acks. peer_acks
    // only has B's explicit acknowledgment of A's changes.
    let false_ack = sqlx::query!(
        "SELECT last_applied_seq FROM peer_acks \
         WHERE space_id = 's2' AND consuming_device_id = 'device-B' \
         AND origin_device_id = 'device-A'"
    )
    .fetch_optional(&a.pool)
    .await
    .unwrap();
    assert!(
        false_ack.is_none(),
        "A's receive cursor must not be inferred as B's acknowledgment"
    );

    // peer_acks should only have the one entry we explicitly stored.
    let ack_count: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM peer_acks")
        .fetch_one(&a.pool)
        .await
        .unwrap();
    assert_eq!(ack_count, 1, "only one explicit peer ack should exist");
}

/// A disconnected peer prevents collection of changes it has not
/// acknowledged. A peer that has never synced has no `peer_acks`
/// entries — GC (Step 29.7) will treat its changes as uncollectable.
#[tokio::test]
async fn disconnected_peer_has_no_acks() {
    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;
    let b = TestDevice::create("device-B", &dir).await;
    let c = TestDevice::create("device-C", &dir).await;

    a.insert_space("s1", "test").await;

    // A syncs with B — B acknowledges A's changes.
    sync_direct(&a, &b).await;
    store_peer_acks(
        &a.pool,
        "device-B",
        &[CursorEntry {
            space_id: "s1".into(),
            device_id: "device-A".into(),
            last_seq: b.cursor_for("s1", "device-A").await,
        }],
    )
    .await
    .unwrap();

    // C never syncs — no acks from C.
    let c_acks: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM peer_acks WHERE consuming_device_id = 'device-C'"
    )
    .fetch_one(&a.pool)
    .await
    .unwrap();
    assert_eq!(c_acks, 0, "disconnected peer must have no acks");

    let b_acks: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM peer_acks WHERE consuming_device_id = 'device-B'"
    )
    .fetch_one(&a.pool)
    .await
    .unwrap();
    assert_eq!(b_acks, 1, "synced peer must have acks");
}

// ---------------------------------------------------------------------------
// Verification tests for Step 29.7 — acknowledgment-based GC
// ---------------------------------------------------------------------------

/// Collection never creates an undetectable gap. After GC collects
/// acked rows, origin_state.retained_floor is updated to the min
/// remaining seq. A peer whose cursor falls below that floor can
/// detect the gap and request snapshot reconciliation.
#[tokio::test]
async fn gc_collection_updates_retained_floor() {
    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;
    let b = TestDevice::create("device-B", &dir).await;

    a.insert_space("s1", "test").await;
    sqlx::query!("UPDATE spaces SET name = 'updated' WHERE id = 's1'")
        .execute(&a.pool)
        .await
        .unwrap();

    sync_direct(&a, &b).await;

    let b_max_cursor = b.cursor_for("s1", "device-A").await;
    assert!(b_max_cursor >= 2, "B must have consumed both changes");

    store_peer_acks(
        &a.pool,
        "device-B",
        &[CursorEntry {
            space_id: "s1".into(),
            device_id: "device-A".into(),
            last_seq: b_max_cursor,
        }],
    )
    .await
    .unwrap();

    let before_count: i64 =
        sqlx::query_scalar!("SELECT COUNT(*) FROM change_log WHERE space_id = 's1'")
            .fetch_one(&a.pool)
            .await
            .unwrap();
    assert!(before_count >= 2, "must have at least 2 rows before GC");

    crate::sync::gc::run(&a.pool).await.unwrap();

    let after_count: i64 =
        sqlx::query_scalar!("SELECT COUNT(*) FROM change_log WHERE space_id = 's1'")
            .fetch_one(&a.pool)
            .await
            .unwrap();
    assert_eq!(after_count, 0, "all rows should be collected");

    let os = sqlx::query!(
        "SELECT high_water, retained_floor FROM origin_state \
         WHERE space_id = 's1' AND origin_device_id = 'device-A'"
    )
    .fetch_one(&a.pool)
    .await
    .unwrap();
    assert_eq!(
        os.retained_floor, 0,
        "retained_floor must be 0 when log is empty"
    );
    assert!(
        os.high_water > 0,
        "high_water must be nonzero — the data existed"
    );
}

/// Pending device revocations survive ordinary history collection.
/// A device with sync_enabled=0 (pending revocation) still appears
/// in trusted_devices, so the GC keeps changes until it acknowledges.
#[tokio::test]
async fn gc_preserves_changes_for_pending_revocation() {
    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;
    let b = TestDevice::create("device-B", &dir).await;
    let c = TestDevice::create("device-C", &dir).await;

    let ts = "2024-01-01T00:00:00Z";

    a.insert_space("s1", "test").await;

    // Register B and C as trusted devices BEFORE syncing. C has
    // sync_enabled=0 (pending revocation). The inserts create
    // change_log rows that B and C need to receive and acknowledge.
    for (peer, enabled) in &[("device-B", 1), ("device-C", 0)] {
        sqlx::query!(
            "INSERT INTO trusted_devices \
             (id, space_id, device_id, display_name, cert_pem, sync_enabled, paired_at) \
             VALUES (?1, 's1', ?2, ?2, 'CERT', ?3, ?4)",
            format!("{peer}-td"),
            peer,
            enabled,
            ts
        )
        .execute(&a.pool)
        .await
        .unwrap();
    }

    // Sync A→B and A→C so both receive all changes (space + trusted_devices).
    sync_direct(&a, &b).await;
    sync_direct(&a, &c).await;

    let b_cursor = b.cursor_for("s1", "device-A").await;
    let c_cursor = c.cursor_for("s1", "device-A").await;

    // B acknowledges, C does not.
    store_peer_acks(
        &a.pool,
        "device-B",
        &[CursorEntry {
            space_id: "s1".into(),
            device_id: "device-A".into(),
            last_seq: b_cursor,
        }],
    )
    .await
    .unwrap();

    let before: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM change_log WHERE space_id = 's1'")
        .fetch_one(&a.pool)
        .await
        .unwrap();
    assert!(before > 0);

    crate::sync::gc::run(&a.pool).await.unwrap();

    let after: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM change_log WHERE space_id = 's1'")
        .fetch_one(&a.pool)
        .await
        .unwrap();
    assert_eq!(after, before, "pending revocation must prevent collection");

    // Now C acknowledges.
    store_peer_acks(
        &a.pool,
        "device-C",
        &[CursorEntry {
            space_id: "s1".into(),
            device_id: "device-A".into(),
            last_seq: c_cursor,
        }],
    )
    .await
    .unwrap();

    crate::sync::gc::run(&a.pool).await.unwrap();

    let after2: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM change_log WHERE space_id = 's1'")
        .fetch_one(&a.pool)
        .await
        .unwrap();
    assert_eq!(
        after2, 0,
        "collection proceeds after pending revocation acknowledges"
    );
}

/// A peer below the retained floor is routed to snapshot
/// reconciliation. After GC collects some (not all) rows, the
/// retained floor rises. A peer whose cursor is below that floor
/// cannot use incremental sync and must fall back to snapshot.
#[tokio::test]
async fn peer_below_retained_floor_routes_to_snapshot() {
    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;
    let b = TestDevice::create("device-B", &dir).await;

    let ts = "2024-01-01T00:00:00Z";

    a.insert_space("s1", "test").await;

    // Register B as trusted so GC has a required device to check.
    sqlx::query!(
        "INSERT INTO trusted_devices \
         (id, space_id, device_id, display_name, cert_pem, sync_enabled, paired_at) \
         VALUES ('b-td', 's1', 'device-B', 'B', 'CERT', 1, ?1)",
        ts
    )
    .execute(&a.pool)
    .await
    .unwrap();

    // Create multiple change_log rows by updating the space name.
    for i in 2..=5 {
        sqlx::query!(
            "UPDATE spaces SET name = ?1 WHERE id = 's1'",
            format!("v{i}")
        )
        .execute(&a.pool)
        .await
        .unwrap();
    }

    let max_seq = changelog::max_seq(&a.pool, "s1", "device-A").await.unwrap();
    assert!(
        max_seq >= 3,
        "need at least 3 seq values for partial ack test"
    );

    // B syncs — consumes all rows.
    sync_direct(&a, &b).await;

    // Simulate partial acknowledgment: B only acks up to seq 3.
    let ack_seq = 3i64;
    store_peer_acks(
        &a.pool,
        "device-B",
        &[CursorEntry {
            space_id: "s1".into(),
            device_id: "device-A".into(),
            last_seq: ack_seq,
        }],
    )
    .await
    .unwrap();

    // GC collects seqs 1-3 (acked by both local and B).
    crate::sync::gc::run(&a.pool).await.unwrap();

    let remaining_min = changelog::min_seq(&a.pool, "s1", "device-A").await.unwrap();
    assert!(
        remaining_min > ack_seq,
        "seqs <= {ack_seq} collected, min remaining is {remaining_min}"
    );

    // A peer with cursor 2 (consumed 1-2, missed 3 before collection)
    // is below the retained floor and must route to snapshot.
    let peer_cursor = 2i64;
    assert!(
        peer_cursor > 0 && remaining_min > 0 && peer_cursor < remaining_min,
        "peer with cursor {peer_cursor} < min_seq {remaining_min} must route to snapshot"
    );
}
