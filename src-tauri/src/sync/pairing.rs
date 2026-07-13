//! Pairing — initial trust establishment between two devices.
//!
//! Flow (TCP over loopback or LAN, port advertised in the 6-digit token):
//!
//! 1. **Host** (the existing space owner) generates a 6-digit token and
//!    starts a single-shot listener on an ephemeral port. The token
//!    encodes both the digits AND the host's `IP:port`, so the joiner
//!    can locate the host without mDNS round-trip latency.
//! 2. **Joiner** types the token, connects, runs symmetric SPAKE2 with
//!    the 6-digit secret to derive a shared 32-byte key.
//! 3. All subsequent messages over the same socket are framed
//!    `[u32 len][12-byte nonce][chacha20poly1305 ciphertext]` and
//!    deserialized via bincode.
//! 4. Joiner sends `JoinPayload` (joining user + this device's identity).
//! 5. Host validates, persists the joiner as a trusted device, sends
//!    back `SpaceBundle` (full space + member roster + host identity).
//! 6. Both sides write `trusted_devices` rows.
//! 7. Connection closes. Future syncs use mTLS (Step 5).
//!
//! The SPAKE2 derived key is intentionally NOT reused for the long-term
//! sync channel — that uses the exchanged X.509 certs over mTLS.

use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use chacha20poly1305::{
    ChaCha20Poly1305, Nonce,
    aead::{Aead, KeyInit},
};
use getrandom::fill;
use serde::{Deserialize, Serialize};
use spake2::{Ed25519Group, Identity as PakeIdentity, Password, Spake2};
use sqlx::SqlitePool;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

use crate::error::AppError;

const TOKEN_TTL_SECS: u64 = 90;
const HANDSHAKE_DEADLINE_SECS: u64 = 300;
const PAKE_IDENTITY: &[u8] = b"plinth-pairing-v1";

/// Fixed TCP port the host always listens on for pairing connections.
/// Advertised in the mDNS TXT record so joiners can find it without
/// encoding the port into the human-typed 6-digit token.
pub const PAIRING_PORT: u16 = 52737;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A pending pairing token shown to the user on the host device.
#[derive(Debug, Clone, Serialize)]
pub struct PairToken {
    /// 6-digit decimal string the user types on the joiner.
    pub token: String,
    /// Encoded form `digits|host:port` that the UI may display as a QR
    /// for one-tap joining.
    pub address: String,
    /// Wall-clock expiry as unix seconds.
    pub expires_at_unix: u64,
}

// Wire types live in `snapshot` so they can be reused by the sync
// session fallback. Re-exported here for backwards-compat callers.
pub use crate::sync::snapshot::{
    SnapshotFrame, SpaceSnapshot, WireAccount, WireAccountSummary, WireCategory, WireMember,
    WireSpace, WireSpaceSetting, WireTransaction, WireUser,
};

/// Sent by the joiner. Tells the host who is joining and how to reach
/// this device later.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinPayload {
    /// The joining user, if they are new to this space. `None` when the
    /// joiner is an existing member (e.g. same person on a second device)
    /// and just needs the device trusted — no new membership row is created.
    pub user: Option<WireUser>,
    pub device_id: String,
    pub device_name: String,
    pub cert_pem: String,
}

/// Sent by the host. Carries the full space context so the joiner can
/// materialize the space on its end.
///
/// Thin wrapper over `SpaceSnapshot` that adds the member list and
/// member users (which the host gathers separately for the pairing
/// handshake and aren't part of the reusable snapshot structure).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceBundle {
    pub space: WireSpace,
    pub members: Vec<WireMember>,
    pub users: Vec<WireUser>,
    pub categories: Vec<WireCategory>,
    pub accounts: Vec<WireAccount>,
    pub transactions: Vec<WireTransaction>,
    pub account_summaries: Vec<WireAccountSummary>,
    pub space_settings: Vec<WireSpaceSetting>,
    pub host_device_id: String,
    pub host_device_name: String,
    pub host_cert_pem: String,
}

/// Small, always-fits-in-1MB header sent first in the host→joiner
/// stream. Contains the space identity, members, users, and the host's
/// network identity. The joiner persists this before any table data so
/// it can reject obviously-invalid transfers early.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PairHeader {
    space: WireSpace,
    members: Vec<WireMember>,
    users: Vec<WireUser>,
    host_device_id: String,
    host_device_name: String,
    host_cert_pem: String,
}

