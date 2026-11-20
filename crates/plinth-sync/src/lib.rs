//! Standalone replication prototype for trusted peers.
//!
//! Create one session per space on each connection. Exchange messages over a
//! reliable, ordered stream and start fresh sessions after reconnecting. The
//! application supplies authorized space IDs and timestamps in Unix milliseconds.

use std::{collections::BTreeMap, path::Path};

use automerge::{
    ActorId, AutoCommit, ChangeHash, ROOT, ReadDoc, ScalarValue, Value as DocValue,
    sync::{Message, State, SyncDoc},
    transaction::Transactable,
};
use rusqlite::{Connection, TransactionBehavior, params};
use serde_json::Value;
use thiserror::Error;

pub mod consensus;
pub mod discovery;
pub mod membership;
pub mod transport;

#[cfg(test)]
mod tests;

pub type Fields = BTreeMap<String, Value>;
pub type Snapshot = BTreeMap<String, Fields>;

#[derive(Debug)]
pub struct PendingEdit {
    pub sequence: i64,
    pub space_id: String,
    pub record_id: String,
    pub fields: Fields,
}

#[derive(Debug, Clone)]
pub struct PairedDevice {
    pub space_id: String,
    pub name: String,
    pub certificate: Vec<u8>,
    pub address: Option<String>,
    pub display_name: Option<String>,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("SQLite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Automerge: {0}")]
    Automerge(#[from] automerge::AutomergeError),
    #[error("record encoding: {0}")]
    Encoding(#[from] serde_json::Error),
    #[error("invalid sync message: {0}")]
    Message(String),
    #[error("invalid field in sync document")]
    InvalidField,
    #[error("sync document has missing dependencies")]
    MissingDependencies,
}

pub struct Session {
    space_id: String,
    state: State,
}

impl Session {
    pub fn new(space_id: impl Into<String>) -> Self {
        Self {
            space_id: space_id.into(),
            state: State::new(),
        }
    }
}

pub struct Store {
    db: Connection,
    actor: ActorId,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let db = Connection::open(path)?;
        db.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = FULL;
             CREATE TABLE IF NOT EXISTS documents (
                 sequence INTEGER PRIMARY KEY,
                 space_id TEXT NOT NULL,
                 bytes BLOB NOT NULL
             );
             CREATE INDEX IF NOT EXISTS documents_by_space ON documents(space_id);
             CREATE TABLE IF NOT EXISTS pending_edits (
                 sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                 space_id TEXT NOT NULL,
                 record_id TEXT NOT NULL,
                 fields BLOB NOT NULL
             );
             CREATE TABLE IF NOT EXISTS paired_devices (
                 space_id TEXT NOT NULL,
                 certificate BLOB NOT NULL,
                 name TEXT NOT NULL,
                 address TEXT,
                 PRIMARY KEY (space_id, certificate)
             );",
        )?;

        let has_display_name: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('paired_devices') WHERE name = 'display_name')",
            [],
            |row| row.get(0),
        )?;
        if !has_display_name {
            db.execute(
                "ALTER TABLE paired_devices ADD COLUMN display_name TEXT",
                [],
            )?;
        }
        let has_disconnected: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('paired_devices') WHERE name = 'disconnected')",
            [],
            |row| row.get(0),
        )?;
        if !has_disconnected {
            db.execute(
                "ALTER TABLE paired_devices ADD COLUMN disconnected INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        Ok(Self {
            db,
            actor: AutoCommit::new().get_actor().clone(),
        })
    }

    pub fn pair_device(&self, peer: &PairedDevice) -> Result<(), Error> {
        self.db.execute(
            "INSERT INTO paired_devices (space_id, certificate, name, address, display_name)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (space_id, certificate) DO UPDATE
             SET name = excluded.name,
                 address = COALESCE(excluded.address, paired_devices.address),
                 display_name = COALESCE(excluded.display_name, paired_devices.display_name),
                 disconnected = 0",
            params![
                peer.space_id,
                peer.certificate,
                peer.name,
                peer.address,
                peer.display_name
            ],
        )?;
        Ok(())
    }

    pub fn unpair_device(&self, space_id: &str, certificate: &[u8]) -> Result<(), Error> {
        self.db.execute(
            "UPDATE paired_devices SET disconnected = 1, address = NULL, display_name = NULL
             WHERE space_id = ?1 AND certificate = ?2",
            params![space_id, certificate],
        )?;
        Ok(())
    }

    pub fn cache_roster_peers(&mut self, peers: &[PairedDevice]) -> Result<(), Error> {
        let txn = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        {
            let mut insert = txn.prepare_cached(
                "INSERT INTO paired_devices (space_id, certificate, name, address)
                 VALUES (?1, ?2, ?3, ?4) ON CONFLICT (space_id, certificate) DO NOTHING",
            )?;
            for peer in peers {
                insert.execute(params![
                    peer.space_id,
                    peer.certificate,
                    peer.name,
                    peer.address
                ])?;
            }
        }
        txn.commit()?;
        Ok(())
    }

    pub fn paired_devices(&self) -> Result<Vec<PairedDevice>, Error> {
        let mut query = self.db.prepare(
            "SELECT space_id, certificate, name, address, display_name FROM paired_devices
             WHERE disconnected = 0 ORDER BY space_id, name",
        )?;
        let rows = query.query_map([], |row| {
            Ok(PairedDevice {
                space_id: row.get(0)?,
                certificate: row.get(1)?,
                name: row.get(2)?,
                address: row.get(3)?,
                display_name: row.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Save a single edit containing the fields that the user changed.
    pub fn patch(
        &mut self,
        space_id: &str,
        record_id: &str,
        timestamp_ms: i64,
        fields: &Fields,
    ) -> Result<(), Error> {
        self.write_patches(space_id, timestamp_ms, [(record_id, fields)], false)
    }

    /// Commit the edit and its local projection entry together.
    pub fn stage_edit(
        &mut self,
        space_id: &str,
        record_id: &str,
        timestamp_ms: i64,
        fields: &Fields,
    ) -> Result<(), Error> {
        self.stage_edits(space_id, timestamp_ms, [(record_id, fields)])
    }

    pub fn stage_edits<'a>(
        &mut self,
        space_id: &str,
        timestamp_ms: i64,
        edits: impl IntoIterator<Item = (&'a str, &'a Fields)>,
    ) -> Result<(), Error> {
        self.write_patches(space_id, timestamp_ms, edits, true)
    }

    fn write_patches<'a>(
        &mut self,
        space_id: &str,
        timestamp_ms: i64,
        edits: impl IntoIterator<Item = (&'a str, &'a Fields)>,
        stage_projection: bool,
    ) -> Result<(), Error> {
        let mut edits = edits
            .into_iter()
            .filter(|(_, fields)| !fields.is_empty())
            .peekable();
        if edits.peek().is_none() {
            return Ok(());
        }
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut doc = load(&tx, space_id)?.with_actor(self.actor.clone());
        let before = doc.get_heads();

        for (record_id, fields) in edits {
            // Flat keys let peers independently create fields for the same record.
            for (name, value) in fields {
                let key = serde_json::to_string(&(record_id, name))?;
                let value = serde_json::to_vec(&(timestamp_ms, value))?;
                doc.put(ROOT, key, ScalarValue::Bytes(value))?;
            }
            if stage_projection {
                enqueue(&tx, space_id, record_id, fields)?;
            }
        }

        append_delta(&tx, space_id, &mut doc, &before)?;
        tx.commit()?;
        Ok(())
    }

    /// A single projection worker reads queued edits in order and retries
    /// the same edit after a restart until it has been completed.
    pub fn next_pending_edit(&self) -> Result<Option<PendingEdit>, Error> {
        let mut stmt = self.db.prepare(
            "SELECT sequence, space_id, record_id, fields FROM pending_edits
             ORDER BY sequence LIMIT 1",
        )?;
        let mut rows = stmt.query([])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        let fields: Vec<u8> = row.get(3)?;
        Ok(Some(PendingEdit {
            sequence: row.get(0)?,
            space_id: row.get(1)?,
            record_id: row.get(2)?,
            fields: serde_json::from_slice(&fields)?,
        }))
    }

    /// Remove an entry once the application has committed or skipped it.
    pub fn complete_edit(&mut self, sequence: i64) -> Result<(), Error> {
        self.db
            .execute("DELETE FROM pending_edits WHERE sequence = ?1", [sequence])?;
        Ok(())
    }

    /// Read the current records so a caller can rebuild its local view.
    pub fn snapshot(&self, space_id: &str) -> Result<Snapshot, Error> {
        project(&load(&self.db, space_id)?)
    }

    pub fn next_message(&self, session: &mut Session) -> Result<Option<Vec<u8>>, Error> {
        let mut doc = load(&self.db, &session.space_id)?;
        Ok(doc
            .sync()
            .generate_sync_message(&mut session.state)
            .map(Message::encode))
    }

    /// Persist incoming edits before the session can acknowledge them.
    pub fn receive(&mut self, session: &mut Session, bytes: &[u8]) -> Result<(), Error> {
        self.receive_message(session, bytes, None)
    }

    /// Commit incoming history and queue its changed resolved fields together.
    pub fn receive_projected(
        &mut self,
        session: &mut Session,
        bytes: &[u8],
        validate: fn(&Fields) -> bool,
    ) -> Result<(), Error> {
        self.receive_message(session, bytes, Some(validate))
    }

    fn receive_message(
        &mut self,
        session: &mut Session,
        bytes: &[u8],
        validate: Option<fn(&Fields) -> bool>,
    ) -> Result<(), Error> {
        let message = Message::decode(bytes).map_err(|err| Error::Message(err.to_string()))?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut doc = load(&tx, &session.space_id)?;
        let before = doc.get_heads();
        let previous = validate.map(|_| project(&doc)).transpose()?;
        let mut state = session.state.clone();
        doc.sync().receive_sync_message(&mut state, message)?;

        if !doc.get_missing_deps(&[]).is_empty() {
            return Err(Error::MissingDependencies);
        }
        if doc.get_heads() != before {
            let resolved = project(&doc)?;
            if let Some((validate, previous)) = validate.zip(previous) {
                for (record_id, mut fields) in resolved {
                    if let Some(old) = previous.get(&record_id) {
                        fields.retain(|name, value| old.get(name) != Some(value));
                    }
                    if fields.is_empty() {
                        continue;
                    }
                    if !validate(&fields) {
                        return Err(Error::InvalidField);
                    }
                    enqueue(&tx, &session.space_id, &record_id, &fields)?;
                }
            }
            append_delta(&tx, &session.space_id, &mut doc, &before)?;
        }

        tx.commit()?;
        session.state = state;
        Ok(())
    }

    /// Replace the stored chunks with one full Automerge document.
    pub fn checkpoint(&mut self, space_id: &str) -> Result<(), Error> {
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut doc = load(&tx, space_id)?;
        tx.execute("DELETE FROM documents WHERE space_id = ?1", [space_id])?;
        if !doc.get_heads().is_empty() {
            tx.execute(
                "INSERT INTO documents (space_id, bytes) VALUES (?1, ?2)",
                params![space_id, doc.save()],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Consolidate fragmented documents without changing their sync history.
    pub fn checkpoint_all(&mut self) -> Result<(), Error> {
        let spaces = {
            let mut statement = self
                .db
                .prepare("SELECT space_id FROM documents GROUP BY space_id HAVING count(*) > 1")?;
            statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        for space in spaces {
            self.checkpoint(&space)?;
        }
        Ok(())
    }
}

fn enqueue(db: &Connection, space_id: &str, record_id: &str, fields: &Fields) -> Result<(), Error> {
    db.execute(
        "INSERT INTO pending_edits (space_id, record_id, fields) VALUES (?1, ?2, ?3)",
        params![space_id, record_id, serde_json::to_vec(fields)?],
    )?;
    Ok(())
}

fn load(db: &Connection, space_id: &str) -> Result<AutoCommit, Error> {
    let mut stmt =
        db.prepare("SELECT bytes FROM documents WHERE space_id = ?1 ORDER BY sequence")?;
    let mut rows = stmt.query([space_id])?;
    let mut doc = AutoCommit::new();
    while let Some(row) = rows.next()? {
        let bytes: Vec<u8> = row.get(0)?;
        doc.load_incremental(&bytes)?;
    }
    if !doc.get_missing_deps(&[]).is_empty() {
        return Err(Error::MissingDependencies);
    }
    Ok(doc)
}

fn append_delta(
    db: &Connection,
    space_id: &str,
    doc: &mut AutoCommit,
    before: &[ChangeHash],
) -> Result<(), Error> {
    let bytes = doc.save_after(before);
    if !bytes.is_empty() {
        db.execute(
            "INSERT INTO documents (space_id, bytes) VALUES (?1, ?2)",
            params![space_id, bytes],
        )?;
    }
    Ok(())
}

fn project(doc: &AutoCommit) -> Result<Snapshot, Error> {
    let mut records = Snapshot::new();
    for key in doc.keys(ROOT) {
        let (record_id, name): (String, String) = serde_json::from_str(&key)?;
        let mut selected = None;
        // Automerge hides causally superseded edits. Order the remaining
        // concurrent edits by timestamp and a peer-independent change hash.
        for (value, op) in doc.get_all(ROOT, key)? {
            let DocValue::Scalar(scalar) = value else {
                return Err(Error::InvalidField);
            };
            let ScalarValue::Bytes(bytes) = scalar.as_ref() else {
                return Err(Error::InvalidField);
            };
            let (timestamp, value): (i64, Value) = serde_json::from_slice(bytes)?;
            let hash = doc.hash_for_opid(&op).ok_or(Error::InvalidField)?;
            let order = (timestamp, hash);
            if selected.as_ref().is_none_or(|(best, _)| order > *best) {
                selected = Some((order, value));
            }
        }
        if let Some((_, value)) = selected {
            records.entry(record_id).or_default().insert(name, value);
        }
    }
    Ok(records)
}
