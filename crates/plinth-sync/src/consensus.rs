//! Durable storage for ordering membership changes. Each space has its own
//! Raft group, with log entries and election votes in the sync database.

use std::{
    collections::BTreeSet,
    io::Cursor,
    ops::{Bound, RangeBounds},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use openraft::{
    BasicNode, Config, Entry, EntryPayload, ErrorSubject, ErrorVerb, LogId, Raft, RaftLogReader,
    RaftSnapshotBuilder, Snapshot, SnapshotMeta, StorageError, StorageIOError, StoredMembership,
    Vote,
    storage::{LogFlushed, LogState, RaftLogStorage, RaftStateMachine},
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use thiserror::Error;

use crate::{
    membership::{Change, DeviceId, Enrollment, Membership, Outcome, PeerContact},
    transport,
};

mod pairing;
mod replica;
pub mod rpc;
mod withdrawal;

pub use pairing::PairingHistory;
pub use replica::PermissionReplica;
pub use withdrawal::Withdrawal;

const FIRST_VOTER: u64 = 1;
const LOCAL_WRITE_TIMEOUT: Duration = Duration::from_secs(5);

#[cfg(test)]
mod cluster_tests;

openraft::declare_raft_types!(
    pub MembershipRaft:
        D = MembershipRequest,
        R = Outcome,
);

/// The transport must validate the author's device identity before proposing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MembershipRequest {
    Initialize {
        owner_account: String,
        first_device: DeviceId,
        #[serde(default)]
        first_contact: Option<PeerContact>,
    },
    Change {
        author: Enrollment,
        change: Change,
    },
    RevokeDevice {
        author: Enrollment,
        enrollment: Enrollment,
    },
}

/// Start a permission history for a freshly created local space. Imported and
/// joined spaces must obtain their existing history from their peers.
pub async fn initialize_local_space(
    sync_db: PathBuf,
    identity_dir: PathBuf,
    space_id: String,
    owner_account: String,
    owner_contact: PeerContact,
) -> transport::Result<()> {
    let network = rpc::PermissionNetwork::new(identity_dir, space_id.clone(), |_| None);
    let store =
        tokio::task::spawn_blocking(move || MembershipLog::open(sync_db, space_id)).await??;
    let node = Raft::new(
        FIRST_VOTER,
        Arc::new(Config::default()),
        network,
        store.clone(),
        store,
    )
    .await?;
    let initialized: transport::Result<()> = async {
        if node.is_initialized().await? {
            return Err("space already has a permission history".into());
        }
        tokio::time::timeout(LOCAL_WRITE_TIMEOUT, async {
            node.initialize(BTreeSet::from([FIRST_VOTER])).await?;
            node.wait(Some(LOCAL_WRITE_TIMEOUT))
                .current_leader(FIRST_VOTER, "new space owner")
                .await?;
            let response = node
                .client_write(MembershipRequest::Initialize {
                    owner_account,
                    first_device: owner_contact.device_id(),
                    first_contact: Some(owner_contact),
                })
                .await?;
            if response.data != Outcome::Initialized {
                return Err("space permission history already has an owner".into());
            }
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
        })
        .await??;
        Ok(())
    }
    .await;
    let stopped = node.shutdown().await;
    initialized?;
    stopped?;
    Ok(())
}

type LogEntry = Entry<MembershipRaft>;
type RaftResult<T> = Result<T, StorageError<u64>>;
type Configuration = StoredMembership<u64, BasicNode>;
type SnapshotMetadata = SnapshotMeta<u64, BasicNode>;

#[derive(Default, Serialize, Deserialize)]
struct AppliedState {
    last_applied: Option<LogId<u64>>,
    voters: Configuration,
    membership: Option<Membership>,
}

#[derive(Serialize, Deserialize)]
struct StoredSnapshot {
    meta: SnapshotMetadata,
    contents: SnapshotContents,
}

#[derive(Serialize, Deserialize)]
struct SnapshotContents {
    space_id: String,
    state: AppliedState,
}

impl StoredSnapshot {
    // Preserve OpenRaft's structured errors at the storage boundary.
    #[allow(clippy::result_large_err)]
    fn into_snapshot(self) -> RaftResult<Snapshot<MembershipRaft>> {
        let bytes = serde_json::to_vec(&self.contents)
            .map_err(|error| StorageIOError::read_snapshot(Some(self.meta.signature()), &error))?;
        Ok(Snapshot {
            meta: self.meta,
            snapshot: Box::new(Cursor::new(bytes)),
        })
    }
}

#[derive(Debug, Error)]
enum AccessError {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Serialization(#[from] serde_json::Error),
    #[error("consensus database lock was poisoned")]
    Poisoned,
    #[error("consensus snapshot metadata does not match its contents")]
    InvalidSnapshot,
}

#[derive(Clone)]
pub struct MembershipLog {
    db: Arc<Mutex<Connection>>,
    space_id: Arc<str>,
}

impl MembershipLog {
    pub fn open(path: impl AsRef<Path>, space_id: impl Into<Arc<str>>) -> rusqlite::Result<Self> {
        let db = Connection::open(path)?;
        db.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = FULL;
             CREATE TABLE IF NOT EXISTS membership_raft_log (
                 space_id TEXT NOT NULL,
                 log_index BLOB NOT NULL,
                 entry BLOB NOT NULL,
                 PRIMARY KEY (space_id, log_index)
             );
             CREATE TABLE IF NOT EXISTS membership_raft_meta (
                 space_id TEXT NOT NULL,
                 key TEXT NOT NULL,
                 value BLOB NOT NULL,
                 PRIMARY KEY (space_id, key)
             );",
        )?;
        Ok(Self {
            db: Arc::new(Mutex::new(db)),
            space_id: space_id.into(),
        })
    }

    #[allow(clippy::result_large_err)]
    pub async fn membership(&self) -> RaftResult<Option<Membership>> {
        self.access(ErrorSubject::StateMachine, ErrorVerb::Read, |db, space| {
            Ok(read_state(db, space)?.membership)
        })
        .await
    }

    #[allow(clippy::result_large_err)]
    async fn access<T, F>(
        &self,
        subject: ErrorSubject<u64>,
        verb: ErrorVerb,
        run: F,
    ) -> RaftResult<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut Connection, &str) -> Result<T, AccessError> + Send + 'static,
    {
        let log = self.clone();
        tokio::task::spawn_blocking(move || {
            let mut db = log.db.lock().map_err(|_| AccessError::Poisoned)?;
            run(&mut db, &log.space_id)
        })
        .await
        .map_err(|error| StorageIOError::new(subject.clone(), verb, &error))?
        .map_err(|error| StorageIOError::new(subject, verb, &error).into())
    }
}

