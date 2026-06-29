use serde::{Deserialize, Serialize};

use crate::sync::payloads::TablePayload;

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
    pub device_id: String,
    /// Highest `seq` already applied locally for changes originating
    /// from the specified device in this space.
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
/// table so callers can map straight through. `payload` is the typed
/// snapshot decoded from the trigger's JSON at the read boundary
/// (`None` for deletes — only the row identity is needed to apply).
/// Postcard encodes the variant tag compactly, so the wire form is
/// strictly smaller than shipping JSON-in-postcard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeRow {
    pub id: String,
    pub space_id: String,
    pub table_name: String,
    pub row_id: String,
    pub operation: String,
    pub payload: Option<TablePayload>,
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
    pub device_id: String,
    pub rows: Vec<ChangeRow>,
    pub final_seq: i64,
}

/// Closing frame; lets the session end cleanly without relying on
/// EOF semantics. Optional — both sides treat a clean shutdown as
/// equivalent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bye {}

/// Heartbeat request sent by the dialer instead of `Hello`/`Cursors` when
/// the only goal is presence detection. The peer replies with `Pong` and
/// closes; no cursor exchange, no batch shipping, no model exchange.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ping {}

/// Heartbeat reply. The recipient does no work — `PeerRegistry::touch`
/// fires when the connection closes cleanly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pong {}

/// One entry in a `ModelVersionSummary` — the sender's active trained
/// model version for one shared space. Version 0 means no finetuned
/// model exists (only the shipped base model). The two MD5 fields let
/// peers detect divergence: same version but different content
/// (corruption, partial transfer, or two devices training the same
/// version number independently). Empty string means "unknown" — the
/// sender chose not to include the hash for this entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelVersionEntry {
    pub space_id: String,
    pub version: u32,
    pub weights_md5: String,
    pub card_md5: String,
}

/// Sent by each side after all `Batch` frames are exhausted. Lets both
/// peers compare their active model versions and decide who needs to
/// receive a `ModelData` transfer. One entry per shared space.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelVersionSummary {
    pub entries: Vec<ModelVersionEntry>,
}

/// Carries the binary weights and JSON card for one model version.
/// Transferred by the peer with the higher version immediately after
/// both `ModelVersionSummary` frames have been exchanged. The receiver
/// writes the files atomically (temp-file + rename), verifies the
/// MD5s in this frame against the received bytes, and updates its
/// `space_settings` active model version. See data/PLAN.md §8.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelData {
    pub space_id: String,
    pub version: u32,
    /// Raw bytes of `model_v{version}.safetensors`.
    pub weights: Vec<u8>,
    /// Raw bytes of `model_v{version}.json` (the `ModelCard`).
    pub card: Vec<u8>,
    /// MD5 of the `weights` bytes in this frame (hex).
    pub weights_md5: String,
    /// MD5 of the `card` bytes in this frame (hex).
    pub card_md5: String,
}

/// Tagged envelope so a single byte stream can carry any frame type.
/// Postcard encodes the variant tag compactly; the wire size overhead
/// is one byte per frame.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Frame {
    Hello(Hello),
    Cursors(Cursors),
    Batch(ChangeBatch),
    /// Full snapshot for a single space, streamed in chunks. Sent by the
    /// host when the peer's cursor is behind `change_log.min_seq` and
    /// incremental replay can no longer catch the joiner up. The host
    /// sends `Snapshot` frames until it emits `SnapshotEnd`; the joiner
    /// reads in a loop, applies each frame under `apply_guard`, and
    /// then resumes the normal sync session from `final_seq`.
    Snapshot(SnapshotChunk),
    SnapshotEnd,
    ModelVersionSummary(ModelVersionSummary),
    ModelData(ModelData),
    Bye(Bye),
    /// Heartbeat sent as the first frame (instead of `Hello`) when the
    /// dialer only wants to confirm the peer is alive. The recipient
    /// responds with `Pong` and closes the session.
    Ping(Ping),
    Pong(Pong),
}

/// One chunk of a space snapshot. The host breaks each table's rows
/// into 500-item slices (same `CHUNK_SIZE` as the pairing transfer) so
/// every frame stays well under the 1 MiB ceiling. The joiner applies
/// each frame inside an `apply_guard` transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotChunk {
    pub space_id: String,
    pub frame: crate::sync::snapshot::SnapshotFrame,
}
