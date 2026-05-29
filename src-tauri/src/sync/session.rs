use sqlx::SqlitePool;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::sync::oneshot;

use crate::error::AppError;
use crate::sync::apply_guard::{run_as_device, GuardedFuture};
use crate::sync::cert_match::PeerIdentity;
use crate::sync::wire::{Bye, ChangeBatch, CursorEntry, Cursors, Frame, Hello, PROTOCOL_VERSION};
use crate::sync::{apply, changelog, cursors};

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// Handle a freshly-accepted inbound mTLS session. The caller has already
/// resolved `peer` from `trusted_devices`, so at least one shared space
/// exists and the cert is trusted.
pub async fn handle_inbound<S>(
    stream: tokio_rustls::server::TlsStream<S>,
    peer: PeerIdentity,
    db: SqlitePool,
) -> Result<(), AppError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let local_device_id = read_device_id(&db).await?;
    let (rd, wr) = tokio::io::split(stream);
    run_session(rd, wr, db, local_device_id, peer).await
}

/// Handle an outbound mTLS session (called by the dialer in step 5.7).
pub async fn handle_outbound<S>(
    stream: tokio_rustls::client::TlsStream<S>,
    peer: PeerIdentity,
    db: SqlitePool,
) -> Result<(), AppError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let local_device_id = read_device_id(&db).await?;
    let (rd, wr) = tokio::io::split(stream);
    run_session(rd, wr, db, local_device_id, peer).await
}

// ---------------------------------------------------------------------------
// Core session
// ---------------------------------------------------------------------------

/// Run the full delta-exchange protocol over a split async stream. The send
/// and receive halves run concurrently: each side exchanges Hello and
/// Cursors, then the sender ships change batches while the receiver applies
/// incoming ones. Both sides close gracefully with a Bye frame.
///
/// `local_device_id` is this device's stable id (from `app_settings`).
/// `peer` carries the cert-resolved peer device_id and shared space ids.
async fn run_session<R, W>(
    read_half: R,
    write_half: W,
    db: SqlitePool,
    local_device_id: String,
    peer: PeerIdentity,
) -> Result<(), AppError>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    // Channels for the send half to learn the peer's Hello and Cursors
    // from the receive half once they arrive.
    let (hello_tx, hello_rx) = oneshot::channel::<Hello>();
    let (cursors_tx, cursors_rx) = oneshot::channel::<Cursors>();

    let db_recv = db.clone();
    let peer_recv = peer.clone();

    let send_fut = send_half(
        write_half,
        db.clone(),
        local_device_id.clone(),
        peer.clone(),
        hello_rx,
        cursors_rx,
    );
    let recv_fut = recv_half(
        read_half,
        db_recv,
        local_device_id,
        peer_recv,
        hello_tx,
        cursors_tx,
    );

    let (send_res, recv_res) = tokio::join!(send_fut, recv_fut);
    send_res?;
    recv_res?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Send half — Hello, read peer Hello, Cursors, batches, Bye
// ---------------------------------------------------------------------------

