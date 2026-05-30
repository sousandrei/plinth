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
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Nonce,
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
const HANDSHAKE_DEADLINE_SECS: u64 = 60;
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

/// Identity exchanged over the encrypted channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireUser {
    pub id: String,
    pub name: String,
    pub pin_hash: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireSpace {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireMember {
    pub space_id: String,
    pub user_id: String,
    pub role: String,
    pub joined_at: String,
}

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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceBundle {
    pub space: WireSpace,
    pub members: Vec<WireMember>,
    pub users: Vec<WireUser>,
    pub host_device_id: String,
    pub host_device_name: String,
    pub host_cert_pem: String,
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

    fn insert(&self, token: String, space_id: String, ttl: Duration) {
        let mut guard = self.pending.lock().unwrap();
        guard.insert(
            token,
            PendingToken {
                space_id,
                expires_at: Instant::now() + ttl,
            },
        );
    }

    fn take(&self, token: &str) -> Option<String> {
        let mut guard = self.pending.lock().unwrap();
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
    );

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

    // Send the space bundle back.
    write_encrypted(&mut stream, &cipher, &bundle).await?;

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

/// Connects to the host using the `digits|host:port` form of the token,
/// runs SPAKE2, exchanges identity, and persists everything locally.
pub async fn run_joiner(
    db: SqlitePool,
    address: String,
    joining_user: Option<WireUser>,
    device_display_name: String,
) -> Result<SpaceBundle, AppError> {
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

    let bundle: SpaceBundle = read_encrypted(&mut stream, &cipher).await?;

    // Persist everything received.
    upsert_space(&db, &bundle.space).await?;
    for u in &bundle.users {
        upsert_user(&db, u).await?;
    }
    for m in &bundle.members {
        upsert_space_member(&db, &m.space_id, &m.user_id, &m.role).await?;
    }
    upsert_trusted_device(
        &db,
        &bundle.space.id,
        &bundle.host_device_id,
        &bundle.host_device_name,
        &bundle.host_cert_pem,
    )
    .await?;

    stream.shutdown().await.ok();
    Ok(bundle)
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
    let len = u32::try_from(buf.len())
        .map_err(|_| AppError::Internal("pair frame too large".into()))?;
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
    let nonce = Nonce::from_slice(&nonce_bytes);
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
    let nonce = Nonce::from_slice(nonce_bytes);
    let plain = cipher
        .decrypt(nonce, ct)
        .map_err(|e| AppError::Internal(format!("pair decrypt: {e}")))?;
    postcard::from_bytes::<T>(&plain)
        .map_err(|e| AppError::Internal(format!("pair decode: {e}")))
}

// ---------------------------------------------------------------------------
// DB upserts used by both sides
// ---------------------------------------------------------------------------
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

async fn upsert_space(db: &SqlitePool, space: &WireSpace) -> Result<(), AppError> {
    sqlx::query_file!(
        "queries/sync/upsert_space.sql",
        space.id,
        space.name,
        space.created_at,
        space.updated_at,
    )
    .execute(db)
    .await
    .map_err(|e| AppError::Db(format!("upsert_space: {e}")))?;
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
