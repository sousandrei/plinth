use serde::Serialize;
use sqlx::{Sqlite, SqlitePool, Transaction};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::sync::oneshot;

use crate::error::AppError;
use crate::sync::apply_guard::{GuardedFuture, run_as_device};
use crate::sync::cert_match::PeerIdentity;
use crate::sync::frame;
use crate::sync::payloads::TablePayload;
use crate::sync::wire::{
    AppliedCursors, Bye, ChangeBatch, ChangeRow, ChangesDone, CursorEntry, Cursors, Frame, Hello,
    ModelVersionSummary, PROTOCOL_VERSION, Pong,
};
use crate::sync::{apply, changelog, cursors, model_sync};

/// Payload for `sync://applied`. Emitted after every successful
/// `Batch` or `Snapshot` apply so the frontend can invalidate its
/// query cache and surface a "synced just now" indicator.
#[derive(Debug, Clone, Serialize)]
pub struct SyncAppliedPayload {
    pub space_id: String,
    pub rows: u64,
    pub snapshot: bool,
}

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// Handle a freshly-accepted inbound mTLS session. The caller has already
/// resolved `peer` from `space_devices`, so at least one shared space
/// exists and the cert is trusted.
///
/// Dispatches on the first frame. `Ping` is a presence-only heartbeat
/// answered with `Pong`+`Bye`. `Hello` kicks off the full sync flow —
/// the Hello exchange is done inline here, so `run_session` starts at
/// the cursor exchange with no special-cased flags.
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
    let (mut rd, mut wr) = tokio::io::split(stream);

    match frame::read_frame(&mut rd).await? {
        Frame::Ping(_) => {
            frame::write_frame(&mut wr, &Frame::Pong(Pong {})).await?;
            frame::write_frame(&mut wr, &Frame::Bye(Bye {})).await?;
            wr.flush()
                .await
                .map_err(|e| AppError::Io(format!("session: ping flush: {e}")))?;
            Ok(())
        }
        Frame::Hello(peer_hello) => {
            verify_hello(&peer_hello, &peer.device_id)?;
            frame::write_frame(
                &mut wr,
                &Frame::Hello(Hello {
                    protocol_version: PROTOCOL_VERSION,
                    device_id: local_device_id.clone(),
                }),
            )
            .await?;
            run_session(rd, wr, db, app, local_device_id, peer).await
        }
        other => Err(AppError::Internal(format!(
            "session: expected Hello or Ping, got {:?}",
            std::mem::discriminant(&other)
        ))),
    }
}

/// Handle an outbound mTLS session (called by the dialer in `sync/client.rs`).
/// Writes our Hello, reads the server's Hello, verifies, then runs the
/// session starting from the cursor exchange.
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
    let (mut rd, mut wr) = tokio::io::split(stream);

    frame::write_frame(
        &mut wr,
        &Frame::Hello(Hello {
            protocol_version: PROTOCOL_VERSION,
            device_id: local_device_id.clone(),
        }),
    )
    .await?;
    match frame::read_frame(&mut rd).await? {
        Frame::Hello(peer_hello) => verify_hello(&peer_hello, &peer.device_id)?,
        other => {
            return Err(AppError::Internal(format!(
                "session: expected Hello, got {:?}",
                std::mem::discriminant(&other)
            )));
        }
    }

    run_session(rd, wr, db, app, local_device_id, peer).await
}