impl RaftLogReader<MembershipRaft> for MembershipLog {
    async fn try_get_log_entries<RB>(&mut self, range: RB) -> RaftResult<Vec<LogEntry>>
    where
        RB: RangeBounds<u64> + Clone + std::fmt::Debug + Send,
    {
        let start = match range.start_bound() {
            Bound::Included(index) => Some(*index),
            Bound::Excluded(index) => index.checked_add(1),
            Bound::Unbounded => Some(u64::MIN),
        };
        let end = match range.end_bound() {
            Bound::Included(index) => Some(*index),
            Bound::Excluded(index) => index.checked_sub(1),
            Bound::Unbounded => Some(u64::MAX),
        };
        let (Some(start), Some(end)) = (start, end) else {
            return Ok(Vec::new());
        };
        self.access(ErrorSubject::Logs, ErrorVerb::Read, move |db, space| {
            let mut statement = db.prepare_cached(
                "SELECT entry FROM membership_raft_log
                 WHERE space_id = ?1 AND log_index BETWEEN ?2 AND ?3 ORDER BY log_index",
            )?;
            let mut rows =
                statement.query(params![space, start.to_be_bytes(), end.to_be_bytes()])?;
            let mut entries = Vec::new();
            while let Some(row) = rows.next()? {
                let bytes: Vec<u8> = row.get(0)?;
                entries.push(serde_json::from_slice(&bytes)?);
            }
            Ok(entries)
        })
        .await
    }
}

