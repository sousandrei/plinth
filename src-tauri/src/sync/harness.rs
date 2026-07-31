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
use crate::sync::trust_mode::TrustMode;
use crate::sync::wire::{ChangeBatch, ChangeRow, CursorEntry};

/// Generate a self-signed cert whose DNS SAN is `device_id`. Used by
/// snapshot apply tests — `apply_snapshot_frame` validates the host
/// cert (Step 30.2) so a fake `"CERT"` string won't survive.
pub fn test_cert_pem(device_id: &str) -> String {
    use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};
    let key = KeyPair::generate().unwrap();
    let mut params = CertificateParams::new(vec![device_id.to_string()]).unwrap();
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, device_id);
    params.distinguished_name = dn;
    params.self_signed(&key).unwrap().pem()
}

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

    /// Register a peer device and grant it access to a space. Inserts
    /// into both `devices` and `space_devices` so the peer is recognized
    /// for TLS and for sync GC's required-devices check.
    pub async fn grant_peer(&self, space_id: &str, peer_device_id: &str, trust_mode: TrustMode) {
        sqlx::query!(
            "INSERT OR IGNORE INTO devices (device_id, cert_pem, display_name) \
             VALUES (?1, 'CERT', ?1)",
            peer_device_id
        )
        .execute(&self.pool)
        .await
        .unwrap();
        sqlx::query!(
            "INSERT INTO space_devices (space_id, device_id, trust_mode, paired_at) \
             VALUES (?1, ?2, ?3, '2024-01-01T00:00:00Z') \
             ON CONFLICT(space_id, device_id) DO UPDATE SET trust_mode = excluded.trust_mode",
            space_id,
            peer_device_id,
            trust_mode
        )
        .execute(&self.pool)
        .await
        .unwrap();
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

async fn apply_batch(to: &TestDevice, batch: ChangeBatch) {
    let transport_device_id = batch.transport_device_id.clone();
    apply_round_core(&to.pool, &transport_device_id, &[batch])
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
    a.grant_peer("s1", "device-B", TrustMode::Active).await;
    a.grant_peer("s1", "device-C", TrustMode::Active).await;

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
    b.grant_peer("s1", "device-C", TrustMode::Active).await;

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
        snapshot_schema_version: crate::sync::snapshot::SNAPSHOT_SCHEMA_VERSION,
        protocol_version: crate::sync::wire::PROTOCOL_VERSION,
        snapshot_id: "snap-test-1".into(),
        host_device_id: "device-A".into(),
        host_device_name: "host".into(),
        host_cert_pem: test_cert_pem("device-A"),
        high_water_vector: vec![],
        row_winners: vec![],
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
                created_at: ts.into(),
                updated_at: ts.into(),
            },
            WireUser {
                id: "u2".into(),
                name: "Bob".into(),
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
        devices: vec![],
        space_devices: vec![],
        device_user_grants: vec![],
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
        ChangeBatch {
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
        ChangeBatch {
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
        ChangeBatch {
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
        ChangeBatch {
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
        ChangeBatch {
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
        ChangeBatch {
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
/// A device with trust_mode = 'revoking' (pending revocation) still
/// appears in space_devices, so the GC keeps changes until it
/// acknowledges.
#[tokio::test]
async fn gc_preserves_changes_for_pending_revocation() {
    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;
    let b = TestDevice::create("device-B", &dir).await;
    let c = TestDevice::create("device-C", &dir).await;

    a.insert_space("s1", "test").await;

    // Register B and C as trusted devices BEFORE syncing. C has
    // trust_mode = 'revoking' (pending revocation). The inserts create
    // change_log rows that B and C need to receive and acknowledge.
    for (peer, mode) in &[
        ("device-B", TrustMode::Active),
        ("device-C", TrustMode::Revoking),
    ] {
        a.grant_peer("s1", peer, *mode).await;
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

    a.insert_space("s1", "test").await;

    // Register B as trusted so GC has a required device to check.
    a.grant_peer("s1", "device-B", TrustMode::Active).await;

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

// ---------------------------------------------------------------------------
// Verification tests for Step 29.8 — durable gap detection
// ---------------------------------------------------------------------------

/// Helper: query the durable gap state for an origin.
async fn gap_state(dev: &TestDevice, space_id: &str, origin: &str) -> (i64, i64, i64, i64) {
    let row = sqlx::query_file!("queries/sync/get_origin_gap_state.sql", space_id, origin)
        .fetch_one(&dev.pool)
        .await
        .unwrap();
    (
        row.high_water,
        row.retained_floor,
        row.live_max_seq,
        row.live_min_seq,
    )
}

/// Helper: check if reconciliation is needed for an origin.
fn needs_recon(
    high_water: i64,
    retained_floor: i64,
    live_max: i64,
    live_min: i64,
    peer_cursor: i64,
) -> bool {
    let v2_required = false; // not testing migration flag here
    let effective_high_water = high_water.max(live_max);
    let effective_floor = if live_min > 0 {
        live_min
    } else {
        retained_floor
    };
    v2_required
        || (effective_high_water > 0 && live_max == 0)
        || (peer_cursor > 0 && peer_cursor < effective_floor)
        || (peer_cursor > effective_high_water)
}

/// Cursor zero with empty retained history but nonzero high_water
/// causes reconciliation instead of an empty successful batch.
/// After GC collects all rows, the live log is empty (live_max = 0)
/// but high_water remembers the data existed.
#[tokio::test]
async fn cursor_zero_empty_history_nonzero_high_water_requires_recon() {
    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;
    let b = TestDevice::create("device-B", &dir).await;

    a.insert_space("s1", "test").await;

    // Register B as trusted + acknowledge to allow GC collection.
    a.grant_peer("s1", "device-B", TrustMode::Active).await;

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

    // GC collects all rows — log is now empty.
    crate::sync::gc::run(&a.pool).await.unwrap();

    let (hw, rf, lmax, lmin) = gap_state(&a, "s1", "device-A").await;
    assert_eq!(lmax, 0, "live log is empty");
    assert_eq!(lmin, 0, "live min is 0");
    assert!(hw > 0, "high_water remembers data existed");

    // Peer with cursor 0 must trigger reconciliation.
    assert!(
        needs_recon(hw, rf, lmax, lmin, 0),
        "cursor 0 + empty log + nonzero high_water must require reconciliation"
    );
}

/// Empty retained history with a peer cursor below the retained floor
/// causes reconciliation. After partial GC, the floor rises above
/// the peer's cursor.
#[tokio::test]
async fn cursor_below_retained_floor_requires_recon() {
    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;
    let b = TestDevice::create("device-B", &dir).await;

    a.insert_space("s1", "test").await;
    for i in 2..=5 {
        sqlx::query!(
            "UPDATE spaces SET name = ?1 WHERE id = 's1'",
            format!("v{i}")
        )
        .execute(&a.pool)
        .await
        .unwrap();
    }

    a.grant_peer("s1", "device-B", TrustMode::Active).await;

    sync_direct(&a, &b).await;

    // B acknowledges only up to seq 3.
    store_peer_acks(
        &a.pool,
        "device-B",
        &[CursorEntry {
            space_id: "s1".into(),
            device_id: "device-A".into(),
            last_seq: 3,
        }],
    )
    .await
    .unwrap();

    crate::sync::gc::run(&a.pool).await.unwrap();

    let (hw, rf, lmax, lmin) = gap_state(&a, "s1", "device-A").await;
    assert!(
        lmin > 3,
        "floor must be above seq 3 after partial collection"
    );

    // Peer with cursor 2 is below the floor.
    assert!(
        needs_recon(hw, rf, lmax, lmin, 2),
        "cursor 2 < floor {lmin} must require reconciliation"
    );

    // Peer with cursor at the floor does NOT need reconciliation.
    assert!(
        !needs_recon(hw, rf, lmax, lmin, lmin),
        "cursor at floor should NOT require reconciliation"
    );
}

/// A legacy poisoned cursor (cursor > high_water) causes reconciliation.
/// This can happen when V1 trigger re-stamping inflated the cursor
/// beyond the actual max seq the origin ever produced.
#[tokio::test]
async fn poisoned_cursor_above_high_water_requires_recon() {
    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;

    a.insert_space("s1", "test").await;

    // Without trusted devices or GC, origin_state is empty. The
    // live log has rows, so effective_high_water = live_max_seq.
    let (hw, rf, lmax, lmin) = gap_state(&a, "s1", "device-A").await;
    let effective_hw = hw.max(lmax);
    assert!(effective_hw > 0, "effective high_water must be nonzero");

    // Poisoned cursor: higher than any seq the origin ever produced.
    let poisoned = effective_hw + 100;
    assert!(
        needs_recon(hw, rf, lmax, lmin, poisoned),
        "cursor {poisoned} > high_water {effective_hw} must require reconciliation"
    );

    // Normal cursor (at high_water) does NOT need reconciliation
    // because the log still has rows (live_max > 0).
    assert!(
        !needs_recon(hw, rf, lmax, lmin, effective_hw),
        "cursor at high_water should NOT require reconciliation when log has rows"
    );
}

/// A space marked as requiring V2 reconciliation always triggers
/// snapshot, regardless of cursor state.
#[tokio::test]
async fn v2_reconciliation_flag_forces_snapshot() {
    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;

    a.insert_space("s1", "test").await;

    // Mark the space as requiring V2 reconciliation (simulates a
    // migrated space that needs snapshot reconciliation).
    sqlx::query!("INSERT INTO v2_reconciliation (space_id, required) VALUES ('s1', 1)")
        .execute(&a.pool)
        .await
        .unwrap();

    let required: i64 = sqlx::query_file_scalar!("queries/sync/get_v2_reconciliation.sql", "s1")
        .fetch_one(&a.pool)
        .await
        .unwrap();
    assert_eq!(required, 1, "space must be marked for V2 reconciliation");

    let (hw, _rf, lmax, lmin) = gap_state(&a, "s1", "device-A").await;

    // Even with cursor at high_water, V2 flag forces reconciliation.
    let v2_required = true;
    let needs = v2_required
        || (hw.max(lmax) > 0 && lmax == 0)
        || (lmax > 0 && lmax < lmin)
        || (hw.max(lmax) > 0 && hw.max(lmax) < hw.max(lmax));
    assert!(needs, "V2 reconciliation flag must force snapshot");
}

// ---------------------------------------------------------------------------
// Verification tests for Step 30.1 — normalize installation identity
// ---------------------------------------------------------------------------

/// Existing peers with different legacy row IDs converge to one
/// logical grant. The new `space_devices` table is keyed by
/// `(space_id, device_id)` — no random id column. Two `ChangeBatch`
/// rows for the same `(space_id, device_id)` upsert into one row.
#[tokio::test]
async fn space_devices_converge_to_one_logical_grant() {
    use crate::sync::payloads::SpaceDevicePayload;
    use crate::sync::wire::ChangeRow;

    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;
    let ts = "2024-01-01T00:00:00Z";

    a.insert_space("s1", "test").await;
    a.grant_peer("s1", "peer-1", TrustMode::Active).await;

    // Apply a changelog row from the peer for the same (space_id, device_id).
    let mut tx = a.pool.begin().await.unwrap();
    let row = ChangeRow {
        id: "cl-1".into(),
        space_id: "s1".into(),
        table_name: "space_devices".into(),
        row_id: "s1:peer-1".into(),
        operation: "insert".into(),
        payload: Some(TablePayload::SpaceDevice(SpaceDevicePayload {
            space_id: "s1".into(),
            device_id: "peer-1".into(),
            trust_mode: TrustMode::Revoking, // even with different trust_mode
            paired_at: ts.into(),
        })),
        seq: 1,
        device_id: "peer-1".into(),
        changed_at: ts.into(),
    };
    crate::sync::apply::apply_change(&mut tx, &row)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let count: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM space_devices WHERE space_id = 's1' AND device_id = 'peer-1'"
    )
    .fetch_one(&a.pool)
    .await
    .unwrap();
    assert_eq!(count, 1, "converge to one logical grant");

    let trust_mode: String = sqlx::query_scalar!(
        "SELECT trust_mode FROM space_devices WHERE space_id = 's1' AND device_id = 'peer-1'"
    )
    .fetch_one(&a.pool)
    .await
    .unwrap();
    assert_eq!(trust_mode, "revoking", "latest value wins on conflict");
}

/// Removing one space grant does not remove the same installation
/// from another space. The `devices` table is separate from
/// `space_devices` — revoking access in one space only deletes the
/// `space_devices` row, leaving the `devices` row intact.
#[tokio::test]
async fn space_grant_revoke_preserves_installation() {
    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;

    a.insert_space("s1", "test").await;
    a.insert_space("s2", "other").await;
    a.grant_peer("s1", "peer-1", TrustMode::Active).await;
    a.grant_peer("s2", "peer-1", TrustMode::Active).await;

    let count_before: i64 =
        sqlx::query_scalar!("SELECT COUNT(*) FROM space_devices WHERE device_id = 'peer-1'")
            .fetch_one(&a.pool)
            .await
            .unwrap();
    assert_eq!(count_before, 2);

    // Revoke from s1 only.
    sqlx::query_file!("queries/sync/delete_space_device.sql", "s1", "peer-1")
        .execute(&a.pool)
        .await
        .unwrap();

    let count_after: i64 =
        sqlx::query_scalar!("SELECT COUNT(*) FROM space_devices WHERE device_id = 'peer-1'")
            .fetch_one(&a.pool)
            .await
            .unwrap();
    assert_eq!(count_after, 1, "only s1 grant removed");

    let s2_grant: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM space_devices WHERE space_id = 's2' AND device_id = 'peer-1'"
    )
    .fetch_one(&a.pool)
    .await
    .unwrap();
    assert_eq!(s2_grant, 1, "s2 grant preserved");

    // The devices row must still exist.
    let device_exists: i64 =
        sqlx::query_scalar!("SELECT COUNT(*) FROM devices WHERE device_id = 'peer-1'")
            .fetch_one(&a.pool)
            .await
            .unwrap();
    assert_eq!(device_exists, 1, "installation identity preserved");
}

// ---------------------------------------------------------------------------
// Verification tests for Step 30.2 — validate certificate ingress
// ---------------------------------------------------------------------------

/// A well-formed cert whose SAN matches `device_id` is accepted;
/// cert_der and fingerprint are populated.
#[tokio::test]
async fn pairing_with_valid_cert_populates_fingerprint() {
    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;

    a.insert_space("s1", "test").await;

    let peer_id = "peer-1";
    let cert = test_cert_pem(peer_id);
    crate::sync::pairing::upsert_device_and_grant_for_test(&a.pool, "s1", peer_id, "Peer 1", &cert)
        .await
        .expect("valid cert must be accepted");

    let fp: String =
        sqlx::query_scalar!("SELECT fingerprint FROM devices WHERE device_id = 'peer-1'")
            .fetch_one(&a.pool)
            .await
            .unwrap();
    assert_eq!(fp.len(), 64, "fingerprint must be 64-char SHA-256 hex");
    assert!(fp.chars().all(|c| c.is_ascii_hexdigit()));

    let der_bytes: i64 = sqlx::query_scalar::<_, i64>(
        "SELECT LENGTH(cert_der) FROM devices WHERE device_id = 'peer-1'",
    )
    .fetch_one(&a.pool)
    .await
    .unwrap_or(0);
    assert!(der_bytes > 0, "cert_der must be populated");
}

/// A cert whose SAN does not match the claimed `device_id` is
/// rejected and quarantined.
#[tokio::test]
async fn pairing_with_mismatched_san_is_quarantined() {
    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;

    a.insert_space("s1", "test").await;

    let cert_for_other = test_cert_pem("other-device");
    let result = crate::sync::pairing::upsert_device_and_grant_for_test(
        &a.pool,
        "s1",
        "peer-1",
        "Peer 1",
        &cert_for_other,
    )
    .await;
    assert!(result.is_err(), "SAN mismatch must reject");

    let count: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM devices WHERE device_id = 'peer-1'")
        .fetch_one(&a.pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "rejected cert must not create devices row");

    let quarantined: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM quarantined_devices WHERE claimed_device_id = 'peer-1'"
    )
    .fetch_one(&a.pool)
    .await
    .unwrap();
    assert_eq!(quarantined, 1, "rejected cert must be quarantined");
}

/// Two device_ids sharing the same fingerprint is rejected — one
/// cert is one installation.
#[tokio::test]
async fn pairing_with_fingerprint_collision_is_quarantined() {
    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;

    a.insert_space("s1", "test").await;
    a.insert_space("s2", "other").await;

    let cert = test_cert_pem("peer-1");
    // First pairing accepts the cert under device_id "peer-1".
    crate::sync::pairing::upsert_device_and_grant_for_test(
        &a.pool, "s1", "peer-1", "Peer 1", &cert,
    )
    .await
    .expect("first pairing succeeds");

    // A second pairing presenting the same cert under a different
    // device_id must be rejected.
    let result = crate::sync::pairing::upsert_device_and_grant_for_test(
        &a.pool, "s2", "peer-2", "Peer 2", &cert,
    )
    .await;
    assert!(result.is_err(), "fingerprint collision must reject");

    let peer_2_count: i64 =
        sqlx::query_scalar!("SELECT COUNT(*) FROM devices WHERE device_id = 'peer-2'")
            .fetch_one(&a.pool)
            .await
            .unwrap();
    assert_eq!(peer_2_count, 0, "colliding device_id must not be persisted");

    let quarantined: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM quarantined_devices WHERE claimed_device_id = 'peer-2'"
    )
    .fetch_one(&a.pool)
    .await
    .unwrap();
    assert_eq!(
        quarantined, 1,
        "colliding fingerprint must be quarantined under the new device_id"
    );
}

/// A device_id presenting a different fingerprint than the one
/// already stored is rejected — the same installation MUST always
/// present the same cert.
#[tokio::test]
async fn pairing_with_changed_fingerprint_is_quarantined() {
    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;

    a.insert_space("s1", "test").await;

    let cert_v1 = test_cert_pem("peer-1");
    crate::sync::pairing::upsert_device_and_grant_for_test(
        &a.pool, "s1", "peer-1", "Peer 1", &cert_v1,
    )
    .await
    .expect("first pairing succeeds");

    // Generate a different cert for the same claimed device_id.
    let cert_v2 = test_cert_pem("peer-1");
    assert_ne!(
        cert_v1, cert_v2,
        "two fresh certs for the same device_id must differ"
    );

    let result = crate::sync::pairing::upsert_device_and_grant_for_test(
        &a.pool, "s1", "peer-1", "Peer 1", &cert_v2,
    )
    .await;
    assert!(
        result.is_err(),
        "fingerprint change for same device_id must reject"
    );

    let quarantined: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM quarantined_devices WHERE claimed_device_id = 'peer-1'"
    )
    .fetch_one(&a.pool)
    .await
    .unwrap();
    assert_eq!(quarantined, 1, "changed fingerprint must be quarantined");
}

/// Garbage PEM input is quarantined without a fingerprint (we never
/// got far enough to compute one).
#[tokio::test]
async fn pairing_with_malformed_pem_is_quarantined() {
    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;

    a.insert_space("s1", "test").await;

    let result = crate::sync::pairing::upsert_device_and_grant_for_test(
        &a.pool,
        "s1",
        "peer-1",
        "Peer 1",
        "this is not a cert",
    )
    .await;
    assert!(result.is_err(), "malformed PEM must reject");

    let quarantined: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM quarantined_devices \
         WHERE claimed_device_id = 'peer-1' AND fingerprint IS NULL"
    )
    .fetch_one(&a.pool)
    .await
    .unwrap();
    assert_eq!(
        quarantined, 1,
        "malformed PEM must be quarantined without fingerprint"
    );
}

// ---------------------------------------------------------------------------
// Verification tests for Step 30.4 — separate trust modes
// ---------------------------------------------------------------------------

/// A trust_mode transition on the originating device is captured in
/// change_log and propagates to the remote peer. Step 30.4's `set_
/// space_device_trust_mode` flow is exercised here via the underlying
/// SQL query the Tauri command wraps.
#[tokio::test]
async fn trust_mode_revoking_propagates_through_change_log() {
    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;
    let b = TestDevice::create("device-B", &dir).await;

    a.insert_space("s1", "test").await;
    a.grant_peer("s1", "device-B", TrustMode::Active).await;
    sync_direct(&a, &b).await;

    // Originator transitions the grant to 'revoking'.
    sqlx::query_file!(
        "queries/sync/update_space_device_trust_mode.sql",
        "s1",
        "device-B",
        TrustMode::Revoking
    )
    .execute(&a.pool)
    .await
    .unwrap();

    sync_direct(&a, &b).await;

    let mode: String = sqlx::query_scalar!(
        "SELECT trust_mode FROM space_devices WHERE space_id = 's1' AND device_id = 'device-B'"
    )
    .fetch_one(&b.pool)
    .await
    .unwrap();
    assert_eq!(
        mode, "revoking",
        "trust_mode transition must propagate through change_log"
    );
}

/// A grant that has been transitioned back to 'active' from 'revoking'
/// propagates correctly. This is the "we changed our mind" path.
#[tokio::test]
async fn trust_mode_revival_to_active_propagates() {
    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;
    let b = TestDevice::create("device-B", &dir).await;

    a.insert_space("s1", "test").await;
    a.grant_peer("s1", "device-B", TrustMode::Revoking).await;
    sync_direct(&a, &b).await;

    // Verify the initial state landed.
    let mode: String = sqlx::query_scalar!(
        "SELECT trust_mode FROM space_devices WHERE space_id = 's1' AND device_id = 'device-B'"
    )
    .fetch_one(&b.pool)
    .await
    .unwrap();
    assert_eq!(mode, "revoking", "initial revoking state must propagate");

    sqlx::query_file!(
        "queries/sync/update_space_device_trust_mode.sql",
        "s1",
        "device-B",
        TrustMode::Active
    )
    .execute(&a.pool)
    .await
    .unwrap();

    sync_direct(&a, &b).await;

    let mode: String = sqlx::query_scalar!(
        "SELECT trust_mode FROM space_devices WHERE space_id = 's1' AND device_id = 'device-B'"
    )
    .fetch_one(&b.pool)
    .await
    .unwrap();
    assert_eq!(
        mode, "active",
        "trust_mode transition to active must propagate"
    );
}

/// A grant transitioned to `revocation_only` propagates. The remote
/// peer must apply the new trust_mode, even though it can't ship us
/// data while in that state.
#[tokio::test]
async fn trust_mode_revocation_only_propagates() {
    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;
    let b = TestDevice::create("device-B", &dir).await;

    a.insert_space("s1", "test").await;
    a.grant_peer("s1", "device-B", TrustMode::Active).await;
    sync_direct(&a, &b).await;

    sqlx::query_file!(
        "queries/sync/update_space_device_trust_mode.sql",
        "s1",
        "device-B",
        TrustMode::RevocationOnly
    )
    .execute(&a.pool)
    .await
    .unwrap();

    sync_direct(&a, &b).await;

    let mode: String = sqlx::query_scalar!(
        "SELECT trust_mode FROM space_devices WHERE space_id = 's1' AND device_id = 'device-B'"
    )
    .fetch_one(&b.pool)
    .await
    .unwrap();
    assert_eq!(
        mode, "revocation_only",
        "revocation_only transition must propagate"
    );
}

/// An update that targets a non-existent grant is a no-op at the SQL
/// level. The Tauri command layer translates `rows_affected == 0`
/// into a `NotFound` error — this test pins the underlying SQL
/// behavior the command relies on.
#[tokio::test]
async fn update_trust_mode_for_missing_grant_is_noop() {
    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;

    a.insert_space("s1", "test").await;

    let updated = sqlx::query_file!(
        "queries/sync/update_space_device_trust_mode.sql",
        "s1",
        "missing-device",
        TrustMode::Revoking
    )
    .execute(&a.pool)
    .await
    .unwrap();

    assert_eq!(updated.rows_affected(), 0, "no row exists to update");
}

/// The CHECK constraint on `space_devices.trust_mode` rejects unknown
/// values. The Rust `TrustMode::parse` defends at the parse layer;
/// this test pins the belt-and-suspenders SQL-level check, which would
/// catch a future bug in `TrustMode::parse` or a malformed JSON payload.
#[tokio::test]
async fn sql_check_constraint_rejects_unknown_trust_mode() {
    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;

    a.insert_space("s1", "test").await;
    sqlx::query!(
        "INSERT INTO devices (device_id, cert_pem, display_name) VALUES ('peer-1', 'CERT', 'P')"
    )
    .execute(&a.pool)
    .await
    .unwrap();

    // Bypass the TrustMode enum by writing the column directly.
    let result = sqlx::query!(
        "INSERT INTO space_devices (space_id, device_id, trust_mode, paired_at) \
         VALUES ('s1', 'peer-1', 'unknown_state', '2024-01-01T00:00:00Z')"
    )
    .execute(&a.pool)
    .await;

    assert!(
        result.is_err(),
        "SQL CHECK constraint must reject unknown trust_mode values"
    );
    let err = result.err().unwrap();
    assert!(
        err.to_string().contains("CHECK constraint failed"),
        "expected CHECK constraint error, got: {err}"
    );
}

// ---------------------------------------------------------------------------
// Verification tests for Step 31.1 — capture a consistent snapshot
// ---------------------------------------------------------------------------

/// Step 31.1: `collect_snapshot` executes inside a single read transaction and
/// captures the complete, consistent space state:
/// - Header versions (snapshot schema version, protocol version, snapshot ID)
/// - High-water vector and row winners/tombstones
/// - Space metadata, members, and users without local credentials
/// - Device roster and space grants (including trust modes)
/// - Explicit empty sections (categories, accounts, etc. as empty Vecs)
#[tokio::test]
async fn collect_snapshot_captures_complete_consistent_state() {
    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;
    let ts = "2024-01-01T00:00:00Z";

    a.insert_space("s1", "Main Space").await;

    // Create a user and a local credential on this installation.
    sqlx::query!(
        "INSERT INTO users (id, name, created_at, updated_at) \
         VALUES ('u1', 'Alice', ?1, ?1)",
        ts
    )
    .execute(&a.pool)
    .await
    .unwrap();

    sqlx::query!(
        "INSERT INTO local_user_credentials (user_id, pin_hash, updated_at) \
         VALUES ('u1', 'argon2_secret_hash', ?1)",
        ts
    )
    .execute(&a.pool)
    .await
    .unwrap();

    sqlx::query!(
        "INSERT INTO space_members (space_id, user_id, role, joined_at) \
         VALUES ('s1', 'u1', 'owner', ?1)",
        ts
    )
    .execute(&a.pool)
    .await
    .unwrap();

    // Register a peer device and grant
    a.grant_peer("s1", "peer-B", TrustMode::Revoking).await;

    let snap = crate::sync::snapshot::collect_snapshot(
        &a.pool,
        None,
        "s1",
        "device-A".into(),
        "Host A".into(),
        test_cert_pem("device-A"),
    )
    .await
    .expect("collect_snapshot must succeed");

    // 1. Header and metadata verification
    assert_eq!(
        snap.snapshot_schema_version,
        crate::sync::snapshot::SNAPSHOT_SCHEMA_VERSION
    );
    assert_eq!(snap.protocol_version, crate::sync::wire::PROTOCOL_VERSION);
    assert!(snap.snapshot_id.starts_with("snap_"));
    assert_eq!(snap.host_device_id, "device-A");

    // 2. High water vector & row winners (explicitly represented as Vec)
    assert!(
        snap.high_water_vector.is_empty(),
        "high_water_vector is explicitly [] when no origin_state exists"
    );
    assert!(
        !snap.row_winners.is_empty(),
        "row_winners contains entry for the space"
    );

    // 3. Users contain profile data only.
    let alice = snap
        .users
        .iter()
        .find(|u| u.id == "u1")
        .expect("Alice must be in users");
    assert_eq!(alice.name, "Alice");

    // 4. Device and space grants
    assert!(snap.devices.iter().any(|d| d.device_id == "peer-B"));
    let b_grant = snap
        .space_devices
        .iter()
        .find(|sd| sd.device_id == "peer-B")
        .expect("peer-B grant present");
    assert_eq!(b_grant.trust_mode, TrustMode::Revoking);

    // 5. Explicit empty sections
    assert!(snap.categories.is_empty());
    assert!(snap.accounts.is_empty());
    assert!(snap.transactions.is_empty());
    assert!(snap.account_summaries.is_empty());
}

#[tokio::test]
async fn remote_user_upsert_cannot_replace_local_credential() {
    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;
    let ts = "2024-01-01T00:00:00Z";

    sqlx::query!(
        "INSERT INTO users (id, name, created_at, updated_at) VALUES ('u1', 'Alice', ?1, ?1)",
        ts
    )
    .execute(&a.pool)
    .await
    .unwrap();
    sqlx::query!(
        "INSERT INTO local_user_credentials (user_id, pin_hash, updated_at) \
         VALUES ('u1', 'local_hash', ?1)",
        ts
    )
    .execute(&a.pool)
    .await
    .unwrap();

    sqlx::query_file!(
        "queries/sync/apply/upsert_user.sql",
        "u1",
        "Alice from peer",
        ts,
        ts
    )
    .execute(&a.pool)
    .await
    .unwrap();

    let local_hash: String =
        sqlx::query_scalar("SELECT pin_hash FROM local_user_credentials WHERE user_id = 'u1'")
            .fetch_one(&a.pool)
            .await
            .unwrap();
    assert_eq!(local_hash, "local_hash");

    sqlx::query_file!(
        "queries/sync/apply/upsert_user.sql",
        "u2",
        "Bob from peer",
        ts,
        ts
    )
    .execute(&a.pool)
    .await
    .unwrap();

    let has_local_credential: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM local_user_credentials WHERE user_id = 'u2')",
    )
    .fetch_one(&a.pool)
    .await
    .unwrap();
    assert!(!has_local_credential);
}

#[tokio::test]
async fn removing_person_retires_only_exclusive_devices() {
    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;
    let ts = "2024-01-01T00:00:00Z";

    a.insert_space("s1", "Main").await;
    sqlx::query!(
        "INSERT INTO users (id, name, created_at, updated_at) VALUES ('u1', 'Alice', ?1, ?1)",
        ts
    )
    .execute(&a.pool)
    .await
    .unwrap();
    sqlx::query!(
        "INSERT INTO space_members (space_id, user_id, role, joined_at) \
         VALUES ('s1', 'u1', 'owner', ?1)",
        ts
    )
    .execute(&a.pool)
    .await
    .unwrap();
    sqlx::query!(
        "INSERT INTO users (id, name, created_at, updated_at) VALUES ('u2', 'Bob', ?1, ?1)",
        ts
    )
    .execute(&a.pool)
    .await
    .unwrap();
    sqlx::query!(
        "INSERT INTO space_members (space_id, user_id, role, joined_at) \
         VALUES ('s1', 'u2', 'member', ?1)",
        ts
    )
    .execute(&a.pool)
    .await
    .unwrap();

    for device_id in ["exclusive-device", "shared-device"] {
        sqlx::query!(
            "INSERT INTO devices (device_id, cert_pem, display_name) VALUES (?1, 'CERT', ?1)",
            device_id
        )
        .execute(&a.pool)
        .await
        .unwrap();
        sqlx::query!(
            "INSERT INTO space_devices (space_id, device_id, paired_at) VALUES ('s1', ?1, ?2)",
            device_id,
            ts
        )
        .execute(&a.pool)
        .await
        .unwrap();
    }

    sqlx::query!(
        "INSERT INTO device_user_grants (space_id, device_id, user_id, granted_at) \
         VALUES ('s1', 'exclusive-device', 'u2', ?1), \
                ('s1', 'shared-device', 'u2', ?1), \
                ('s1', 'shared-device', 'u1', ?1)",
        ts
    )
    .execute(&a.pool)
    .await
    .unwrap();

    let exclusive = sqlx::query_file!(
        "queries/spaces/list_exclusive_device_ids_for_user.sql",
        "s1",
        "u2"
    )
    .fetch_all(&a.pool)
    .await
    .unwrap();
    assert_eq!(exclusive.len(), 1);
    assert_eq!(exclusive[0].device_id, "exclusive-device");

    sqlx::query_file!(
        "queries/sync/delete_space_device.sql",
        "s1",
        "exclusive-device"
    )
    .execute(&a.pool)
    .await
    .unwrap();
    sqlx::query_file!("queries/spaces/remove_space_member.sql", "s1", "u2")
        .execute(&a.pool)
        .await
        .unwrap();

    let remaining_devices: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM space_devices WHERE space_id = 's1' AND device_id IN ('exclusive-device', 'shared-device')",
    )
    .fetch_one(&a.pool)
    .await
    .unwrap();
    assert_eq!(remaining_devices, 1);

    let remaining_shared_grants: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM device_user_grants WHERE space_id = 's1' AND device_id = 'shared-device'",
    )
    .fetch_one(&a.pool)
    .await
    .unwrap();
    assert_eq!(remaining_shared_grants, 1);
}