/// Tagged envelope for the host→joiner pairing stream. Each variant is
/// serialized, encrypted with a fresh nonce, and sent as an independent
/// length-prefixed frame under the 1 MB cap. Postcard encodes the variant
/// tag as one byte.
#[derive(Debug, Clone, Serialize, Deserialize)]
enum PairFrame {
    Header(PairHeader),
    Chunk(SnapshotFrame),
    End,
}

/// Stream snapshot data in chunks, producing one `PairFrame::Chunk`
/// per chunk. The host sends these between `PairFrame::Header` and
/// `PairFrame::End`. `wrap` converts an owned `Vec<T>` chunk into the
/// corresponding `SnapshotFrame` variant.
async fn stream_pair_chunks<T>(
    stream: &mut TcpStream,
    cipher: &ChaCha20Poly1305,
    items: &[T],
    wrap: impl Fn(Vec<T>) -> SnapshotFrame,
) -> Result<(), AppError>
where
    T: Clone,
{
    for slice in items.chunks(CHUNK_SIZE) {
        let owned: Vec<T> = slice.to_vec();
        let frame = wrap(owned);
        write_encrypted(stream, cipher, &PairFrame::Chunk(frame)).await?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Pending token registry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct PendingToken {
    space_id: String,
    expires_at: Instant,
}

/// In-memory map of outstanding pair tokens. Cleaned up when consumed,
/// expired, or when the app shuts down.
#[derive(Default)]
pub struct PairingState {
    pending: Arc<Mutex<HashMap<String, PendingToken>>>,
}

impl PairingState {
    pub fn new() -> Self {
        Self::default()
    }

    fn insert(&self, token: String, space_id: String, ttl: Duration) -> Result<(), AppError> {
        let mut guard = self
            .pending
            .lock()
            .map_err(|e| AppError::Internal(format!("pairing mutex poisoned: {e}")))?;
        guard.insert(
            token,
            PendingToken {
                space_id,
                expires_at: Instant::now() + ttl,
            },
        );
        Ok(())
    }

    fn take(&self, token: &str) -> Option<String> {
        let mut guard = self.pending.lock().ok()?;
        let pending = guard.remove(token)?;
        if pending.expires_at < Instant::now() {
            return None;
        }
        Some(pending.space_id)
    }
}

// ---------------------------------------------------------------------------
// Host side — listen for an incoming pairing
// ---------------------------------------------------------------------------

/// Inputs the host side needs to pre-package the space context before
/// any peer connects. Owner of the space gathers these from the DB,
/// hands them to `start_host_session`, and the session task does the
/// rest.
pub struct HostInputs {
    pub space: WireSpace,
    pub members: Vec<WireMember>,
    pub member_users: Vec<WireUser>,
    pub owner_user: WireUser,
    pub host_display_name: String,
    pub categories: Vec<WireCategory>,
    pub accounts: Vec<WireAccount>,
    pub transactions: Vec<WireTransaction>,
    pub account_summaries: Vec<WireAccountSummary>,
    pub space_settings: Vec<WireSpaceSetting>,
}

/// Generates a fresh 6-digit token, starts a single-shot listener on an
/// ephemeral port, and registers the token in the in-memory pairing
/// state. The listener auto-times-out after `HANDSHAKE_DEADLINE_SECS`.
pub async fn start_host_session(
    db: SqlitePool,
    state: Arc<PairingState>,
    inputs: HostInputs,
) -> Result<PairToken, AppError> {
    let identity = crate::sync::identity::ensure_identity(&db).await?;
    let space_id = inputs.space.id.clone();

    let token = {
        let mut buf = [0u8; 4];
        fill(&mut buf).map_err(|e| AppError::Internal(format!("token rng: {e}")))?;
        let n = u32::from_le_bytes(buf) % 1_000_000;
        format!("{n:06}")
    };

    let listener = TcpListener::bind(format!("0.0.0.0:{PAIRING_PORT}"))
        .await
        .map_err(|e| AppError::Io(format!("pairing listen: {e}")))?;
    let local_addr = listener
        .local_addr()
        .map_err(|e| AppError::Io(format!("pairing addr: {e}")))?;

    state.insert(
        token.clone(),
        space_id.clone(),
        Duration::from_secs(TOKEN_TTL_SECS),
    )?;

    let bundle = SpaceBundle {
        space: inputs.space,
        members: inputs.members,
        users: {
            let mut u = inputs.member_users;
            if !u.iter().any(|x| x.id == inputs.owner_user.id) {
                u.push(inputs.owner_user);
            }
            u
        },
        categories: inputs.categories,
        accounts: inputs.accounts,
        transactions: inputs.transactions,
        account_summaries: inputs.account_summaries,
        space_settings: inputs.space_settings,
        host_device_id: identity.device_id.clone(),
        host_device_name: inputs.host_display_name,
        host_cert_pem: identity.cert_pem.clone(),
    };

    let token_for_session = token.clone();
    let state_for_session = state.clone();
    tokio::spawn(async move {
        let result = tokio::time::timeout(
            Duration::from_secs(HANDSHAKE_DEADLINE_SECS),
            run_host_session(listener, token_for_session.clone(), space_id, bundle, db),
        )
        .await;
        // Always drop the token after the session ends or times out.
        let _ = state_for_session.take(&token_for_session);
        if let Err(e) = result {
            eprintln!("pairing host: timed out — {e}");
        }
    });

    let expires_at_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
        + TOKEN_TTL_SECS;

    Ok(PairToken {
        token: token.clone(),
        address: format!("{token}|{local_addr}"),
        expires_at_unix,
    })
}

async fn run_host_session(
    listener: TcpListener,
    token: String,
    space_id: String,
    bundle: SpaceBundle,
    db: SqlitePool,
) -> Result<(), AppError> {
    let (mut stream, _peer) = listener
        .accept()
        .await
        .map_err(|e| AppError::Io(format!("pairing accept: {e}")))?;

    // SPAKE2 symmetric handshake.
    let (state, our_msg) = Spake2::<Ed25519Group>::start_symmetric(
        &Password::new(token.as_bytes()),
        &PakeIdentity::new(PAKE_IDENTITY),
    );
    write_raw(&mut stream, &our_msg).await?;
    let their_msg = read_raw(&mut stream).await?;
    let key = state
        .finish(&their_msg)
        .map_err(|e| AppError::Internal(format!("pake finish: {e:?}")))?;
    let cipher = ChaCha20Poly1305::new_from_slice(&key[..32])
        .map_err(|e| AppError::Internal(format!("aead init: {e}")))?;

    // Receive JoinPayload from the joiner.
    let join: JoinPayload = read_encrypted(&mut stream, &cipher).await?;

    // Stream the space data: header first, then chunked table data, then
    // an End marker. Each frame is independently encrypted and stays
    // under the 1 MB cap regardless of space size.
    let header = PairHeader {
        space: bundle.space.clone(),
        members: bundle.members.clone(),
        users: bundle.users.clone(),
        host_device_id: bundle.host_device_id.clone(),
        host_device_name: bundle.host_device_name.clone(),
        host_cert_pem: bundle.host_cert_pem.clone(),
    };
    write_encrypted(&mut stream, &cipher, &PairFrame::Header(header)).await?;

    stream_pair_chunks(&mut stream, &cipher, &bundle.categories, |c| {
        SnapshotFrame::Categories(c)
    })
    .await?;
    stream_pair_chunks(&mut stream, &cipher, &bundle.accounts, |c| {
        SnapshotFrame::Accounts(c)
    })
    .await?;
    stream_pair_chunks(&mut stream, &cipher, &bundle.transactions, |c| {
        SnapshotFrame::Transactions(c)
    })
    .await?;
    stream_pair_chunks(&mut stream, &cipher, &bundle.account_summaries, |c| {
        SnapshotFrame::AccountSummaries(c)
    })
    .await?;
    stream_pair_chunks(&mut stream, &cipher, &bundle.space_settings, |c| {
        SnapshotFrame::SpaceSettings(c)
    })
    .await?;

    write_encrypted(&mut stream, &cipher, &PairFrame::End).await?;

    // Persist the joiner as a trusted device of this space.
    upsert_trusted_device(
        &db,
        &space_id,
        &join.device_id,
        &join.device_name,
        &join.cert_pem,
    )
    .await?;

    // Only add a new member if the joiner sent a user payload.
    // If `user` is None, the joiner is an existing member joining on a new
    // device — just trust the device, don't create a duplicate membership.
    if let Some(ref u) = join.user {
        upsert_user(&db, u).await?;
        upsert_space_member(&db, &space_id, &u.id, "member").await?;
    }

    stream.shutdown().await.ok();
    Ok(())
}

// ---------------------------------------------------------------------------
// Joiner side — connect using a token+address
// ---------------------------------------------------------------------------

/// Minimal info returned to the caller after successful pairing. The
/// full `SpaceBundle` has been persisted to the local DB and consumed
/// by the apply path; only the fields the UI needs go back.
#[derive(Debug, Clone)]
pub struct PairingResult {
    pub space_id: String,
    pub space_name: String,
    pub users: Vec<WireUser>,
}

/// Connects to the host using the `digits|host:port` form of the token,
/// runs SPAKE2, exchanges identity, and persists everything locally.
pub async fn run_joiner(
    db: SqlitePool,
    address: String,
    joining_user: Option<WireUser>,
    device_display_name: String,
) -> Result<PairingResult, AppError> {
    let (digits, target) = parse_address(&address)?;
    let identity = crate::sync::identity::ensure_identity(&db).await?;

    let mut stream = tokio::time::timeout(
        Duration::from_secs(10),
        TcpStream::connect::<SocketAddr>(target),
    )
    .await
    .map_err(|_| AppError::Io("pairing connect: timeout".into()))?
    .map_err(|e| AppError::Io(format!("pairing connect: {e}")))?;

    let (state, our_msg) = Spake2::<Ed25519Group>::start_symmetric(
        &Password::new(digits.as_bytes()),
        &PakeIdentity::new(PAKE_IDENTITY),
    );
    let their_msg = read_raw(&mut stream).await?;
    write_raw(&mut stream, &our_msg).await?;
    let key = state
        .finish(&their_msg)
        .map_err(|e| AppError::Internal(format!("pake finish: {e:?}")))?;
    let cipher = ChaCha20Poly1305::new_from_slice(&key[..32])
        .map_err(|e| AppError::Internal(format!("aead init: {e}")))?;

    let join = JoinPayload {
        user: joining_user,
        device_id: identity.device_id.clone(),
        device_name: device_display_name,
        cert_pem: identity.cert_pem.clone(),
    };
    write_encrypted(&mut stream, &cipher, &join).await?;

    // Read the header first (outside any transaction) so we know the
    // host's device_id before we open the apply_guard transaction —
    // the override must be set as the very first statement.
    let header: PairHeader = match read_encrypted(&mut stream, &cipher).await? {
        PairFrame::Header(h) => h,
        other => {
            return Err(AppError::Internal(format!(
                "pairing: expected Header, got {:?}",
                std::mem::discriminant(&other)
            )));
        }
    };

    let host_device_id = header.host_device_id.clone();
    let space_id = header.space.id.clone();
    let space_name = header.space.name.clone();
    let users = header.users.clone();

    // Persist everything received. Run inside apply_guard so the
    // change_log override is set to the host's device_id — every
    // change_log trigger on a synced table stamps the host's id, so
    // the inserted rows originated on the host and don't echo back as
    // local changes on subsequent sync sessions.
    //
    // The stream is moved into the closure so the remaining frames
    // can be read inside the transaction. A mid-stream failure rolls
    // back every DB write; the joiner can then retry pairing.
    crate::sync::apply_guard::run_as_device(&db, &host_device_id, move |tx| {
        Box::pin(async move {
            // Build a SpaceSnapshot skeleton from the header so we can
            // pass it to the reusable `apply_snapshot_frame` helper.
            // The space row gets filled in from the first Space chunk
            // (or we already have it from the header).
            let mut snapshot = SpaceSnapshot {
                space: header.space.clone(),
                members: header.members.clone(),
                users: header.users.clone(),
                categories: Vec::new(),
                accounts: Vec::new(),
                transactions: Vec::new(),
                account_summaries: Vec::new(),
                space_settings: Vec::new(),
                model_versions: Vec::new(),
                host_device_id: header.host_device_id.clone(),
                host_device_name: header.host_device_name.clone(),
                host_cert_pem: header.host_cert_pem.clone(),
            };
            apply_header(tx, &header).await?;
            loop {
                let frame: PairFrame = read_encrypted(&mut stream, &cipher).await?;
                match frame {
                    PairFrame::Header(_) => {
                        return Err(AppError::Internal("pairing: duplicate Header frame".into()));
                    }
                    PairFrame::Chunk(chunk) => {
                        // Lift the space identity if we ever get a Space
                        // chunk (defensive — host sends it in the header).
                        if let SnapshotFrame::Space(s) = &chunk {
                            snapshot.space = s.clone();
                        }
                        crate::sync::snapshot::apply_snapshot_frame(tx, &snapshot, &chunk).await?;
                    }
                    PairFrame::End => {
                        stream.shutdown().await.ok();
                        break;
                    }
                }
            }
            Ok(())
        })
    })
    .await
    .map_err(|e| AppError::Db(format!("pairing persist: {e}")))?;

    Ok(PairingResult {
        space_id,
        space_name,
        users,
    })
}

// ---------------------------------------------------------------------------
// Host-side upserts (run outside apply_guard — the host is creating
// its own rows, not applying a peer's data)
// ---------------------------------------------------------------------------

async fn upsert_user(db: &SqlitePool, user: &WireUser) -> Result<(), AppError> {
    let pin = user.pin_hash.clone();
    sqlx::query_file!(
        "queries/sync/upsert_user.sql",
        user.id,
        user.name,
        pin,
        user.created_at,
        user.updated_at,
    )
    .execute(db)
    .await
    .map_err(|e| AppError::Db(format!("upsert_user: {e}")))?;
    Ok(())
}

async fn upsert_space_member(
    db: &SqlitePool,
    space_id: &str,
    user_id: &str,
    role: &str,
) -> Result<(), AppError> {
    sqlx::query_file!(
        "queries/sync/upsert_space_member.sql",
        space_id,
        user_id,
        role
    )
    .execute(db)
    .await
    .map_err(|e| AppError::Db(format!("upsert_space_member: {e}")))?;
    Ok(())
}

async fn upsert_trusted_device(
    db: &SqlitePool,
    space_id: &str,
    device_id: &str,
    display_name: &str,
    cert_pem: &str,
) -> Result<(), AppError> {
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query_file!(
        "queries/sync/upsert_trusted_device.sql",
        id,
        space_id,
        device_id,
        display_name,
        cert_pem,
    )
    .execute(db)
    .await
    .map_err(|e| AppError::Db(format!("upsert_trusted_device: {e}")))?;
    Ok(())
}

fn parse_address(s: &str) -> Result<(String, SocketAddr), AppError> {
    let (digits, addr) = s
        .split_once('|')
        .ok_or_else(|| AppError::InvalidInput("pair token: missing '|' separator".into()))?;
    if digits.len() != 6 || !digits.chars().all(|c| c.is_ascii_digit()) {
        return Err(AppError::InvalidInput(
            "pair token: digits must be exactly 6 ASCII digits".into(),
        ));
    }
    let parsed: SocketAddr = addr
        .parse()
        .map_err(|e| AppError::InvalidInput(format!("pair token: bad address: {e}")))?;
    Ok((digits.to_string(), parsed))
}

// ---------------------------------------------------------------------------
// Wire framing
// ---------------------------------------------------------------------------

async fn write_raw(stream: &mut TcpStream, buf: &[u8]) -> Result<(), AppError> {
    let len =
        u32::try_from(buf.len()).map_err(|_| AppError::Internal("pair frame too large".into()))?;
    stream
        .write_all(&len.to_be_bytes())
        .await
        .map_err(|e| AppError::Io(format!("pair write: {e}")))?;
    stream
        .write_all(buf)
        .await
        .map_err(|e| AppError::Io(format!("pair write: {e}")))?;
    Ok(())
}

async fn read_raw(stream: &mut TcpStream) -> Result<Vec<u8>, AppError> {
    let mut len_buf = [0u8; 4];
    stream
        .read_exact(&mut len_buf)
        .await
        .map_err(|e| AppError::Io(format!("pair read len: {e}")))?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > 1024 * 1024 {
        return Err(AppError::Internal("pair frame > 1 MiB".into()));
    }
    let mut buf = vec![0u8; len];
    stream
        .read_exact(&mut buf)
        .await
        .map_err(|e| AppError::Io(format!("pair read: {e}")))?;
    Ok(buf)
}

async fn write_encrypted<T: Serialize>(
    stream: &mut TcpStream,
    cipher: &ChaCha20Poly1305,
    msg: &T,
) -> Result<(), AppError> {
    let plain =
        postcard::to_allocvec(msg).map_err(|e| AppError::Internal(format!("pair encode: {e}")))?;
    let mut nonce_bytes = [0u8; 12];
    fill(&mut nonce_bytes).map_err(|e| AppError::Internal(format!("nonce rng: {e}")))?;
    let nonce: &Nonce = (&nonce_bytes).into();
    let ct = cipher
        .encrypt(nonce, plain.as_slice())
        .map_err(|e| AppError::Internal(format!("pair encrypt: {e}")))?;
    let mut framed = Vec::with_capacity(12 + ct.len());
    framed.extend_from_slice(&nonce_bytes);
    framed.extend_from_slice(&ct);
    write_raw(stream, &framed).await
}

async fn read_encrypted<T: for<'de> Deserialize<'de>>(
    stream: &mut TcpStream,
    cipher: &ChaCha20Poly1305,
) -> Result<T, AppError> {
    let buf = read_raw(stream).await?;
    if buf.len() < 12 + 16 {
        return Err(AppError::Internal("pair frame: too short".into()));
    }
    let (nonce_bytes, ct) = buf.split_at(12);
    let nonce = <&Nonce>::try_from(nonce_bytes)
        .map_err(|e| AppError::Internal(format!("pair nonce: {e}")))?;
    let plain = cipher
        .decrypt(nonce, ct)
        .map_err(|e| AppError::Internal(format!("pair decrypt: {e}")))?;
    postcard::from_bytes::<T>(&plain).map_err(|e| AppError::Internal(format!("pair decode: {e}")))
}

/// Slice `items` into chunks of CHUNK_SIZE and send each one as an
/// independently-encrypted `PairFrame::Chunk` wrapping a `SnapshotFrame`.
/// `wrap` receives the chunk (owned) and produces a `SnapshotFrame`
/// variant. Items that serialize to more than 1 MB in one chunk will
/// be rejected by `read_raw` — keep `CHUNK_SIZE` conservative.
const CHUNK_SIZE: usize = 500;

// ---------------------------------------------------------------------------
// DB upserts used by both sides
// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------

/// Persist the header's space, members, users, and trusted_device
/// inside an open apply_guard transaction. Delegates to the shared
/// `apply_snapshot_frame` helper so the pairing path uses exactly the
/// same upserts as the sync-session snapshot fallback.
async fn apply_header(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    header: &PairHeader,
) -> Result<(), AppError> {
    let skeleton = SpaceSnapshot {
        space: header.space.clone(),
        members: header.members.clone(),
        users: header.users.clone(),
        categories: Vec::new(),
        accounts: Vec::new(),
        transactions: Vec::new(),
        account_summaries: Vec::new(),
        space_settings: Vec::new(),
        model_versions: Vec::new(),
        host_device_id: header.host_device_id.clone(),
        host_device_name: header.host_device_name.clone(),
        host_cert_pem: header.host_cert_pem.clone(),
    };
    crate::sync::snapshot::apply_snapshot_frame(
        tx,
        &skeleton,
        &SnapshotFrame::Space(header.space.clone()),
    )
    .await?;
    crate::sync::snapshot::apply_snapshot_frame(
        tx,
        &skeleton,
        &SnapshotFrame::Members(header.members.clone()),
    )
    .await?;
    crate::sync::snapshot::apply_snapshot_frame(
        tx,
        &skeleton,
        &SnapshotFrame::Users(header.users.clone()),
    )
    .await?;
    crate::sync::snapshot::apply_snapshot_frame(tx, &skeleton, &SnapshotFrame::End).await?;
    Ok(())
}