impl RaftLogStorage<MembershipRaft> for MembershipLog {
    type LogReader = Self;

    async fn get_log_state(&mut self) -> RaftResult<LogState<MembershipRaft>> {
        self.access(ErrorSubject::Logs, ErrorVerb::Read, |db, space| {
            let txn = db.transaction()?;
            let last_purged_log_id = read_meta(&txn, space, "purged")?;
            let last_entry: Option<Vec<u8>> = txn
                .query_row(
                    "SELECT entry FROM membership_raft_log
                     WHERE space_id = ?1 ORDER BY log_index DESC LIMIT 1",
                    [space],
                    |row| row.get(0),
                )
                .optional()?;
            let last_log_id = last_entry
                .map(|bytes| serde_json::from_slice::<LogEntry>(&bytes))
                .transpose()?
                .map(|entry| entry.log_id)
                .or(last_purged_log_id);
            Ok(LogState {
                last_purged_log_id,
                last_log_id,
            })
        })
        .await
    }

    async fn get_log_reader(&mut self) -> Self::LogReader {
        self.clone()
    }

    async fn save_vote(&mut self, vote: &Vote<u64>) -> RaftResult<()> {
        let vote = *vote;
        self.access(ErrorSubject::Vote, ErrorVerb::Write, move |db, space| {
            write_meta(db, space, "vote", &vote)
        })
        .await
    }

    async fn read_vote(&mut self) -> RaftResult<Option<Vote<u64>>> {
        self.access(ErrorSubject::Vote, ErrorVerb::Read, |db, space| {
            read_meta(db, space, "vote")
        })
        .await
    }

    async fn save_committed(&mut self, log_id: Option<LogId<u64>>) -> RaftResult<()> {
        self.access(ErrorSubject::Logs, ErrorVerb::Write, move |db, space| {
            write_meta(db, space, "committed", &log_id)
        })
        .await
    }

    async fn read_committed(&mut self) -> RaftResult<Option<LogId<u64>>> {
        self.access(ErrorSubject::Logs, ErrorVerb::Read, |db, space| {
            Ok(read_meta::<Option<_>>(db, space, "committed")?.flatten())
        })
        .await
    }

    async fn append<I>(
        &mut self,
        entries: I,
        callback: LogFlushed<MembershipRaft>,
    ) -> RaftResult<()>
    where
        I: IntoIterator<Item = LogEntry> + Send,
        I::IntoIter: Send,
    {
        let entries: Vec<_> = entries.into_iter().collect();
        self.access(ErrorSubject::Logs, ErrorVerb::Write, move |db, space| {
            let txn = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
            {
                let mut statement = txn.prepare_cached(
                    "INSERT INTO membership_raft_log (space_id, log_index, entry) VALUES (?1, ?2, ?3)
                     ON CONFLICT (space_id, log_index) DO UPDATE SET entry = excluded.entry",
                )?;
                for entry in entries {
                    statement.execute(params![
                        space,
                        entry.log_id.index.to_be_bytes(),
                        serde_json::to_vec(&entry)?,
                    ])?;
                }
            }
            txn.commit()?;
            Ok(())
        })
        .await?;
        callback.log_io_completed(Ok(()));
        Ok(())
    }

    async fn truncate(&mut self, log_id: LogId<u64>) -> RaftResult<()> {
        self.access(ErrorSubject::Logs, ErrorVerb::Delete, move |db, space| {
            db.execute(
                "DELETE FROM membership_raft_log WHERE space_id = ?1 AND log_index >= ?2",
                params![space, log_id.index.to_be_bytes()],
            )?;
            Ok(())
        })
        .await
    }

