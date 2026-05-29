use serde::{Deserialize, Serialize};

/// Protocol version, sent in `Hello`. Bumped on any wire-incompatible
/// change. A mismatch terminates the session immediately.
pub const PROTOCOL_VERSION: u16 = 1;

/// First frame sent in both directions after the mTLS handshake. Lets
/// peers negotiate compatibility and exchange their stable device IDs
/// (the cert CN is authoritative, but echoing the ID makes logs
/// readable and catches accidental cert-swap bugs early).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hello {
    pub protocol_version: u16,
    pub device_id: String,
}

/// One peer's view of how far it has consumed another peer's change log
/// for one shared space. Sent as a batch inside `Cursors`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorEntry {
    pub space_id: String,
    /// Highest `seq` already applied locally for changes originating
    /// from the remote peer in this space.
    pub last_seq: i64,
}

/// Sent by each side after `Hello` — a snapshot of cursors covering
/// every space shared with this peer. The recipient uses each entry
/// to compute which `change_log` rows of its own to ship back.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cursors {
    pub entries: Vec<CursorEntry>,
}

/// A single change_log row in transit. Field names mirror the SQLite
/// table so callers can map straight through. `payload` is the JSON
/// snapshot written by the trigger (None for deletes).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeRow {
    pub id: String,
    pub space_id: String,
    pub table_name: String,
    pub row_id: String,
    pub operation: String,
    pub payload: Option<String>,
    pub seq: i64,
    pub device_id: String,
    pub changed_at: String,
}

/// A batch of changes for one space. The sender streams as many of these
/// as it needs to cover everything past the recipient's cursor; the
/// recipient applies them in `seq` order and records the new high-water
/// mark via `sync_cursors`. `final_seq` is the sender's current max
/// `seq` for this space — used by the receiver as its new cursor even
/// if `rows` is empty.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeBatch {
    pub space_id: String,
    pub rows: Vec<ChangeRow>,
    pub final_seq: i64,
}

/// Closing frame; lets the session end cleanly without relying on
/// EOF semantics. Optional — both sides treat a clean shutdown as
/// equivalent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bye {}

/// Tagged envelope so a single byte stream can carry any frame type.
/// Postcard encodes the variant tag compactly; the wire size overhead
/// is one byte per frame.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Frame {
    Hello(Hello),
    Cursors(Cursors),
    Batch(ChangeBatch),
    Bye(Bye),
}
