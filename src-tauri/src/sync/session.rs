use sqlx::SqlitePool;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::sync::oneshot;

use crate::error::AppError;
use crate::sync::apply_guard::{GuardedFuture, run_as_device};
use crate::sync::cert_match::PeerIdentity;
use crate::sync::wire::{
    Bye, ChangeBatch, ChangeRow, CursorEntry, Cursors, Frame, Hello, ModelVersionSummary,
    PROTOCOL_VERSION,
};
use crate::sync::{apply, changelog, cursors, model_sync};

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
    app: AppHandle,
) -> Result<(), AppError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let local_device_id = read_device_id(&db).await?;
    let (rd, wr) = tokio::io::split(stream);
    run_session(rd, wr, db, app, local_device_id, peer).await
}

/// Handle an outbound mTLS session (called by the dialer in `sync/client.rs`).
pub async fn handle_outbound<S>(
    stream: tokio_rustls::client::TlsStream<S>,
    peer: PeerIdentity,
    db: SqlitePool,
    app: AppHandle,
) -> Result<(), AppError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let local_device_id = read_device_id(&db).await?;
    let (rd, wr) = tokio::io::split(stream);
    run_session(rd, wr, db, app, local_device_id, peer).await
}

// ---------------------------------------------------------------------------
// Core session
// ---------------------------------------------------------------------------