// ---------------------------------------------------------------------------
// Verification tests for Step 31.2 — merge without echoes (tombstone honoring)
// ---------------------------------------------------------------------------

/// Step 31.2: when a `row_winner` arrives with `deleted = 1`, the
/// receiver must remove the corresponding row from the source table —
/// stale state from before the snapshot was captured should not survive
/// the merge.
///
/// The test constructs a snapshot that includes a category row followed
/// by a tombstoned row_winner, applies it through the production
/// `apply_guard` + `apply_snapshot_frame` path, and verifies the
/// category is gone afterwards.
#[tokio::test]
async fn snapshot_apply_deletes_tombstoned_rows() {
    use crate::sync::apply_guard::run_as_device;
    use crate::sync::snapshot::{
        SnapshotFrame, SpaceSnapshot, WireCategory, WireRowWinner, WireSpace,
    };

    let dir = TempDir::new().unwrap();
    let b = TestDevice::create("device-B", &dir).await;

    // Pre-seed receiver B with a stale category "stale_cat" that is
    // *not* referenced by any tombstone — this stays untouched, since
    // the snapshot only authoritatively deletes rows it explicitly
    // tombstones. Step 31.2 is about honoring tombstones, not about
    // wiping unmentioned rows.
    let ts = "2024-01-01T00:00:00Z";
    b.insert_space("s1", "Main").await;
    sqlx::query!(
        "INSERT INTO categories (id, name, color, space_id) VALUES ('stale_cat', 'Stale', '#fff', 's1')"
    )
    .execute(&b.pool)
    .await
    .unwrap();

    // Host sends: Space + Categories (includes "old_cat" to be deleted
    // and "keep_cat" to survive) + RowWinners (tombstone for "old_cat").
    let snapshot = SpaceSnapshot {
        snapshot_schema_version: crate::sync::snapshot::SNAPSHOT_SCHEMA_VERSION,
        protocol_version: crate::sync::wire::PROTOCOL_VERSION,
        snapshot_id: "snap-tombstone-1".into(),
        host_device_id: "device-A".into(),
        host_device_name: "Host A".into(),
        host_cert_pem: test_cert_pem("device-A"),
        high_water_vector: vec![],
        row_winners: vec![WireRowWinner {
            space_id: "s1".into(),
            table_name: "categories".into(),
            row_id: "old_cat".into(),
            winning_seq: 5,
            winning_origin: "device-A".into(),
            deleted: 1,
        }],
        space: WireSpace {
            id: "s1".into(),
            name: "Main".into(),
            created_at: ts.into(),
            updated_at: ts.into(),
        },
        members: vec![],
        users: vec![],
        categories: vec![
            WireCategory {
                id: "old_cat".into(),
                name: "To Be Deleted".into(),
                color: "#000".into(),
                space_id: "s1".into(),
            },
            WireCategory {
                id: "keep_cat".into(),
                name: "Survives".into(),
                color: "#0f0".into(),
                space_id: "s1".into(),
            },
        ],
        accounts: vec![],
        transactions: vec![],
        account_summaries: vec![],
        space_settings: vec![],
        model_versions: vec![],
        devices: vec![],
        space_devices: vec![],
        device_user_grants: vec![],
    };

    // Production apply path: under apply_guard, with host as author.
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
                &SnapshotFrame::Categories(snap.categories.clone()),
            )
            .await?;
            crate::sync::snapshot::apply_snapshot_frame(
                tx,
                &snap,
                &SnapshotFrame::RowWinners(snap.row_winners.clone()),
            )
            .await?;
            Ok::<(), crate::error::AppError>(())
        })
    })
    .await
    .expect("snapshot apply must succeed");

    // Tombstoned category removed
    let old_count: i64 =
        sqlx::query_scalar!("SELECT COUNT(*) FROM categories WHERE id = 'old_cat'")
            .fetch_one(&b.pool)
            .await
            .unwrap();
    assert_eq!(old_count, 0, "tombstoned category must be removed");

    // Non-tombstoned category preserved
    let keep_count: i64 =
        sqlx::query_scalar!("SELECT COUNT(*) FROM categories WHERE id = 'keep_cat'")
            .fetch_one(&b.pool)
            .await
            .unwrap();
    assert_eq!(keep_count, 1, "non-tombstoned category must survive");

    // Untouched local category (no tombstone) is left alone.
    let stale_count: i64 =
        sqlx::query_scalar!("SELECT COUNT(*) FROM categories WHERE id = 'stale_cat'")
            .fetch_one(&b.pool)
            .await
            .unwrap();
    assert_eq!(
        stale_count, 1,
        "category without a tombstone must be left alone"
    );

    // row_winner table records the tombstone
    let winner_deleted: i64 = sqlx::query_scalar!(
        "SELECT deleted FROM row_winners WHERE space_id='s1' AND table_name='categories' AND row_id='old_cat'"
    )
    .fetch_one(&b.pool)
    .await
    .unwrap();
    assert_eq!(winner_deleted, 1, "row_winner must record tombstone");
}