async fn send_half<W>(
    mut wr: W,
    db: SqlitePool,
    local_device_id: String,
    peer: PeerIdentity,
    hello_rx: oneshot::Receiver<Hello>,
    cursors_rx: oneshot::Receiver<Cursors>,
) -> Result<(), AppError>
where
    W: AsyncWrite + Unpin,
{
    // Send our Hello immediately.
    write_frame(&mut wr, &Frame::Hello(Hello {
        protocol_version: PROTOCOL_VERSION,
        device_id: local_device_id.clone(),
    })).await?;

    // Wait for the peer's Hello (delivered by recv_half via channel).
    let peer_hello = hello_rx.await.map_err(|_| {
        AppError::Internal("session: recv half dropped before sending Hello".into())
    })?;
    if peer_hello.protocol_version != PROTOCOL_VERSION {
        return Err(AppError::Internal(format!(
            "session: protocol version mismatch: ours={PROTOCOL_VERSION} peer={}",
            peer_hello.protocol_version
        )));
    }
    if peer_hello.device_id != peer.device_id {
        return Err(AppError::Internal(format!(
            "session: Hello device_id {} doesn't match cert-resolved {}",
            peer_hello.device_id, peer.device_id
        )));
    }

    // Send our Cursors — our high-water marks for each shared space, keyed
    // by how far we have consumed from the peer (not from ourselves).
    let mut cursor_entries = Vec::new();
    for space_id in &peer.shared_space_ids {
        let last_seq = cursors::get(&db, space_id, &peer.device_id).await?;
        cursor_entries.push(CursorEntry {
            space_id: space_id.clone(),
            last_seq,
        });
    }
    write_frame(&mut wr, &Frame::Cursors(Cursors {
        entries: cursor_entries,
    })).await?;

    // Wait for the peer's Cursors to arrive (they tell us how much of our
    // own log they have already seen).
    let peer_cursors = cursors_rx.await.map_err(|_| {
        AppError::Internal("session: recv half dropped before sending Cursors".into())
    })?;

    // Ship our change batches for each space the peer is behind on.
    for entry in &peer_cursors.entries {
        if !peer.shared_space_ids.contains(&entry.space_id) {
            // Peer sent a cursor for a space we don't share — ignore.
            continue;
        }
        ship_batches(
            &mut wr,
            &db,
            &entry.space_id,
            &local_device_id,
            entry.last_seq,
        )
        .await?;
    }

    // Also ship for shared spaces the peer didn't mention at all
    // (no cursor row = they want everything from 0).
    let mentioned: std::collections::HashSet<_> =
        peer_cursors.entries.iter().map(|e| e.space_id.as_str()).collect();
    for space_id in &peer.shared_space_ids {
        if !mentioned.contains(space_id.as_str()) {
            ship_batches(&mut wr, &db, space_id, &local_device_id, 0).await?;
        }
    }

    // Graceful close.
    write_frame(&mut wr, &Frame::Bye(Bye {})).await?;
    wr.flush().await.map_err(|e| AppError::Io(format!("session flush: {e}")))?;
    Ok(())
}