    async fn purge(&mut self, log_id: LogId<u64>) -> RaftResult<()> {
        self.access(ErrorSubject::Logs, ErrorVerb::Delete, move |db, space| {
            let txn = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
            txn.execute(
                "DELETE FROM membership_raft_log WHERE space_id = ?1 AND log_index <= ?2",
                params![space, log_id.index.to_be_bytes()],
            )?;
            write_meta(&txn, space, "purged", &log_id)?;
            txn.commit()?;
            Ok(())
        })
        .await
    }
}

impl RaftStateMachine<MembershipRaft> for MembershipLog {
    type SnapshotBuilder = Self;

    async fn applied_state(&mut self) -> RaftResult<(Option<LogId<u64>>, Configuration)> {
        self.access(ErrorSubject::StateMachine, ErrorVerb::Read, |db, space| {
            let state = read_state(db, space)?;
            Ok((state.last_applied, state.voters))
        })
        .await
    }

    async fn apply<I>(&mut self, entries: I) -> RaftResult<Vec<Outcome>>
    where
        I: IntoIterator<Item = LogEntry> + Send,
        I::IntoIter: Send,
    {
        let entries: Vec<_> = entries.into_iter().collect();
        self.access(
            ErrorSubject::StateMachine,
            ErrorVerb::Write,
            move |db, space| {
                let txn = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
                let mut state = read_state(&txn, space)?;
                let mut outcomes = Vec::with_capacity(entries.len());
                for entry in entries {
                    if state
                        .last_applied
                        .is_some_and(|last| last.index >= entry.log_id.index)
                    {
                        outcomes.push(Outcome::AlreadyApplied);
                        continue;
                    }
                    state.last_applied = Some(entry.log_id);
                    let index = entry.log_id.index;
                    outcomes.push(match entry.payload {
                        EntryPayload::Blank => Outcome::NoChange,
                        EntryPayload::Membership(voters) => {
                            state.voters = StoredMembership::new(Some(entry.log_id), voters);
                            Outcome::NoChange
                        }
                        EntryPayload::Normal(MembershipRequest::Initialize {
                            owner_account,
                            first_device,
                            first_contact,
                        }) => {
                            if state.membership.is_some() {
                                Outcome::AlreadyInitialized
                            } else {
                                let mut membership =
                                    Membership::new(owner_account, first_device, index);
                                if let Some(contact) = first_contact {
                                    membership.record_contact(first_device, contact);
                                }
                                state.membership = Some(membership);
                                Outcome::Initialized
                            }
                        }
                        EntryPayload::Normal(MembershipRequest::Change { author, change }) => state
                            .membership
                            .as_mut()
                            .map_or(Outcome::Uninitialized, |membership| {
                                membership.apply(index, author, change)
                            }),
                        EntryPayload::Normal(MembershipRequest::RevokeDevice {
                            author,
                            enrollment,
                        }) => {
                            state
                                .membership
                                .as_mut()
                                .map_or(Outcome::Uninitialized, |membership| {
                                    if replica::owner(membership, author.device) != Some(author) {
                                        Outcome::AuthorNotOwner
                                    } else if membership.deletion_index().is_some() {
                                        Outcome::SpaceAlreadyDeleted
                                    } else if would_lose_quorum(
                                        membership,
                                        &state.voters,
                                        enrollment,
                                    ) {
                                        Outcome::WouldLoseQuorum
                                    } else {
                                        membership.apply(
                                            index,
                                            author,
                                            Change::Remove { enrollment },
                                        )
                                    }
                                })
                        }
                    });
                }
                write_meta(&txn, space, "state", &state)?;
                txn.commit()?;
                Ok(outcomes)
            },
        )
        .await
    }

    async fn get_snapshot_builder(&mut self) -> Self::SnapshotBuilder {
        self.clone()
    }