/// Step 31.2: `install_snapshot_cursors` writes the cursor vector to
/// `sync_cursors` in a separate transaction so a crash between snapshot
/// apply and cursor installation is detectable (next sync re-snapshots).
/// `MAX(...)` clamping inside `upsert_cursor.sql` guarantees the cursor
/// only moves forward.
#[tokio::test]
async fn snapshot_installs_cursor_vector_post_commit() {
    use crate::sync::snapshot::WireOriginState;

    let dir = TempDir::new().unwrap();
    let b = TestDevice::create("device-B", &dir).await;
    b.insert_space("s1", "shared").await;

    // Two origins in the vector — host at seq 42, peer at seq 17.
    let vector = vec![
        WireOriginState {
            origin_device_id: "device-A".into(),
            high_water: 42,
            retained_floor: 10,
        },
        WireOriginState {
            origin_device_id: "peer-C".into(),
            high_water: 17,
            retained_floor: 5,
        },
    ];

    crate::sync::snapshot::install_snapshot_cursors(&b.pool, "s1", &vector, "device-A")
        .await
        .expect("install_snapshot_cursors must succeed");

    let host_cursor: i64 = sqlx::query_scalar!(
        "SELECT last_seq FROM sync_cursors WHERE space_id='s1' AND peer_device_id='device-A'"
    )
    .fetch_one(&b.pool)
    .await
    .unwrap();
    assert_eq!(host_cursor, 42, "host cursor advanced to high_water");

    let peer_cursor: i64 = sqlx::query_scalar!(
        "SELECT last_seq FROM sync_cursors WHERE space_id='s1' AND peer_device_id='peer-C'"
    )
    .fetch_one(&b.pool)
    .await
    .unwrap();
    assert_eq!(peer_cursor, 17, "peer cursor advanced to high_water");

    // Calling install a second time with a SMALLER seq must not regress
    // the cursor — MAX(...) clamp is the safety belt.
    let smaller = vec![WireOriginState {
        origin_device_id: "device-A".into(),
        high_water: 5,
        retained_floor: 0,
    }];
    crate::sync::snapshot::install_snapshot_cursors(&b.pool, "s1", &smaller, "device-A")
        .await
        .expect("install_snapshot_cursors (regression test) must succeed");

    let host_cursor: i64 = sqlx::query_scalar!(
        "SELECT last_seq FROM sync_cursors WHERE space_id='s1' AND peer_device_id='device-A'"
    )
    .fetch_one(&b.pool)
    .await
    .unwrap();
    assert_eq!(host_cursor, 42, "cursor must NOT regress on smaller seq");

    // If the host isn't in the vector, the function should still install
    // it at the max observed high water (defensive fallback).
    let vector_no_host = vec![WireOriginState {
        origin_device_id: "peer-C".into(),
        high_water: 99,
        retained_floor: 0,
    }];
    sqlx::query!("DELETE FROM sync_cursors WHERE space_id='s1' AND peer_device_id='device-A'")
        .execute(&b.pool)
        .await
        .unwrap();
    crate::sync::snapshot::install_snapshot_cursors(&b.pool, "s1", &vector_no_host, "device-A")
        .await
        .expect("install_snapshot_cursors (host fallback) must succeed");

    let host_cursor: i64 = sqlx::query_scalar!(
        "SELECT last_seq FROM sync_cursors WHERE space_id='s1' AND peer_device_id='device-A'"
    )
    .fetch_one(&b.pool)
    .await
    .unwrap();
    assert_eq!(
        host_cursor, 99,
        "host cursor installed at max high_water when not in vector"
    );
}