fn verify_hello(hello: &Hello, expected_device_id: &str) -> Result<(), AppError> {
    if hello.protocol_version != PROTOCOL_VERSION {
        return Err(AppError::Internal(format!(
            "session: protocol version mismatch: ours={PROTOCOL_VERSION} peer={}",
            hello.protocol_version
        )));
    }
    if hello.device_id != expected_device_id {
        return Err(AppError::Internal(format!(
            "session: Hello device_id {} doesn't match expected {}",
            hello.device_id, expected_device_id
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Core session
// ---------------------------------------------------------------------------

/// Run the full delta-exchange + model-sync protocol over a split async
/// stream. The Hello exchange is assumed complete by the caller.
/// Send and receive halves run concurrently. Protocol order:
///
///   Cursors ↔ Cursors
///   ChangeBatch* (from us to peer, per peer's cursors — includes
///               `model_versions` INSERT/DELETE rows since training or
///               deletion mutations)
///   ↕ simultaneous with peer shipping their batches to us
///   ModelVersionSummary ↔ ModelVersionSummary
///                          (peer versions list: which `model_versions`
///                          rows the peer has files for locally;
///                          canonical MD5s are in the table already)
///   ModelData* (one frame per version we have files for that the peer
///               doesn't have files for — `apply_model` does the
///               MD5/canonical verification on the receiver side)
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
    let (cursors_tx, cursors_rx) = oneshot::channel::<Cursors>();
    let (model_versions_tx, model_versions_rx) = oneshot::channel::<ModelVersionSummary>();
    let (applied_cursors_tx, applied_cursors_rx) = oneshot::channel::<AppliedCursors>();

    let db_recv = db.clone();
    let app_recv = app.clone();
    let peer_recv = peer.clone();

    let send_fut = send_half(
        write_half,
        db.clone(),
        app.clone(),
        local_device_id.clone(),
        peer.clone(),
        cursors_rx,
        model_versions_rx,
        applied_cursors_rx,
    );
    let recv_fut = recv_half(
        read_half,
        db_recv,
        app_recv,
        local_device_id,
        peer_recv,
        cursors_tx,
        model_versions_tx,
        applied_cursors_tx,
    );

    let (send_res, recv_res) = tokio::join!(send_fut, recv_fut);

    if let Err(e) = &recv_res {
        eprintln!("session: recv half failed: {e}");
    }
    if let Err(e) = &send_res {
        eprintln!("session: send half failed: {e}");
    }

    recv_res?;
    send_res?;

    // Step 30.4: no eviction tombstone to clean up. A revoked peer
    // simply has no space_devices row to begin with (the change_log
    // DELETE row IS the durable record of revocation).

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
    cursors_rx: oneshot::Receiver<Cursors>,
    model_versions_rx: oneshot::Receiver<ModelVersionSummary>,
    applied_cursors_rx: oneshot::Receiver<AppliedCursors>,
) -> Result<(), AppError>
where
    W: AsyncWrite + Unpin,
{
    // --- Cursor exchange ---
    let mut cursor_entries = Vec::new();
    for space_id in peer.outbound_spaces() {
        let devices = sqlx::query_file!("queries/sync/list_space_devices.sql", space_id)
            .fetch_all(&db)
            .await
            .map_err(|e| AppError::Db(format!("session cursors query: {e}")))?;

        for d in devices {
            if d.device_id != local_device_id {
                let last_seq = cursors::get(&db, space_id, &d.device_id).await?;
                cursor_entries.push(CursorEntry {
                    space_id: space_id.to_string(),
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
        if !peer.outbound_contains(&entry.space_id) {
            continue;
        }
        ship_batches(
            &mut wr,
            &db,
            &app,
            &entry.space_id,
            &entry.device_id,
            entry.last_seq,
            &local_device_id,
        )
        .await?;
    }
    let mentioned: std::collections::HashSet<_> = peer_cursors
        .entries
        .iter()
        .map(|e| e.space_id.as_str())
        .collect();
    for space_id in peer.outbound_spaces() {
        if !mentioned.contains(space_id) {
            let devices = sqlx::query_file!("queries/sync/list_space_devices.sql", space_id)
                .fetch_all(&db)
                .await
                .map_err(|e| AppError::Db(format!("session fallback devices query: {e}")))?;

            for d in devices {
                if d.device_id != peer.device_id {
                    ship_batches(
                        &mut wr,
                        &db,
                        &app,
                        space_id,
                        &d.device_id,
                        0,
                        &local_device_id,
                    )
                    .await?;
                }
            }
            ship_batches(
                &mut wr,
                &db,
                &app,
                space_id,
                &local_device_id,
                0,
                &local_device_id,
            )
            .await?;
        }
    }

    // --- ChangesDone + AppliedCursors barrier ---
    write_frame(&mut wr, &Frame::ChangesDone(ChangesDone {})).await?;
    let applied = applied_cursors_rx.await.map_err(|_| {
        AppError::Internal("session: recv half dropped before sending AppliedCursors".into())
    })?;
    write_frame(&mut wr, &Frame::AppliedCursors(applied)).await?;

    // --- Model version exchange ---
    // Sweep orphan files for each shared space BEFORE building the
    // summary — change_log DELETE on `model_versions` may have
    // propagated since the previous session, and the local files
    // matching that deletion should be removed now so the summary
    // doesn't claim we still have them.
    let outbound_owned: Vec<String> = peer.outbound_spaces().map(|s| s.to_string()).collect();
    for space_id in &outbound_owned {
        if let Err(e) = model_sync::gc_orphan_files(&db, &app, space_id).await {
            eprintln!("session: gc_orphan_files {space_id}: {e}");
        }
    }

    let local_summary = model_sync::local_summary(&db, &app, &outbound_owned).await;
    write_frame(&mut wr, &Frame::ModelVersionSummary(local_summary)).await?;

    let peer_summary = model_versions_rx.await.map_err(|_| {
        AppError::Internal("session: recv half dropped before sending ModelVersionSummary".into())
    })?;

    // Push every model version the peer is missing. The per-version
    // MD5s live in `model_versions` (synced via change_log), so this
    // loop just compares version sets: any local version whose number
    // isn't in the peer's `versions` list is in flight or hasn't been
    // seeded yet, so we ship it. MD5 verification — both transfer
    // integrity and canonical disagreement — happens in
    // `model_sync::apply_model` on the receiver side.
    for peer_entry in &peer_summary.entries {
        if !peer.outbound_contains(&peer_entry.space_id) {
            continue;
        }
        let peer_versions: std::collections::HashSet<u32> =
            peer_entry.versions.iter().copied().collect();
        let local_versions =
            model_sync::local_versions_with_files(&db, &app, &peer_entry.space_id).await;
        for v in local_versions {
            if peer_versions.contains(&v) {
                continue;
            }
            match model_sync::read_model(&app, &peer_entry.space_id, v)? {
                Some(data) => {
                    write_frame(&mut wr, &Frame::ModelData(data)).await?;
                }
                None => {
                    eprintln!(
                        "session: model v{v} for space {} missing on disk, skipping",
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

/// Ship change_log rows for `(space_id, device_id)` with seq >
/// peer_last_seq in batches of `DEFAULT_BATCH_LIMIT`. `transport_device_id`
/// is the local device's ID — stamped on every batch so the receiver
/// can verify it against the TLS peer identity.
async fn ship_batches<W>(
    wr: &mut W,
    db: &SqlitePool,
    app: &AppHandle,
    space_id: &str,
    device_id: &str,
    peer_last_seq: i64,
    transport_device_id: &str,
) -> Result<(), AppError>
where
    W: AsyncWrite + Unpin,
{
    let gap = sqlx::query_file!("queries/sync/get_origin_gap_state.sql", space_id, device_id)
        .fetch_one(db)
        .await
        .map_err(|e| AppError::Db(format!("get_origin_gap_state: {e}")))?;

    let v2_required = sqlx::query_file!("queries/sync/get_v2_reconciliation.sql", space_id)
        .fetch_one(db)
        .await
        .map_err(|e| AppError::Db(format!("get_v2_reconciliation: {e}")))?
        .required;

    let effective_high_water = gap.high_water.max(gap.live_max_seq);
    let effective_floor = if gap.live_min_seq > 0 {
        gap.live_min_seq
    } else {
        gap.retained_floor
    };

    let needs_reconciliation = v2_required == 1
        || (effective_high_water > 0 && gap.live_max_seq == 0)
        || (peer_last_seq > 0 && peer_last_seq < effective_floor)
        || (peer_last_seq > effective_high_water);

    if needs_reconciliation {
        eprintln!(
            "session: reconciliation required for space {space_id} device {device_id} \
             (v2_required={v2_required}, high_water={}, live_max={}, floor={}, peer_cursor={peer_last_seq})",
            effective_high_water, gap.live_max_seq, effective_floor
        );
        stream_space_snapshot(wr, db, app, space_id, device_id).await?;
        write_frame(wr, &Frame::SnapshotEnd).await?;
        write_frame(
            wr,
            &Frame::Batch(ChangeBatch {
                space_id: space_id.to_string(),
                origin_device_id: device_id.to_string(),
                transport_device_id: transport_device_id.to_string(),
                rows: vec![],
                final_seq: effective_high_water,
            }),
        )
        .await?;
        return Ok(());
    }

    let final_seq = gap.live_max_seq;
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
                origin_device_id: device_id.to_string(),
                transport_device_id: transport_device_id.to_string(),
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

/// Stream every row of every synced table for `space_id` as a sequence
/// of `Frame::Snapshot(SnapshotChunk)` frames. The first frame carries
/// the space row + members + users + the host's trusted_device entry;
/// subsequent frames carry table data in 500-row chunks. Each frame is
/// self-contained and applied independently by the joiner.
async fn stream_space_snapshot<W>(
    wr: &mut W,
    db: &SqlitePool,
    app: &AppHandle,
    space_id: &str,
    device_id: &str,
) -> Result<(), AppError>
where
    W: AsyncWrite + Unpin,
{
    let identity = crate::sync::identity::ensure_identity(db).await?;
    let display_name = gethostname::gethostname().to_string_lossy().into_owned();
    let snapshot = crate::sync::snapshot::collect_snapshot(
        db,
        Some(app),
        space_id,
        identity.device_id.clone(),
        display_name,
        identity.cert_pem.clone(),
    )
    .await?;

    let space_id_owned = snapshot.space.id.clone();

    // First chunk: space + users + members (the seed data needed
    // before any of the dependent tables can be inserted). Users must
    // precede members because `space_members.user_id` has a FK to
    // `users(id)`.
    write_frame(
        wr,
        &Frame::Snapshot(crate::sync::wire::SnapshotChunk {
            space_id: space_id_owned.clone(),
            frame: crate::sync::snapshot::SnapshotFrame::Space(snapshot.space.clone()),
        }),
    )
    .await?;
    write_frame(
        wr,
        &Frame::Snapshot(crate::sync::wire::SnapshotChunk {
            space_id: space_id_owned.clone(),
            frame: crate::sync::snapshot::SnapshotFrame::Users(snapshot.users.clone()),
        }),
    )
    .await?;
    write_frame(
        wr,
        &Frame::Snapshot(crate::sync::wire::SnapshotChunk {
            space_id: space_id_owned.clone(),
            frame: crate::sync::snapshot::SnapshotFrame::Members(snapshot.members.clone()),
        }),
    )
    .await?;

    stream_chunked(wr, &space_id_owned, snapshot.devices, |chunk| {
        crate::sync::snapshot::SnapshotFrame::Devices(chunk)
    })
    .await?;
    stream_chunked(wr, &space_id_owned, snapshot.space_devices, |chunk| {
        crate::sync::snapshot::SnapshotFrame::SpaceDevices(chunk)
    })
    .await?;
    stream_chunked(wr, &space_id_owned, snapshot.device_user_grants, |chunk| {
        crate::sync::snapshot::SnapshotFrame::DeviceUserGrants(chunk)
    })
    .await?;
    stream_chunked(wr, &space_id_owned, snapshot.categories, |chunk| {
        crate::sync::snapshot::SnapshotFrame::Categories(chunk)
    })
    .await?;
    stream_chunked(wr, &space_id_owned, snapshot.accounts, |chunk| {
        crate::sync::snapshot::SnapshotFrame::Accounts(chunk)
    })
    .await?;
    stream_chunked(wr, &space_id_owned, snapshot.transactions, |chunk| {
        crate::sync::snapshot::SnapshotFrame::Transactions(chunk)
    })
    .await?;
    stream_chunked(wr, &space_id_owned, snapshot.account_summaries, |chunk| {
        crate::sync::snapshot::SnapshotFrame::AccountSummaries(chunk)
    })
    .await?;
    stream_chunked(wr, &space_id_owned, snapshot.model_versions, |chunk| {
        crate::sync::snapshot::SnapshotFrame::ModelVersions(chunk)
    })
    .await?;
    stream_chunked(wr, &space_id_owned, snapshot.space_settings, |chunk| {
        crate::sync::snapshot::SnapshotFrame::SpaceSettings(chunk)
    })
    .await?;
    stream_chunked(wr, &space_id_owned, snapshot.high_water_vector, |chunk| {
        crate::sync::snapshot::SnapshotFrame::HighWaterVector(chunk)
    })
    .await?;
    stream_chunked(wr, &space_id_owned, snapshot.row_winners, |chunk| {
        crate::sync::snapshot::SnapshotFrame::RowWinners(chunk)
    })
    .await?;

    let _ = device_id; // included in identity above
    Ok(())
}

const SNAPSHOT_CHUNK_SIZE: usize = 500;

async fn stream_chunked<W, T, F>(
    wr: &mut W,
    space_id: &str,
    items: Vec<T>,
    wrap: F,
) -> Result<(), AppError>
where
    W: AsyncWrite + Unpin,
    T: Clone,
    F: Fn(Vec<T>) -> crate::sync::snapshot::SnapshotFrame,
{
    for chunk in items.chunks(SNAPSHOT_CHUNK_SIZE) {
        let owned: Vec<T> = chunk.to_vec();
        write_frame(
            wr,
            &Frame::Snapshot(crate::sync::wire::SnapshotChunk {
                space_id: space_id.to_string(),
                frame: wrap(owned),
            }),
        )
        .await?;
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
    cursors_tx: oneshot::Sender<Cursors>,
    model_versions_tx: oneshot::Sender<ModelVersionSummary>,
    applied_cursors_tx: oneshot::Sender<AppliedCursors>,
) -> Result<(), AppError>
where
    R: AsyncRead + Unpin,
{
    let _ = &local_device_id; // used by apply_batch (not called in this path yet)

    // Cursors
    let peer_cursors = expect_cursors(&mut rd).await?;
    cursors_tx.send(peer_cursors).map_err(|_| {
        AppError::Internal("session: send half dropped before receiving Cursors".into())
    })?;

    // Frame loop: Batch* (staged) then ChangesDone (applies round)
    // then AppliedCursors (proof) then ModelVersionSummary then
    // ModelData* then Bye. Snapshots are handled inline as before.
    let mut peer_model_summary_sent = false;
    let mut model_versions_tx = Some(model_versions_tx);
    let mut applied_cursors_tx = Some(applied_cursors_tx);
    let mut staged_batches: Vec<ChangeBatch> = Vec::new();
    let mut snapshot_buf: Vec<crate::sync::wire::SnapshotChunk> = Vec::new();
    let mut snapshot_space: Option<String> = None;
    let mut snapshot_host: Option<crate::sync::snapshot::SpaceSnapshot> = None;
    loop {
        let frame = crate::sync::frame::read_frame(&mut rd).await?;
        match frame {
            Frame::Batch(batch) => {
                staged_batches.push(batch);
            }
            Frame::ChangesDone(_) => {
                let entries = apply_round_core(&db, &peer.device_id, &staged_batches).await?;
                let rows_count = staged_batches.iter().map(|b| b.rows.len()).sum::<usize>();
                let spaces: std::collections::HashSet<&str> =
                    staged_batches.iter().map(|b| b.space_id.as_str()).collect();
                for space_id in &spaces {
                    let _ = app.emit(
                        "sync://applied",
                        SyncAppliedPayload {
                            space_id: space_id.to_string(),
                            rows: rows_count as u64,
                            snapshot: false,
                        },
                    );
                }
                staged_batches.clear();
                if let Some(tx) = applied_cursors_tx.take() {
                    tx.send(AppliedCursors { entries }).map_err(|_| {
                        AppError::Internal(
                            "session: send half dropped before receiving AppliedCursors".into(),
                        )
                    })?;
                }
            }
            Frame::AppliedCursors(cursors) => {
                store_peer_acks(&db, &peer.device_id, &cursors.entries).await?;
            }
            Frame::Snapshot(chunk) => {
                if snapshot_space.is_none() {
                    snapshot_space = Some(chunk.space_id.clone());
                    // Fetch host identity lazily — we'll create a
                    // SpaceSnapshot skeleton when SnapshotEnd arrives.
                    let identity = crate::sync::identity::ensure_identity(&db).await?;
                    let display_name = gethostname::gethostname().to_string_lossy().into_owned();
                    snapshot_host = Some(crate::sync::snapshot::SpaceSnapshot {
                        snapshot_schema_version: crate::sync::snapshot::SNAPSHOT_SCHEMA_VERSION,
                        protocol_version: crate::sync::wire::PROTOCOL_VERSION,
                        snapshot_id: String::new(),
                        host_device_id: identity.device_id.clone(),
                        host_device_name: display_name,
                        host_cert_pem: identity.cert_pem.clone(),
                        high_water_vector: vec![],
                        row_winners: vec![],
                        space: crate::sync::snapshot::WireSpace {
                            id: String::new(),
                            name: String::new(),
                            created_at: String::new(),
                            updated_at: String::new(),
                        },
                        members: vec![],
                        users: vec![],
                        categories: vec![],
                        accounts: vec![],
                        transactions: vec![],
                        account_summaries: vec![],
                        space_settings: vec![],
                        model_versions: vec![],
                        devices: vec![],
                        space_devices: vec![],
                        device_user_grants: vec![],
                    });
                }
                snapshot_buf.push(chunk);
            }
            Frame::SnapshotEnd => {
                if let (Some(space_id), Some(host)) = (snapshot_space.take(), snapshot_host.take())
                {
                    apply_snapshot_stream(&db, host, &snapshot_buf).await?;
                    let _ = app.emit(
                        "sync://applied",
                        SyncAppliedPayload {
                            space_id,
                            rows: snapshot_buf.len() as u64,
                            snapshot: true,
                        },
                    );
                }
                snapshot_buf.clear();
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
                if peer.inbound_contains(&data.space_id)
                    && let Err(e) = model_sync::apply_model(&app, &db, &data).await
                {
                    eprintln!(
                        "session: apply model v{} for space {}: {e}",
                        data.version, data.space_id
                    );
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

/// Apply a buffered snapshot stream under `apply_guard` so the host
/// device (not the local device) is stamped as the author of every
/// change_log row. The body collapses the chunk buffer back into a
/// `SpaceSnapshot` and reuses `snapshot::apply_snapshot_frame`.
///
/// Step 31.2: after the apply transaction commits, install the
/// complete cursor vector in a separate transaction so subsequent
/// incremental syncs pick up where the snapshot left off (instead of
/// re-shipping the entire backlog from seq=1).
async fn apply_snapshot_stream(
    db: &SqlitePool,
    snapshot: crate::sync::snapshot::SpaceSnapshot,
    chunks: &[crate::sync::wire::SnapshotChunk],
) -> Result<(), AppError> {
    // Step 31.2: verify the snapshot boundary BEFORE opening the apply
    // transaction. A mixed-space or unsupported-version snapshot must
    // be rejected outright — partial apply would leak hostile state.
    let expected_space_id = snapshot.space.id.clone();
    crate::sync::snapshot::verify_snapshot_boundary(
        &expected_space_id,
        snapshot.snapshot_schema_version,
        chunks,
    )?;

    // Apply each frame independently so a mid-stream failure aborts the
    // whole batch (apply_guard transaction rolls back).
    //
    // The snapshot skeleton is built from `collect_snapshot` (session
    // path) or the pair-header (pairing path), so `snapshot.space.id`
    // is populated from the start — the End frame's trusted_device
    // upsert can find the space without needing to lift it from a
    // later chunk.
    let host_device_id = snapshot.host_device_id.clone();
    let chunks_owned: Vec<_> = chunks.to_vec();
    let snapshot_for_apply = snapshot.clone();
    crate::sync::apply_guard::run_as_device(db, &host_device_id, move |tx| {
        Box::pin(async move {
            for chunk in &chunks_owned {
                crate::sync::snapshot::apply_snapshot_frame(tx, &snapshot_for_apply, &chunk.frame)
                    .await?;
            }
            Ok(())
        })
    })
    .await
    .map_err(|e| AppError::Db(format!("apply_snapshot_stream: {e}")))?;

    // Step 31.2: install cursor vector AFTER commit. Separate tx so a
    // crash here is detectable and the next sync can safely re-snapshot.
    crate::sync::snapshot::install_snapshot_cursors(
        db,
        &expected_space_id,
        &snapshot.high_water_vector,
        &snapshot.host_device_id,
    )
    .await
    .map_err(|e| AppError::Db(format!("apply_snapshot_stream cursors: {e}")))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Batch validation — Step 29.4
// ---------------------------------------------------------------------------

/// Validate a `ChangeBatch` before any writes. Rejects atomically:
/// if any row fails, the entire batch is refused without side effects.
///
/// Check 1 (transport device trusted for the space) is handled by the
/// caller via `PeerIdentity::inbound_contains` — this function covers
/// checks 2–7.
pub(crate) fn validate_batch(batch: &ChangeBatch) -> Result<(), AppError> {
    let batch_space = &batch.space_id;
    let batch_origin = &batch.origin_device_id;
    let mut prev_seq: i64 = 0;

    for row in &batch.rows {
        if row.device_id != *batch_origin {
            return Err(AppError::InvalidInput(format!(
                "validate_batch: row origin {} != batch origin {}",
                row.device_id, batch_origin
            )));
        }

        if row.space_id != *batch_space {
            return Err(AppError::InvalidInput(format!(
                "validate_batch: row space {} != batch space {}",
                row.space_id, batch_space
            )));
        }

        match row.operation.as_str() {
            "insert" | "update" => {
                let payload = row.payload.as_ref().ok_or_else(|| {
                    AppError::InvalidInput(format!(
                        "validate_batch: missing payload for {}/{}",
                        row.table_name, row.operation
                    ))
                })?;
                if payload.as_table_name() != row.table_name {
                    return Err(AppError::InvalidInput(format!(
                        "validate_batch: payload variant {} != table_name {}",
                        payload.as_table_name(),
                        row.table_name
                    )));
                }
                validate_payload_keys(row, payload)?;
            }
            "delete" => {
                validate_delete_key(row, batch_space)?;
            }
            other => {
                return Err(AppError::InvalidInput(format!(
                    "validate_batch: unknown operation {other:?}"
                )));
            }
        }

        if row.seq <= 0 {
            return Err(AppError::InvalidInput(format!(
                "validate_batch: non-positive seq {}",
                row.seq
            )));
        }
        if row.seq <= prev_seq {
            return Err(AppError::InvalidInput(format!(
                "validate_batch: seq {} not strictly monotonic (prev {})",
                row.seq, prev_seq
            )));
        }
        prev_seq = row.seq;
    }

    Ok(())
}

/// Verify that the payload's logical keys match the row envelope
/// (`row_id` and `space_id`). Each table has its own key shape.
fn validate_payload_keys(row: &ChangeRow, payload: &TablePayload) -> Result<(), AppError> {
    fn mismatch(field: &str, expected: &str, actual: &str) -> AppError {
        AppError::InvalidInput(format!(
            "validate_batch: payload key mismatch: {field}: expected {expected:?}, got {actual:?}"
        ))
    }

    match payload {
        TablePayload::Space(p) => {
            if p.id != row.row_id {
                return Err(mismatch("space.id", &p.id, &row.row_id));
            }
            if p.id != row.space_id {
                return Err(mismatch("space.id/space_id", &p.id, &row.space_id));
            }
        }
        TablePayload::SpaceMember(p) => {
            if p.space_id != row.space_id {
                return Err(mismatch("member.space_id", &p.space_id, &row.space_id));
            }
            let expected = format!("{}:{}", p.space_id, p.user_id);
            if expected != row.row_id {
                return Err(mismatch("member.row_id", &expected, &row.row_id));
            }
        }
        TablePayload::Account(p) => {
            if p.id != row.row_id {
                return Err(mismatch("account.id", &p.id, &row.row_id));
            }
            if p.space_id != row.space_id {
                return Err(mismatch("account.space_id", &p.space_id, &row.space_id));
            }
        }
        TablePayload::Category(p) => {
            if p.id != row.row_id {
                return Err(mismatch("category.id", &p.id, &row.row_id));
            }
            if p.space_id != row.space_id {
                return Err(mismatch("category.space_id", &p.space_id, &row.space_id));
            }
        }
        TablePayload::Transaction(p) => {
            if p.id != row.row_id {
                return Err(mismatch("transaction.id", &p.id, &row.row_id));
            }
        }
        TablePayload::AccountSummary(p) => {
            let expected = format!("{}:{}", p.account_id, p.month);
            if expected != row.row_id {
                return Err(mismatch("summary.row_id", &expected, &row.row_id));
            }
        }
        TablePayload::SpaceSetting(p) => {
            if p.space_id != row.space_id {
                return Err(mismatch("setting.space_id", &p.space_id, &row.space_id));
            }
            let expected = format!("{}:{}", p.space_id, p.key);
            if expected != row.row_id {
                return Err(mismatch("setting.row_id", &expected, &row.row_id));
            }
        }
        TablePayload::SpaceDevice(p) => {
            if p.space_id != row.space_id {
                return Err(mismatch("device.space_id", &p.space_id, &row.space_id));
            }
            let expected = format!("{}:{}", p.space_id, p.device_id);
            if expected != row.row_id {
                return Err(mismatch("device.row_id", &expected, &row.row_id));
            }
        }
        TablePayload::DeviceUserGrant(p) => {
            if p.space_id != row.space_id {
                return Err(mismatch(
                    "device_user_grant.space_id",
                    &p.space_id,
                    &row.space_id,
                ));
            }
            let expected = format!("{}:{}:{}", p.space_id, p.device_id, p.user_id);
            if expected != row.row_id {
                return Err(mismatch("device_user_grant.row_id", &expected, &row.row_id));
            }
        }
        TablePayload::ModelVersion(p) => {
            if p.space_id != row.space_id {
                return Err(mismatch("model.space_id", &p.space_id, &row.space_id));
            }
            let expected = format!("{}:{}", p.space_id, p.version);
            if expected != row.row_id {
                return Err(mismatch("model.row_id", &expected, &row.row_id));
            }
        }
    }
    Ok(())
}

/// Verify composite delete keys are well-formed and scoped to the
/// outer space. Single-PK tables (accounts, categories, transactions)
/// need no composite-key check.
fn validate_delete_key(row: &ChangeRow, batch_space_id: &str) -> Result<(), AppError> {
    match row.table_name.as_str() {
        "spaces" => {
            if row.row_id != row.space_id {
                return Err(AppError::InvalidInput(format!(
                    "validate_batch: spaces delete row_id {} != space_id {}",
                    row.row_id, row.space_id
                )));
            }
        }
        "space_members" | "space_settings" | "model_versions" | "space_devices" => {
            let parts: Vec<&str> = row.row_id.splitn(2, ':').collect();
            if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
                return Err(AppError::InvalidInput(format!(
                    "validate_batch: invalid composite delete key {:?} for {}",
                    row.row_id, row.table_name
                )));
            }
            if parts[0] != batch_space_id {
                return Err(AppError::InvalidInput(format!(
                    "validate_batch: delete key space {} != batch space {}",
                    parts[0], batch_space_id
                )));
            }
        }
        "device_user_grants" => {
            let parts: Vec<&str> = row.row_id.split(':').collect();
            if parts.len() != 3 || parts.iter().any(|part| part.is_empty()) {
                return Err(AppError::InvalidInput(format!(
                    "validate_batch: invalid composite delete key {:?} for device_user_grants",
                    row.row_id
                )));
            }
            if parts[0] != batch_space_id {
                return Err(AppError::InvalidInput(format!(
                    "validate_batch: delete key space {} != batch space {}",
                    parts[0], batch_space_id
                )));
            }
        }
        "account_summaries" => {
            let parts: Vec<&str> = row.row_id.splitn(2, ':').collect();
            if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
                return Err(AppError::InvalidInput(format!(
                    "validate_batch: invalid composite delete key {:?} for account_summaries",
                    row.row_id
                )));
            }
        }
        _ => {}
    }
    Ok(())
}

/// Apply one remote change_log row with Lamport revision tracking.
///
/// 1. Raises the local sequence clock to at least the incoming seq.
/// 2. Inserts the change_log row (for relay) with its original origin.
/// 3. Compares `(origin_seq, origin_device_id)` against the current
///    winner. A losing revision is skipped — the row body is not
///    materialized.
/// 4. If the incoming revision wins, upserts `row_winners` and
///    materializes the row body via `apply_change`.
///
/// Must be called inside `run_as_device` so triggers are suppressed.
pub(crate) async fn apply_remote_row(
    tx: &mut Transaction<'_, Sqlite>,
    row: &ChangeRow,
) -> Result<(), AppError> {
    sqlx::query_file!("queries/sync/raise_sync_seq.sql", row.seq)
        .execute(&mut **tx)
        .await
        .map_err(|e| AppError::Db(format!("raise_sync_seq: {e}")))?;

    let payload_json = row.payload.as_ref().and_then(|p| p.to_json().ok());
    sqlx::query_file!(
        "queries/sync/insert_remote_change_log.sql",
        row.id,
        row.space_id,
        row.table_name,
        row.row_id,
        row.operation,
        payload_json,
        row.seq,
        row.device_id,
        row.device_id,
        row.seq,
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Db(format!("insert_remote_change_log: {e}")))?;

    let existing = sqlx::query_file!(
        "queries/sync/get_row_winner.sql",
        row.space_id,
        row.table_name,
        row.row_id
    )
    .fetch_optional(&mut **tx)
    .await
    .map_err(|e| AppError::Db(format!("get_row_winner: {e}")))?;

    let incoming_wins = match existing {
        None => true,
        Some(e) => {
            row.seq > e.winning_seq
                || (row.seq == e.winning_seq && row.device_id > e.winning_origin)
        }
    };

    if !incoming_wins {
        return Ok(());
    }

    let is_delete: i64 = if row.operation == "delete" { 1 } else { 0 };
    sqlx::query_file!(
        "queries/sync/upsert_row_winner.sql",
        row.space_id,
        row.table_name,
        row.row_id,
        row.seq,
        row.device_id,
        is_delete,
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Db(format!("upsert_row_winner: {e}")))?;

    apply::apply_change(tx, row).await
}

// ---------------------------------------------------------------------------
// Round-based apply — Step 29.5
// ---------------------------------------------------------------------------

/// Foreign-key-safe table ordering. Tables with lower priority are
/// applied first so that child rows (e.g. transactions) never
/// reference a parent (e.g. account) that hasn't been inserted yet.
fn table_priority(table_name: &str) -> u8 {
    match table_name {
        "spaces" => 0,
        "accounts" | "categories" | "space_settings" | "space_devices" | "model_versions" => 1,
        "space_members" => 2,
        "device_user_grants" => 3,
        "transactions" | "account_summaries" => 4,
        _ => 4,
    }
}

/// Apply a complete sync round: validate all batches, flatten rows,
/// sort by foreign-key-safe order, apply inside one `run_as_device`
/// transaction, and advance all cursors. Returns the cursor entries
/// that should be reported back to the sender via `AppliedCursors`.
///
/// This is the core logic shared by the production recv half and the
/// test harness. The production wrapper adds transport-identity
/// verification and event emission.
pub(crate) async fn apply_round_core(
    db: &SqlitePool,
    transport_device_id: &str,
    batches: &[ChangeBatch],
) -> Result<Vec<CursorEntry>, AppError> {
    for batch in batches {
        validate_batch(batch)?;
    }

    let mut all_rows: Vec<ChangeRow> = batches
        .iter()
        .flat_map(|b| b.rows.iter().cloned())
        .collect();
    all_rows.sort_by_key(|r| (table_priority(&r.table_name), r.seq));

    let cursor_entries: Vec<CursorEntry> = batches
        .iter()
        .map(|b| CursorEntry {
            space_id: b.space_id.clone(),
            device_id: b.origin_device_id.clone(),
            last_seq: b.final_seq,
        })
        .collect();

    let rows = all_rows;
    let cursors = cursor_entries.clone();
    run_as_device(db, transport_device_id, move |tx| {
        let rows = rows.clone();
        let cursors = cursors.clone();
        Box::pin(async move {
            for row in &rows {
                apply_remote_row(tx, row).await?;
            }
            for c in &cursors {
                cursors::advance(tx, &c.space_id, &c.device_id, c.last_seq).await?;
            }
            Ok::<(), AppError>(())
        })
    })
    .await
    .map_err(|e| AppError::Db(format!("apply_round_core: {e}")))?;

    Ok(cursor_entries)
}

/// Persist a peer's `AppliedCursors` entries as real acknowledgments.
/// Each entry says "peer `consuming_device_id` consumed changes from
/// `origin_device_id` up to `last_seq`." This is the only source of
/// truth for what a peer has acknowledged — never infer it from local
/// receive cursors. See data/PLAN.md Step 29.6.
pub(crate) async fn store_peer_acks(
    db: &SqlitePool,
    peer_device_id: &str,
    entries: &[CursorEntry],
) -> Result<(), AppError> {
    for entry in entries {
        sqlx::query_file!(
            "queries/sync/upsert_peer_ack.sql",
            entry.space_id,
            peer_device_id,
            entry.device_id,
            entry.last_seq,
        )
        .execute(db)
        .await
        .map_err(|e| AppError::Db(format!("store_peer_acks: {e}")))?;
    }
    Ok(())
}

/// Apply one `ChangeBatch` atomically with its cursor advance, and emit
/// `sync://evicted` if this device's own trusted_devices row was deleted.
#[allow(dead_code)]
async fn apply_batch(
    db: &SqlitePool,
    local_device_id: &str,
    peer: &PeerIdentity,
    batch: ChangeBatch,
    app: &AppHandle,
) -> Result<(), AppError> {
    if !peer.inbound_contains(&batch.space_id) {
        return Ok(());
    }

    validate_batch(&batch)?;

    let kind = classify_batch(&batch.rows, local_device_id);
    let is_space_deletion = matches!(kind, BatchKind::SpaceDeletion);
    let evicted = matches!(kind, BatchKind::PotentialEviction);

    let space_id = batch.space_id.clone();
    let evicted_space_id = batch.space_id.clone();
    let batch_device_id = batch.origin_device_id.clone();
    let final_seq = batch.final_seq;
    let batch_space_id = batch.space_id.clone();

    if evicted {
        let deleted_device_ids: Vec<String> = batch
            .rows
            .iter()
            .filter(|r| r.table_name == "space_devices" && r.operation == "delete")
            .filter_map(|r| r.row_id.split(':').nth(1).map(String::from))
            .collect();

        let own_rows = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM space_devices WHERE device_id = ?1 \
             AND device_id IN (SELECT value FROM json_each(?2))",
            local_device_id,
            serde_json::to_string(&deleted_device_ids).unwrap_or_default()
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
                        apply_remote_row(tx, row).await?;
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
    let rows_count = batch.rows.len() as u64;
    run_as_device(db, &batch_device_id, move |tx| -> GuardedFuture<'_, ()> {
        let device_id_inner = device_id_for_closure.clone();
        Box::pin(async move {
            for row in &batch.rows {
                apply_remote_row(tx, row).await?;
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

    let _ = app.emit(
        "sync://applied",
        SyncAppliedPayload {
            space_id: space_id.clone(),
            rows: rows_count,
            snapshot: false,
        },
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Batch classification
// ---------------------------------------------------------------------------

/// Classify a change batch to determine if it represents a space deletion
/// (suppresses eviction detection) or a potential device revocation
/// (triggers the eviction DB check). See PLAN.md §9.5.
#[allow(dead_code)]
enum BatchKind {
    SpaceDeletion,
    PotentialEviction,
    Normal,
}

/// A batch containing a `spaces` delete is a space-deletion propagation.
/// We suppress the eviction check for the whole batch in that case, since
/// the `space_devices` deletes that ride along are cascade effects of the
/// space deletion, not an explicit device revocation. The only way both
/// could coexist in one batch is if a space deletion and an unrelated
/// device revocation happened to share the same shipping window —
/// acceptable risk: the revoked device's data is gone either way, and the
/// next batch from the same peer will re-trigger eviction detection.
#[allow(dead_code)]
fn classify_batch(rows: &[ChangeRow], local_device_id: &str) -> BatchKind {
    if rows
        .iter()
        .any(|r| r.table_name == "spaces" && r.operation == "delete")
    {
        return BatchKind::SpaceDeletion;
    }
    if rows.iter().any(|r| {
        r.table_name == "space_devices" && r.operation == "delete" && r.device_id != local_device_id
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
            change_row("space_devices", "delete", "peer-1"),
        ];
        assert!(matches!(
            classify_batch(&rows, "local"),
            BatchKind::SpaceDeletion
        ));
    }

    #[test]
    fn classify_batch_detects_device_revocation() {
        let rows = vec![change_row("space_devices", "delete", "peer-1")];
        assert!(matches!(
            classify_batch(&rows, "local"),
            BatchKind::PotentialEviction
        ));
    }

    /// A space_devices delete authored by us is an echo of our own
    /// change coming back — not an eviction.
    #[test]
    fn classify_batch_ignores_own_device_revocation() {
        let rows = vec![change_row("space_devices", "delete", "local")];
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
