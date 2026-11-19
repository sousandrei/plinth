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

#[cfg(test)]
mod tests;

pub type Fields = BTreeMap<String, Value>;
pub type Snapshot = BTreeMap<String, Fields>;

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
             CREATE INDEX IF NOT EXISTS documents_by_space ON documents(space_id);",
        )?;

        Ok(Self {
            db,
            actor: AutoCommit::new().get_actor().clone(),
        })
    }

    /// Save a single edit containing the fields that the user changed.
    pub fn patch(
        &mut self,
        space_id: &str,
        record_id: &str,
        timestamp_ms: i64,
        fields: &Fields,
    ) -> Result<(), Error> {
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut doc = load(&tx, space_id)?.with_actor(self.actor.clone());
        let before = doc.get_heads();

        // Flat keys let peers independently create fields for the same record.
        for (name, value) in fields {
            let key = serde_json::to_string(&(record_id, name))?;
            let value = serde_json::to_vec(&(timestamp_ms, value))?;
            doc.put(ROOT, key, ScalarValue::Bytes(value))?;
        }

        append_delta(&tx, space_id, &mut doc, &before)?;
        tx.commit()?;
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
        let message = Message::decode(bytes).map_err(|err| Error::Message(err.to_string()))?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut doc = load(&tx, &session.space_id)?;
        let before = doc.get_heads();
        let mut state = session.state.clone();
        doc.sync().receive_sync_message(&mut state, message)?;

        if !doc.get_missing_deps(&[]).is_empty() {
            return Err(Error::MissingDependencies);
        }
        if doc.get_heads() != before {
            project(&doc)?;
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