/// Step 31.2: `verify_snapshot_boundary` rejects snapshots whose
/// chunks disagree on the space_id or whose schema version is
/// unsupported. A mixed-space stream could otherwise poison a
/// partial apply that passes all per-row checks.
#[tokio::test]
async fn snapshot_rejects_inconsistent_space_ids() {
    use crate::sync::snapshot::{SnapshotFrame, WireSpace};
    use crate::sync::wire::SnapshotChunk;

    let good = SnapshotChunk {
        space_id: "s1".into(),
        frame: SnapshotFrame::Space(WireSpace {
            id: "s1".into(),
            name: "n".into(),
            created_at: "t".into(),
            updated_at: "t".into(),
        }),
    };
    let bad = SnapshotChunk {
        space_id: "s2".into(),
        frame: SnapshotFrame::Space(WireSpace {
            id: "s2".into(),
            name: "n".into(),
            created_at: "t".into(),
            updated_at: "t".into(),
        }),
    };

    // All-chunks-same-space: ok
    let result = crate::sync::snapshot::verify_snapshot_boundary(
        "s1",
        crate::sync::snapshot::SNAPSHOT_SCHEMA_VERSION,
        std::slice::from_ref(&good),
    );
    assert!(result.is_ok(), "uniform stream must pass");

    // Mixed-space: rejected
    let result = crate::sync::snapshot::verify_snapshot_boundary(
        "s1",
        crate::sync::snapshot::SNAPSHOT_SCHEMA_VERSION,
        &[good, bad],
    );
    let err = result.expect_err("mixed-space must be rejected");
    assert!(
        err.to_string().contains("snapshot boundary"),
        "error must mention boundary: {err}"
    );

    // Unsupported schema version: rejected
    let result = crate::sync::snapshot::verify_snapshot_boundary(
        "s1",
        crate::sync::snapshot::SNAPSHOT_SCHEMA_VERSION + 999,
        &[],
    );
    let err = result.expect_err("unsupported schema must be rejected");
    assert!(
        err.to_string()
            .contains("unsupported snapshot_schema_version"),
        "error must mention schema version: {err}"
    );

    // Empty expected space_id: rejected
    let result = crate::sync::snapshot::verify_snapshot_boundary(
        "",
        crate::sync::snapshot::SNAPSHOT_SCHEMA_VERSION,
        &[],
    );
    assert!(result.is_err(), "empty expected space_id must be rejected");
}