    async fn begin_receiving_snapshot(&mut self) -> RaftResult<Box<Cursor<Vec<u8>>>> {
        Ok(Box::new(Cursor::new(Vec::new())))
    }

    async fn install_snapshot(
        &mut self,
        meta: &SnapshotMetadata,
        snapshot: Box<Cursor<Vec<u8>>>,
    ) -> RaftResult<()> {
        let meta = meta.clone();
        let signature = meta.signature();
        self.access(
            ErrorSubject::Snapshot(Some(signature)),
            ErrorVerb::Write,
            move |db, space| {
                let contents: SnapshotContents = serde_json::from_slice(snapshot.get_ref())?;
                let state = &contents.state;
                if contents.space_id != space
                    || state.last_applied != meta.last_log_id
                    || state.voters != meta.last_membership
                {
                    return Err(AccessError::InvalidSnapshot);
                }
                let txn = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
                if read_state(&txn, space)?.last_applied.map(|log| log.index)
                    > state.last_applied.map(|log| log.index)
                {
                    return Err(AccessError::InvalidSnapshot);
                }
                write_meta(&txn, space, "state", state)?;
                write_meta(&txn, space, "snapshot", &StoredSnapshot { meta, contents })?;
                txn.commit()?;
                Ok(())
            },
        )
        .await
    }

    async fn get_current_snapshot(&mut self) -> RaftResult<Option<Snapshot<MembershipRaft>>> {
        let snapshot: Option<StoredSnapshot> = self
            .access(
                ErrorSubject::Snapshot(None),
                ErrorVerb::Read,
                |db, space| read_meta(db, space, "snapshot"),
            )
            .await?;
        snapshot.map(StoredSnapshot::into_snapshot).transpose()
    }
}

impl RaftSnapshotBuilder<MembershipRaft> for MembershipLog {
    async fn build_snapshot(&mut self) -> RaftResult<Snapshot<MembershipRaft>> {
        let snapshot = self
            .access(
                ErrorSubject::Snapshot(None),
                ErrorVerb::Write,
                |db, space| {
                    let txn = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
                    let state = read_state(&txn, space)?;
                    let meta = SnapshotMeta {
                        last_log_id: state.last_applied,
                        last_membership: state.voters.clone(),
                        snapshot_id: state
                            .last_applied
                            .map_or_else(|| "empty".into(), |log| log.to_string()),
                    };
                    let snapshot = StoredSnapshot {
                        meta,
                        contents: SnapshotContents {
                            space_id: space.into(),
                            state,
                        },
                    };
                    write_meta(&txn, space, "snapshot", &snapshot)?;
                    txn.commit()?;
                    Ok(snapshot)
                },
            )
            .await?;
        snapshot.into_snapshot()
    }
}

fn read_state(db: &Connection, space: &str) -> Result<AppliedState, AccessError> {
    Ok(read_meta(db, space, "state")?.unwrap_or_default())
}

fn write_meta<T: Serialize>(
    db: &Connection,
    space: &str,
    key: &str,
    value: &T,
) -> Result<(), AccessError> {
    db.execute(
        "INSERT INTO membership_raft_meta (space_id, key, value) VALUES (?1, ?2, ?3)
         ON CONFLICT (space_id, key) DO UPDATE SET value = excluded.value",
        params![space, key, serde_json::to_vec(value)?],
    )?;
    Ok(())
}

fn read_meta<T: DeserializeOwned>(
    db: &Connection,
    space: &str,
    key: &str,
) -> Result<Option<T>, AccessError> {
    let value: Option<Vec<u8>> = db
        .query_row(
            "SELECT value FROM membership_raft_meta WHERE space_id = ?1 AND key = ?2",
            params![space, key],
            |row| row.get(0),
        )
        .optional()?;
    Ok(value
        .map(|bytes| serde_json::from_slice(&bytes))
        .transpose()?)
}