/// Ship all of our change_log rows for `(space_id, local_device_id)` with
/// seq > peer_last_seq, in batches of `changelog::DEFAULT_BATCH_LIMIT`.
/// Terminates each space with a final_seq so the peer can advance its cursor
/// even if no rows need to be sent.
async fn ship_batches<W>(
    wr: &mut W,
    db: &SqlitePool,
    space_id: &str,
    local_device_id: &str,
    peer_last_seq: i64,
) -> Result<(), AppError>
where
    W: AsyncWrite + Unpin,
{
    let final_seq = changelog::max_seq(db, space_id, local_device_id).await?;
    let min_seq = changelog::min_seq(db, space_id, local_device_id).await?;

    // If the peer's cursor is behind our oldest retained row we can't serve
    // the delta — they need a full snapshot (step 7). Log and send an empty
    // batch so the peer can at least record the final_seq.
    if peer_last_seq > 0 && min_seq > 0 && peer_last_seq < min_seq {
        eprintln!(
            "session: peer cursor {peer_last_seq} < min_seq {min_seq} for space {space_id}; \
             full snapshot required (not yet implemented)"
        );
        write_frame(wr, &Frame::Batch(ChangeBatch {
            space_id: space_id.to_string(),
            rows: vec![],
            final_seq,
        }))
        .await?;
        return Ok(());
    }

    let mut last_sent = peer_last_seq;
    loop {
        let rows = changelog::read_since(
            db,
            space_id,
            local_device_id,
            last_sent,
            changelog::DEFAULT_BATCH_LIMIT,
        )
        .await?;

        let done = rows.len() < changelog::DEFAULT_BATCH_LIMIT as usize;
        let batch_final = if done {
            final_seq
        } else {
            rows.last().map(|r| r.seq).unwrap_or(final_seq)
        };

        if let Some(last) = rows.last() {
            last_sent = last.seq;
        }

        write_frame(wr, &Frame::Batch(ChangeBatch {
            space_id: space_id.to_string(),
            rows,
            final_seq: batch_final,
        }))
        .await?;

        if done {
            break;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Receive half — read Hello, Cursors, apply ChangeBatch loop, Bye
// ---------------------------------------------------------------------------

async fn recv_half<R>(
    mut rd: R,
    db: SqlitePool,
    local_device_id: String,
    peer: PeerIdentity,
    hello_tx: oneshot::Sender<Hello>,
    cursors_tx: oneshot::Sender<Cursors>,
) -> Result<(), AppError>
where
    R: AsyncRead + Unpin,
{
    // Read peer Hello.
    let hello = expect_hello(&mut rd).await?;
    hello_tx.send(hello).map_err(|_| {
        AppError::Internal("session: send half dropped before receiving Hello".into())
    })?;

    // Read peer Cursors.
    let peer_cursors = expect_cursors(&mut rd).await?;
    cursors_tx.send(peer_cursors).map_err(|_| {
        AppError::Internal("session: send half dropped before receiving Cursors".into())
    })?;

    // Apply incoming ChangeBatch frames until Bye.
    loop {
        let frame = crate::sync::frame::read_frame(&mut rd).await?;
        match frame {
            Frame::Batch(batch) => {
                apply_batch(&db, &local_device_id, &peer, batch).await?;
            }
            Frame::Bye(_) => break,
            other => {
                return Err(AppError::Internal(format!(
                    "session: unexpected frame {:?} after Cursors",
                    std::mem::discriminant(&other)
                )));
            }
        }
    }
    Ok(())
}

/// Apply one `ChangeBatch`: for each row, run apply_guard → apply_change,
/// then advance the cursor to `batch.final_seq` inside the same transaction.
async fn apply_batch(
    db: &SqlitePool,
    local_device_id: &str,
    peer: &PeerIdentity,
    batch: ChangeBatch,
) -> Result<(), AppError> {
    if !peer.shared_space_ids.contains(&batch.space_id) {
        // Peer sent a batch for a space we don't share — silently drop.
        return Ok(());
    }

    let space_id = batch.space_id.clone();
    let peer_device_id = peer.device_id.clone();
    let final_seq = batch.final_seq;

    run_as_device(db, &peer.device_id, move |tx| -> GuardedFuture<'_, ()> {
        Box::pin(async move {
            for row in &batch.rows {
                apply::apply_change(tx, row).await?;
            }
            cursors::advance(tx, &space_id, &peer_device_id, final_seq).await?;
            Ok(())
        })
    })
    .await
    .map_err(|e| AppError::Db(format!("apply_batch {local_device_id}: {e}")))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Frame helpers
// ---------------------------------------------------------------------------

async fn write_frame<W>(wr: &mut W, frame: &Frame) -> Result<(), AppError>
where
    W: AsyncWrite + Unpin,
{
    crate::sync::frame::write_frame(wr, frame).await
}

async fn expect_hello<R>(rd: &mut R) -> Result<Hello, AppError>
where
    R: AsyncRead + Unpin,
{
    match crate::sync::frame::read_frame(rd).await? {
        Frame::Hello(h) => Ok(h),
        other => Err(AppError::Internal(format!(
            "session: expected Hello, got {:?}",
            std::mem::discriminant(&other)
        ))),
    }
}

async fn expect_cursors<R>(rd: &mut R) -> Result<Cursors, AppError>
where
    R: AsyncRead + Unpin,
{
    match crate::sync::frame::read_frame(rd).await? {
        Frame::Cursors(c) => Ok(c),
        other => Err(AppError::Internal(format!(
            "session: expected Cursors, got {:?}",
            std::mem::discriminant(&other)
        ))),
    }
}

// ---------------------------------------------------------------------------
// Settings helper
// ---------------------------------------------------------------------------

async fn read_device_id(db: &SqlitePool) -> Result<String, AppError> {
    let key = "device_id";
    sqlx::query_file_scalar!("queries/settings/get_setting.sql", key)
        .fetch_optional(db)
        .await
        .map_err(|e| AppError::Db(format!("read_device_id: {e}")))?
        .ok_or_else(|| AppError::Internal("device_id not initialised in app_settings".into()))
}