/// Step 31.2: snapshot application must NOT create any
/// `change_log` rows attributed to the local device. The `apply_guard`
/// suppresses triggers and stamps `applying_as_device` with the host,
/// so a receiver that applies a snapshot will not echo those rows back
/// to the network on the next sync — the host is the authoritative
/// author of every row the snapshot contains.
///
/// Verification: count change_log rows before vs after a snapshot apply
/// that materializes a new category, a new space grant, and a new
/// transaction. The counts must be identical.
#[tokio::test]
async fn snapshot_apply_creates_no_local_change_log() {
    use crate::sync::apply_guard::run_as_device;
    use crate::sync::snapshot::{
        SnapshotFrame, SpaceSnapshot, WireCategory, WireDevice, WireSpace, WireSpaceDevice,
        WireTransaction,
    };

    let dir = TempDir::new().unwrap();
    let b = TestDevice::create("device-B", &dir).await;
    let ts = "2024-01-01T00:00:00Z";
    b.insert_space("s1", "shared").await;

    // Register an account so the foreign key on transactions is satisfied.
    sqlx::query!(
        "INSERT INTO accounts (id, name, currency, account_type, account_source, color, space_id) \
         VALUES ('acc-1', 'Checking', 'USD', 'checking', 'manual', '#fff', 's1')"
    )
    .execute(&b.pool)
    .await
    .unwrap();

    let before: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM change_log")
        .fetch_one(&b.pool)
        .await
        .unwrap();
    // Note: B's local inserts (space, account) create change_log rows
    // attributed to 'device-B'. We snapshot the count so we can prove
    // the snapshot apply adds zero rows on top.

    // Build a snapshot from host A that contains fresh data. The
    // host's cert is generated once and reused in both the snapshot
    // header and the Devices chunk — different invocations of
    // `test_cert_pem` produce different fingerprints and would fail
    // the End frame's fingerprint-uniqueness check.
    let host_cert = test_cert_pem("device-A");
    let snapshot = SpaceSnapshot {
        snapshot_schema_version: crate::sync::snapshot::SNAPSHOT_SCHEMA_VERSION,
        protocol_version: crate::sync::wire::PROTOCOL_VERSION,
        snapshot_id: "snap-noecho".into(),
        host_device_id: "device-A".into(),
        host_device_name: "Host A".into(),
        host_cert_pem: host_cert.clone(),
        high_water_vector: vec![],
        row_winners: vec![],
        space: WireSpace {
            id: "s1".into(),
            name: "shared".into(),
            created_at: ts.into(),
            updated_at: ts.into(),
        },
        members: vec![],
        users: vec![],
        categories: vec![WireCategory {
            id: "cat-new".into(),
            name: "New".into(),
            color: "#0f0".into(),
            space_id: "s1".into(),
        }],
        accounts: vec![],
        transactions: vec![WireTransaction {
            id: "tx-1".into(),
            booking_date: "2024-01-01".into(),
            value_date: "2024-01-01".into(),
            reference: "ref".into(),
            text: "txn".into(),
            currency: "USD".into(),
            amount: -1000,
            balance: 9000,
            approved: 0,
            note: "n".into(),
            category: Some("cat-new".into()),
            account_id: "acc-1".into(),
        }],
        account_summaries: vec![],
        space_settings: vec![],
        model_versions: vec![],
        devices: vec![WireDevice {
            device_id: "device-A".into(),
            cert_pem: host_cert,
            display_name: "Host A".into(),
        }],
        space_devices: vec![WireSpaceDevice {
            space_id: "s1".into(),
            device_id: "device-A".into(),
            trust_mode: TrustMode::Active,
            paired_at: ts.into(),
        }],
        device_user_grants: vec![],
    };

    // Apply on B with the production apply_guard path.
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
                &SnapshotFrame::Categories(snap.categories.clone()),
            )
            .await?;
            crate::sync::snapshot::apply_snapshot_frame(
                tx,
                &snap,
                &SnapshotFrame::Transactions(snap.transactions.clone()),
            )
            .await?;
            crate::sync::snapshot::apply_snapshot_frame(
                tx,
                &snap,
                &SnapshotFrame::Devices(snap.devices.clone()),
            )
            .await?;
            crate::sync::snapshot::apply_snapshot_frame(
                tx,
                &snap,
                &SnapshotFrame::SpaceDevices(snap.space_devices.clone()),
            )
            .await?;
            crate::sync::snapshot::apply_snapshot_frame(tx, &snap, &SnapshotFrame::End).await?;
            Ok::<(), crate::error::AppError>(())
        })
    })
    .await
    .expect("snapshot apply must succeed");

    let after: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM change_log")
        .fetch_one(&b.pool)
        .await
        .unwrap();
    assert_eq!(
        after, before,
        "snapshot apply must NOT create any change_log rows"
    );

    // Any change_log rows that exist must be attributed to the local
    // device for legitimate pre-snapshot writes (B's space insert +
    // auto-updated_at + account insert), not the snapshot host.
    let local_rows: i64 =
        sqlx::query_scalar!("SELECT COUNT(*) FROM change_log WHERE device_id = 'device-B'")
            .fetch_one(&b.pool)
            .await
            .unwrap();
    let host_rows: i64 =
        sqlx::query_scalar!("SELECT COUNT(*) FROM change_log WHERE device_id = 'device-A'")
            .fetch_one(&b.pool)
            .await
            .unwrap();
    assert!(
        local_rows >= 1,
        "B has at least one pre-snapshot local row (space insert)"
    );
    assert_eq!(host_rows, 0, "no echo rows attributed to the snapshot host");
}