fn would_lose_quorum(
    membership: &Membership,
    configuration: &Configuration,
    target: Enrollment,
) -> bool {
    if replica::owner(membership, target.device) != Some(target)
        || !replica::voter_id(membership, target)
            .is_some_and(|id| configuration.voter_ids().any(|voter| voter == id))
    {
        return false;
    }
    // A revoked voter can no longer authenticate, so count only remaining active enrollments.
    let remaining: BTreeSet<_> = membership
        .enrolled_owners()
        .filter(|owner| *owner != target)
        .filter_map(|owner| replica::voter_id(membership, owner))
        .collect();
    configuration
        .membership()
        .get_joint_config()
        .iter()
        .any(|voters| voters.intersection(&remaining).count() <= voters.len() / 2)
}

#[cfg(test)]
mod tests {
    use openraft::{
        CommittedLeaderId,
        testing::{StoreBuilder, Suite},
    };
    use tempfile::{TempDir, tempdir};

    use super::*;
    use crate::membership::Role;

    struct Disk;

    impl StoreBuilder<MembershipRaft, MembershipLog, MembershipLog, TempDir> for Disk {
        async fn build(&self) -> RaftResult<(TempDir, MembershipLog, MembershipLog)> {
            let dir = tempdir().unwrap();
            let store = MembershipLog::open(dir.path().join("sync.sqlite"), "space").unwrap();
            Ok((dir, store.clone(), store))
        }
    }

    #[test]
    fn satisfies_openraft_storage_contract() {
        Suite::test_all(Disk).unwrap();
    }

    fn request(index: u64, payload: MembershipRequest) -> LogEntry {
        Entry {
            log_id: LogId::new(CommittedLeaderId::new(1, 1), index),
            payload: EntryPayload::Normal(payload),
        }
    }

    #[tokio::test]
    async fn removal_survives_reopen_and_transfer_to_another_replica() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("sync.sqlite");
        let mut source = MembershipLog::open(&path, "household").unwrap();
        let first_device = [1; 32];
        let other_device = [2; 32];
        let owner = Enrollment {
            device: first_device,
            generation: 1,
        };
        let enrolled = Enrollment {
            device: other_device,
            generation: 2,
        };
        let applied = source
            .apply([
                request(
                    1,
                    MembershipRequest::Initialize {
                        owner_account: "shared account".into(),
                        first_device,
                        first_contact: None,
                    },
                ),
                request(
                    2,
                    MembershipRequest::Change {
                        author: owner,
                        change: Change::Enroll {
                            device: other_device,
                            role: Role::Member,
                        },
                    },
                ),
                request(
                    3,
                    MembershipRequest::Change {
                        author: owner,
                        change: Change::Remove {
                            enrollment: enrolled,
                        },
                    },
                ),
            ])
            .await
            .unwrap();
        assert_eq!(
            applied,
            [
                Outcome::Initialized,
                Outcome::Enrolled(enrolled),
                Outcome::Removed(enrolled)
            ]
        );
        drop(source);

        let mut source = MembershipLog::open(&path, "household").unwrap();
        let mut unrelated = MembershipLog::open(&path, "other space").unwrap();
        assert!(unrelated.membership().await.unwrap().is_none());
        let snapshot = source.build_snapshot().await.unwrap();
        assert!(
            unrelated
                .install_snapshot(&snapshot.meta, snapshot.snapshot.clone())
                .await
                .is_err()
        );
        let mut replica =
            MembershipLog::open(dir.path().join("replica.sqlite"), "household").unwrap();
        replica
            .install_snapshot(&snapshot.meta, snapshot.snapshot)
            .await
            .unwrap();
        let membership = replica.membership().await.unwrap().unwrap();
        assert_eq!(membership.owner_account(), "shared account");
        assert_eq!(membership.removal_index(enrolled), Some(3));
        assert!(membership.device(other_device).is_none());
    }
}