/// Run the full delta-exchange + model-sync protocol over a split async
/// stream. Send and receive halves run concurrently. Protocol order:
///
///   Hello ↔ Hello
///   Cursors ↔ Cursors
///   ChangeBatch* (from us to peer, per peer's cursors)
///   ↕ simultaneous with peer shipping their batches to us
///   ModelVersionSummary ↔ ModelVersionSummary
///   ModelData* (from us if our version > peer's, received if peer's > ours)
///   Bye ↔ Bye
async fn run_session<R, W>(
    read_half: R,
    write_half: W,
    db: SqlitePool,
    app: AppHandle,
    local_device_id: String,
    peer: PeerIdentity,
) -> Result<(), AppError>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let (hello_tx, hello_rx) = oneshot::channel::<Hello>();
    let (cursors_tx, cursors_rx) = oneshot::channel::<Cursors>();
    let (model_versions_tx, model_versions_rx) = oneshot::channel::<ModelVersionSummary>();

    let db_recv = db.clone();
    let app_recv = app.clone();
    let peer_recv = peer.clone();

    let send_fut = send_half(
        write_half,
        db.clone(),
        app.clone(),
        local_device_id.clone(),
        peer.clone(),
        hello_rx,
        cursors_rx,
        model_versions_rx,
    );
    let recv_fut = recv_half(
        read_half,
        db_recv,
        app_recv,
        local_device_id,
        peer_recv,
        hello_tx,
        cursors_tx,
        model_versions_tx,
    );

    let (send_res, recv_res) = tokio::join!(send_fut, recv_fut);
    send_res?;
    recv_res?;

    for space_id in &peer.shared_space_ids {
        let _ = sqlx::query_file!(
            "queries/sync/delete_evicted_device.sql",
            space_id,
            peer.device_id
        )
        .execute(&db)
        .await;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Send half
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
async fn send_half<W>(
    mut wr: W,
    db: SqlitePool,
    app: AppHandle,
    local_device_id: String,
    peer: PeerIdentity,
    hello_rx: oneshot::Receiver<Hello>,
    cursors_rx: oneshot::Receiver<Cursors>,
    model_versions_rx: oneshot::Receiver<ModelVersionSummary>,
) -> Result<(), AppError>
where
    W: AsyncWrite + Unpin,
{
    // --- Hello handshake ---
    write_frame(
        &mut wr,
        &Frame::Hello(Hello {
            protocol_version: PROTOCOL_VERSION,
            device_id: local_device_id.clone(),
        }),
    )
    .await?;

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

    // --- Cursor exchange ---
    let mut cursor_entries = Vec::new();
    for space_id in &peer.shared_space_ids {
        let devices = sqlx::query_file!(
            "queries/sync/list_trusted_devices.sql",
            space_id
        )
        .fetch_all(&db)
        .await
        .map_err(|e| AppError::Db(format!("session cursors query: {e}")))?;

        for d in devices {
            if d.device_id != local_device_id {
                let last_seq = cursors::get(&db, space_id, &d.device_id).await?;
                cursor_entries.push(CursorEntry {
                    space_id: space_id.clone(),
                    device_id: d.device_id,
                    last_seq,
                });
            }
        }
    }
    write_frame(
        &mut wr,
        &Frame::Cursors(Cursors {
            entries: cursor_entries,
        }),
    )
    .await?;

    let peer_cursors = cursors_rx.await.map_err(|_| {
        AppError::Internal("session: recv half dropped before sending Cursors".into())
    })?;

    // --- Change batches ---
    for entry in &peer_cursors.entries {
        if !peer.shared_space_ids.contains(&entry.space_id) {
            continue;
        }
        ship_batches(
            &mut wr,
            &db,
            &entry.space_id,
            &entry.device_id,
            entry.last_seq,
        )
        .await?;
    }
    let mentioned: std::collections::HashSet<_> = peer_cursors
        .entries
        .iter()
        .map(|e| e.space_id.as_str())
        .collect();
    for space_id in &peer.shared_space_ids {
        if !mentioned.contains(space_id.as_str()) {
            let devices = sqlx::query_file!(
                "queries/sync/list_trusted_devices.sql",
                space_id
            )
            .fetch_all(&db)
            .await
            .map_err(|e| AppError::Db(format!("session fallback devices query: {e}")))?;

            for d in devices {
                if d.device_id != peer.device_id {
                    ship_batches(&mut wr, &db, space_id, &d.device_id, 0).await?;
                }
            }
            ship_batches(&mut wr, &db, space_id, &local_device_id, 0).await?;
        }
    }

    // --- Model version exchange ---
    let local_summary = model_sync::local_summary(&db, &peer.shared_space_ids).await;
    write_frame(&mut wr, &Frame::ModelVersionSummary(local_summary)).await?;

    let peer_summary = model_versions_rx.await.map_err(|_| {
        AppError::Internal("session: recv half dropped before sending ModelVersionSummary".into())
    })?;

    // Push our model to the peer for any space where our version is higher.
    for peer_entry in &peer_summary.entries {
        if !peer.shared_space_ids.contains(&peer_entry.space_id) {
            continue;
        }
        let local_ver = model_sync::local_version(&db, &peer_entry.space_id).await;
        if local_ver > peer_entry.version {
            match model_sync::read_model(&app, &peer_entry.space_id, local_ver)? {
                Some(data) => {
                    write_frame(&mut wr, &Frame::ModelData(data)).await?;
                }
                None => {
                    eprintln!(
                        "session: model v{local_ver} for space {} missing on disk, skipping",
                        peer_entry.space_id
                    );
                }
            }
        }
    }

    // --- Close ---
    write_frame(&mut wr, &Frame::Bye(Bye {})).await?;
    wr.flush()
        .await
        .map_err(|e| AppError::Io(format!("session flush: {e}")))?;
    Ok(())
}

/// Ship change_log rows for `(space_id, local_device_id)` with seq >
/// peer_last_seq in batches of `DEFAULT_BATCH_LIMIT`.
async fn ship_batches<W>(
    wr: &mut W,
    db: &SqlitePool,
    space_id: &str,
    device_id: &str,
    peer_last_seq: i64,
) -> Result<(), AppError>
where
    W: AsyncWrite + Unpin,
{
    let final_seq = changelog::max_seq(db, space_id, device_id).await?;
    let min_seq = changelog::min_seq(db, space_id, device_id).await?;

    if peer_last_seq > 0 && min_seq > 0 && peer_last_seq < min_seq {
        eprintln!(
            "session: peer cursor {peer_last_seq} < min_seq {min_seq} for space {space_id} device {device_id}; \
             full snapshot required (not yet implemented)"
        );
        write_frame(
            wr,
            &Frame::Batch(ChangeBatch {
                space_id: space_id.to_string(),
                device_id: device_id.to_string(),
                rows: vec![],
                final_seq,
            }),
        )
        .await?;
        return Ok(());
    }

    let mut last_sent = peer_last_seq;
    loop {
        let rows = changelog::read_since(
            db,
            space_id,
            device_id,
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

        write_frame(
            wr,
            &Frame::Batch(ChangeBatch {
                space_id: space_id.to_string(),
                device_id: device_id.to_string(),
                rows,
                final_seq: batch_final,
            }),
        )
        .await?;

        if done {
            break;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Receive half
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
async fn recv_half<R>(
    mut rd: R,
    db: SqlitePool,
    app: AppHandle,
    local_device_id: String,
    peer: PeerIdentity,
    hello_tx: oneshot::Sender<Hello>,
    cursors_tx: oneshot::Sender<Cursors>,
    model_versions_tx: oneshot::Sender<ModelVersionSummary>,
) -> Result<(), AppError>
where
    R: AsyncRead + Unpin,
{
    // Hello
    let hello = expect_hello(&mut rd).await?;
    hello_tx.send(hello).map_err(|_| {
        AppError::Internal("session: send half dropped before receiving Hello".into())
    })?;

    // Cursors
    let peer_cursors = expect_cursors(&mut rd).await?;
    cursors_tx.send(peer_cursors).map_err(|_| {
        AppError::Internal("session: send half dropped before receiving Cursors".into())
    })?;

    // Frame loop: ChangeBatch* then ModelVersionSummary then ModelData* then Bye
    let mut peer_model_summary_sent = false;
    let mut model_versions_tx = Some(model_versions_tx);
    loop {
        let frame = crate::sync::frame::read_frame(&mut rd).await?;
        match frame {
            Frame::Batch(batch) => {
                apply_batch(&db, &local_device_id, &peer, batch, &app).await?;
            }
            Frame::ModelVersionSummary(summary) => {
                if let Some(tx) = model_versions_tx.take() {
                    tx.send(summary).map_err(|_| {
                        AppError::Internal(
                            "session: send half dropped before receiving ModelVersionSummary"
                                .into(),
                        )
                    })?;
                }
                peer_model_summary_sent = true;
            }
            Frame::ModelData(data) => {
                if !peer_model_summary_sent {
                    return Err(AppError::Internal(
                        "session: ModelData arrived before ModelVersionSummary".into(),
                    ));
                }
                if peer.shared_space_ids.contains(&data.space_id) {
                    let local_ver = model_sync::local_version(&db, &data.space_id).await;
                    if data.version > local_ver
                        && let Err(e) = model_sync::apply_model(&app, &db, &data).await
                    {
                        eprintln!(
                            "session: apply model v{} for space {}: {e}",
                            data.version, data.space_id
                        );
                    }
                }
            }
            Frame::Bye(_) => break,
            other => {
                return Err(AppError::Internal(format!(
                    "session: unexpected frame {:?}",
                    std::mem::discriminant(&other)
                )));
            }
        }
    }
    Ok(())
}

/// Apply one `ChangeBatch` atomically with its cursor advance, and emit
/// `sync://evicted` if this device's own trusted_devices row was deleted.
async fn apply_batch(
    db: &SqlitePool,
    local_device_id: &str,
    peer: &PeerIdentity,
    batch: ChangeBatch,
    app: &AppHandle,
) -> Result<(), AppError> {
    if !peer.shared_space_ids.contains(&batch.space_id) {
        return Ok(());
    }

    let kind = classify_batch(&batch.rows, local_device_id);
    let is_space_deletion = matches!(kind, BatchKind::SpaceDeletion);
    let evicted = matches!(kind, BatchKind::PotentialEviction);

    let space_id = batch.space_id.clone();
    let evicted_space_id = batch.space_id.clone();
    let batch_device_id = batch.device_id.clone();
    let final_seq = batch.final_seq;
    let batch_space_id = batch.space_id.clone();

    if evicted {
        let deleted_ids: Vec<String> = batch
            .rows
            .iter()
            .filter(|r| r.table_name == "trusted_devices" && r.operation == "delete")
            .map(|r| r.row_id.clone())
            .collect();

        let own_rows = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM trusted_devices WHERE device_id = ?1 AND id IN (\
             SELECT value FROM json_each(?2))",
            local_device_id,
            serde_json::to_string(&deleted_ids).unwrap_or_default()
        )
        .fetch_one(db)
        .await
        .unwrap_or(0);

        if own_rows > 0 {
            let device_id_for_closure = batch_device_id.clone();
            run_as_device(db, &batch_device_id, move |tx| -> GuardedFuture<'_, ()> {
                let device_id_inner = device_id_for_closure.clone();
                Box::pin(async move {
                    for row in &batch.rows {
                        apply::apply_change(tx, row).await?;
                    }
                    cursors::advance(tx, &space_id, &device_id_inner, final_seq).await?;
                    Ok(())
                })
            })
            .await
            .map_err(|e| AppError::Db(format!("apply_batch {local_device_id}: {e}")))?;

            let _ = app.emit("sync://evicted", &evicted_space_id);
            return Ok(());
        }
    }

    let device_id_for_closure = batch_device_id.clone();
    run_as_device(db, &batch_device_id, move |tx| -> GuardedFuture<'_, ()> {
        let device_id_inner = device_id_for_closure.clone();
        Box::pin(async move {
            for row in &batch.rows {
                apply::apply_change(tx, row).await?;
            }
            cursors::advance(tx, &batch_space_id, &device_id_inner, final_seq).await?;
            Ok(())
        })
    })
    .await
    .map_err(|e| AppError::Db(format!("apply_batch {local_device_id}: {e}")))?;

    if is_space_deletion {
        let _ = app.emit("sync://space-deleted", &space_id);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Batch classification
// ---------------------------------------------------------------------------

/// Classify a change batch to determine if it represents a space deletion
/// (suppresses eviction detection) or a potential device revocation
/// (triggers the eviction DB check). See PLAN.md §9.5.
enum BatchKind {
    SpaceDeletion,
    PotentialEviction,
    Normal,
}

/// A batch containing a `spaces` delete is a space-deletion propagation.
/// We suppress the eviction check for the whole batch in that case, since
/// the `trusted_devices` deletes that ride along are cascade effects of the
/// space deletion, not an explicit device revocation. The only way both
/// could coexist in one batch is if a space deletion and an unrelated
/// device revocation happened to share the same shipping window —
/// acceptable risk: the revoked device's data is gone either way, and the
/// next batch from the same peer will re-trigger eviction detection.
fn classify_batch(rows: &[ChangeRow], local_device_id: &str) -> BatchKind {
    if rows
        .iter()
        .any(|r| r.table_name == "spaces" && r.operation == "delete")
    {
        return BatchKind::SpaceDeletion;
    }
    if rows.iter().any(|r| {
        r.table_name == "trusted_devices"
            && r.operation == "delete"
            && r.device_id != local_device_id
    }) {
        return BatchKind::PotentialEviction;
    }
    BatchKind::Normal
}

// ---------------------------------------------------------------------------
// Frame + settings helpers
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

async fn read_device_id(db: &SqlitePool) -> Result<String, AppError> {
    let key = "device_id";
    sqlx::query_file_scalar!("queries/settings/get_setting.sql", key)
        .fetch_optional(db)
        .await
        .map_err(|e| AppError::Db(format!("read_device_id: {e}")))?
        .ok_or_else(|| AppError::Internal("device_id not initialised in app_settings".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change_row(table: &str, op: &str, device_id: &str) -> ChangeRow {
        ChangeRow {
            id: "x".into(),
            space_id: "s1".into(),
            table_name: table.into(),
            row_id: "r1".into(),
            operation: op.into(),
            payload: None,
            seq: 1,
            device_id: device_id.into(),
            changed_at: "2024-01-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn classify_batch_detects_space_deletion() {
        let rows = vec![
            change_row("space_members", "delete", "peer-1"),
            change_row("spaces", "delete", "peer-1"),
            change_row("trusted_devices", "delete", "peer-1"),
        ];
        assert!(matches!(
            classify_batch(&rows, "local"),
            BatchKind::SpaceDeletion
        ));
    }

    #[test]
    fn classify_batch_detects_device_revocation() {
        let rows = vec![change_row("trusted_devices", "delete", "peer-1")];
        assert!(matches!(
            classify_batch(&rows, "local"),
            BatchKind::PotentialEviction
        ));
    }

    /// A trusted_devices delete authored by us is an echo of our own
    /// change coming back — not an eviction.
    #[test]
    fn classify_batch_ignores_own_device_revocation() {
        let rows = vec![change_row("trusted_devices", "delete", "local")];
        assert!(matches!(classify_batch(&rows, "local"), BatchKind::Normal));
    }

    #[test]
    fn classify_batch_normal_for_inserts_and_updates() {
        let rows = vec![
            change_row("transactions", "insert", "peer-1"),
            change_row("accounts", "update", "peer-1"),
        ];
        assert!(matches!(classify_batch(&rows, "local"), BatchKind::Normal));
    }
}