/// Step 31.3: a later pairing through A must receive the complete trust
/// roster, not just rows that happen to remain in A's change_log. C must be
/// able to resolve A and B from their certificates immediately after the
/// snapshot is applied.
#[tokio::test]
async fn snapshot_seeds_complete_trust_roster_for_later_pairing() {
    use tokio_rustls::rustls::pki_types::CertificateDer;

    use crate::sync::apply_guard::run_as_device;
    use crate::sync::snapshot::{SnapshotFrame, SpaceSnapshot};

    let dir = TempDir::new().unwrap();
    let a = TestDevice::create("device-A", &dir).await;
    let c = TestDevice::create("device-C", &dir).await;
    let ts = "2024-01-01T00:00:00Z";
    let cert_b = test_cert_pem("device-B");
    let cert_c = test_cert_pem("device-C");

    a.insert_space("s1", "shared").await;

    // A knows B and C before C pairs through A. These are durable roster
    // rows, so the snapshot remains complete even after old history is
    // removed.
    for (device_id, cert) in [("device-B", cert_b.clone()), ("device-C", cert_c)] {
        sqlx::query_file!(
            "queries/sync/upsert_device.sql",
            device_id,
            cert,
            Option::<Vec<u8>>::None,
            "",
            device_id,
        )
        .execute(&a.pool)
        .await
        .unwrap();
        sqlx::query!(
            "INSERT INTO space_devices (space_id, device_id, trust_mode, paired_at) \
             VALUES ('s1', ?1, 'active', ?2)",
            device_id,
            ts,
        )
        .execute(&a.pool)
        .await
        .unwrap();
    }

    // Simulate a host that has compacted/collected its old history. The
    // roster must still be available from the durable devices/grants.
    sqlx::query_file!("queries/spaces/delete_change_log_for_space.sql", "s1")
        .execute(&a.pool)
        .await
        .unwrap();

    let snapshot: SpaceSnapshot = crate::sync::snapshot::collect_snapshot(
        &a.pool,
        None,
        "s1",
        "device-A".into(),
        "Host A".into(),
        test_cert_pem("device-A"),
    )
    .await
    .expect("roster snapshot must succeed");

    assert!(snapshot.devices.iter().any(|d| d.device_id == "device-B"));
    assert!(snapshot.devices.iter().any(|d| d.device_id == "device-C"));
    assert!(
        snapshot
            .space_devices
            .iter()
            .any(|grant| grant.device_id == "device-B")
    );
    assert!(
        snapshot
            .space_devices
            .iter()
            .any(|grant| grant.device_id == "device-C")
    );

    let host_cert = snapshot.host_cert_pem.clone();
    let snapshot_for_apply = snapshot.clone();
    run_as_device(&c.pool, "device-A", move |tx| {
        Box::pin(async move {
            crate::sync::snapshot::apply_snapshot_frame(
                tx,
                &snapshot_for_apply,
                &SnapshotFrame::Space(snapshot_for_apply.space.clone()),
            )
            .await?;
            crate::sync::snapshot::apply_snapshot_frame(
                tx,
                &snapshot_for_apply,
                &SnapshotFrame::Devices(snapshot_for_apply.devices.clone()),
            )
            .await?;
            crate::sync::snapshot::apply_snapshot_frame(
                tx,
                &snapshot_for_apply,
                &SnapshotFrame::SpaceDevices(snapshot_for_apply.space_devices.clone()),
            )
            .await?;
            crate::sync::snapshot::apply_snapshot_frame(
                tx,
                &snapshot_for_apply,
                &SnapshotFrame::End,
            )
            .await?;
            Ok::<(), crate::error::AppError>(())
        })
    })
    .await
    .expect("roster snapshot apply must succeed");

    let mut host_reader = host_cert.as_bytes();
    let host_der = rustls_pemfile::certs(&mut host_reader)
        .next()
        .expect("host certificate must contain one certificate")
        .expect("host certificate must parse");
    let host_peer =
        crate::sync::cert_match::resolve_peer(&c.pool, &CertificateDer::from(host_der.to_vec()))
            .await
            .expect("host resolution must succeed")
            .expect("host must be trusted after snapshot");
    assert_eq!(host_peer.device_id, "device-A");
    assert!(host_peer.inbound_contains("s1"));

    let mut peer_reader = cert_b.as_bytes();
    let peer_der = rustls_pemfile::certs(&mut peer_reader)
        .next()
        .expect("peer certificate must contain one certificate")
        .expect("peer certificate must parse");
    let peer =
        crate::sync::cert_match::resolve_peer(&c.pool, &CertificateDer::from(peer_der.to_vec()))
            .await
            .expect("peer resolution must succeed")
            .expect("B must be trusted after snapshot");
    assert_eq!(peer.device_id, "device-B");
    assert!(peer.outbound_contains("s1"));
}
