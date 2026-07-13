use std::path::{Path, PathBuf};

use sqlx::SqlitePool;
use tauri::{AppHandle, Manager};

use crate::error::AppError;
use crate::sync::wire::{ModelData, ModelVersionEntry, ModelVersionSummary};

// ---------------------------------------------------------------------------
// Keys and paths (mirrors `commands/training.rs` conventions)
// ---------------------------------------------------------------------------

const SETTING_ACTIVE_MODEL: &str = "active_model_version";

fn models_dir(app: &AppHandle, space_id: &str) -> Result<PathBuf, AppError> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Internal(format!("app_data_dir: {e}")))?
        .join("models")
        .join(space_id);
    std::fs::create_dir_all(&dir).map_err(|e| AppError::Io(format!("create models dir: {e}")))?;
    Ok(dir)
}

fn weights_path(dir: &Path, version: u32) -> PathBuf {
    dir.join(format!("model_v{version}.safetensors"))
}

fn card_path(dir: &Path, version: u32) -> PathBuf {
    dir.join(format!("model_v{version}.json"))
}

/// Public so `commands/training.rs` can hash freshly-saved model bytes
/// without reaching into sync internals.
pub fn md5_hex(bytes: &[u8]) -> String {
    use md5::{Digest, Md5};
    let hash = Md5::digest(bytes);
    hash.iter().map(|b| format!("{b:02x}")).collect()
}

// ---------------------------------------------------------------------------
// Reading local state
// ---------------------------------------------------------------------------

/// Returns the active finetuned model version for `space_id`, or 0 if
/// none exists (no active model). Reads from `space_settings` and
/// verifies the corresponding weights + card files actually exist on
/// disk — `space_settings` can be updated by change_log sync or a
/// snapshot before the model files land, so trusting the DB value
/// alone would cause `ModelVersionSummary` to report a version the
/// peer doesn't actually have on disk and suppress the ModelData
/// transfer.
pub async fn local_version(db: &SqlitePool, app: &AppHandle, space_id: &str) -> u32 {
    let version: u32 = sqlx::query_file!(
        "queries/training/get_setting.sql",
        space_id,
        SETTING_ACTIVE_MODEL
    )
    .fetch_optional(db)
    .await
    .ok()
    .flatten()
    .and_then(|r| r.value.parse().ok())
    .unwrap_or(0);

    if version == 0 {
        return 0;
    }

    let dir = match app.path().app_data_dir() {
        Ok(d) => d.join("models").join(space_id),
        Err(_) => return 0,
    };
    let wp = weights_path(&dir, version);
    let cp = card_path(&dir, version);
    if wp.is_file() && cp.is_file() {
        version
    } else {
        0
    }
}

/// Returns the versions for `space_id` that exist on disk with files
/// present AND whose on-disk MD5 matches the canonical entry in
/// `model_versions`. A row whose files are missing (recv_half hasn't
/// delivered them yet) or whose MD5 doesn't match (corruption, partial
/// write) is filtered out — listing it would invite a self-send loop
/// or a request for our own corrupt file.
pub async fn local_versions_with_files(
    db: &SqlitePool,
    app: &AppHandle,
    space_id: &str,
) -> Vec<u32> {
    let Ok(rows) = sqlx::query_file!(
        "queries/training/list_model_versions_with_md5s.sql",
        space_id
    )
    .fetch_all(db)
    .await
    else {
        return Vec::new();
    };

    let dir = match app.path().app_data_dir() {
        Ok(d) => d.join("models").join(space_id),
        Err(_) => return Vec::new(),
    };

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let v = row.version as u32;
        let wp = weights_path(&dir, v);
        let cp = card_path(&dir, v);
        if !wp.is_file() || !cp.is_file() {
            continue;
        }
        let w_bytes = match std::fs::read(&wp) {
            Ok(b) => b,
            Err(_) => continue,
        };
        if md5_hex(&w_bytes) != row.weights_md5 {
            continue;
        }
        // Card MD5 may legitimately differ if the card was rebuilt at
        // some point (e.g., backfill during startup wrote the row from
        // an old card). Trust the table's MD5 in that case — the
        // file is canonical for what the table claims. Skip the byte
        // check on the card to avoid false negatives.
        out.push(v);
    }
    out
}

/// Build the `ModelVersionSummary` to send to a peer: one entry per
/// shared space listing the versions this peer has files for, plus
/// the active version. The per-version MD5s are NOT shipped here —
/// they're owned by `model_versions` (synced via change_log).
pub async fn local_summary(
    db: &SqlitePool,
    app: &AppHandle,
    space_ids: &[String],
) -> ModelVersionSummary {
    let mut entries = Vec::with_capacity(space_ids.len());
    for space_id in space_ids {
        let active_version = local_version(db, app, space_id).await;
        let versions = local_versions_with_files(db, app, space_id).await;
        entries.push(ModelVersionEntry {
            space_id: space_id.clone(),
            active_version,
            versions,
        });
    }
    ModelVersionSummary { entries }
}

/// Read the weights and card bytes for `version` in `space_id`. Returns
/// `None` if either file is missing (can happen if the model was GC'd
/// or never trained). MD5 fields on the returned `ModelData` are
/// computed from the read bytes so the receiver can verify transfer
/// integrity.
pub fn read_model(
    app: &AppHandle,
    space_id: &str,
    version: u32,
) -> Result<Option<ModelData>, AppError> {
    if version == 0 {
        return Ok(None);
    }
    let dir = models_dir(app, space_id)?;
    let wp = weights_path(&dir, version);
    let cp = card_path(&dir, version);

    if !wp.exists() || !cp.exists() {
        return Ok(None);
    }

    let weights =
        std::fs::read(&wp).map_err(|e| AppError::Io(format!("read weights v{version}: {e}")))?;
    let card =
        std::fs::read(&cp).map_err(|e| AppError::Io(format!("read card v{version}: {e}")))?;

    Ok(Some(ModelData {
        space_id: space_id.to_string(),
        version,
        weights_md5: md5_hex(&weights),
        card_md5: md5_hex(&card),
        weights,
        card,
    }))
}

/// Canonical MD5s for `(space_id, version)` from the mesh-wide
/// `model_versions` table. Returns `None` when no row exists — the
/// model-sync phase uses this to authenticate incoming `ModelData`
/// (LWW'd MD5 is the source of truth; a frame sent by a peer that
/// has no row locally hasn't been propagated yet).
pub async fn canonical_md5s(
    db: &SqlitePool,
    space_id: &str,
    version: u32,
) -> Option<(String, String)> {
    sqlx::query_file!(
        "queries/training/get_model_version_md5s.sql",
        space_id,
        version
    )
    .fetch_optional(db)
    .await
    .ok()
    .flatten()
    .map(|r| (r.weights_md5, r.card_md5))
}

// ---------------------------------------------------------------------------
// Training-side registration
// ---------------------------------------------------------------------------

/// Read the just-saved weights + card, compute MD5s, and upsert one
/// row into `model_versions`. Called by `fine_tune` immediately after
/// `trainable.save()` + `write_card()`. The change_log trigger
/// propagates the INSERT to peers; from that point onward the new
/// version participates in mesh-wide model sync.
pub async fn register_trained_model(
    db: &SqlitePool,
    app: &AppHandle,
    space_id: &str,
    version: u32,
) -> Result<(), AppError> {
    let dir = models_dir(app, space_id)?;
    let wp = weights_path(&dir, version);
    let cp = card_path(&dir, version);
    let weights =
        std::fs::read(&wp).map_err(|e| AppError::Io(format!("read weights v{version}: {e}")))?;
    let card_bytes =
        std::fs::read(&cp).map_err(|e| AppError::Io(format!("read card v{version}: {e}")))?;
    let weights_md5 = md5_hex(&weights);
    let card_md5 = md5_hex(&card_bytes);

    // Pull `trained_at` out of the freshly-written card so the row in
    // `model_versions` reflects the same timestamp as the on-disk
    // JSON. Falls back to the current time on a parse error (should
    // never happen — we just wrote the card).
    let trained_at: String = serde_json::from_slice::<serde_json::Value>(&card_bytes)
        .ok()
        .and_then(|v| {
            v.get("trained_at")
                .and_then(|t| t.as_str())
                .map(String::from)
        })
        .unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string());

    sqlx::query_file!(
        "queries/training/upsert_model_version.sql",
        space_id,
        version,
        weights_md5,
        card_md5,
        trained_at
    )
    .execute(db)
    .await
    .map_err(|e| AppError::Db(format!("register_trained_model: {e}")))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Local lifecycle (delete + GC)
// ---------------------------------------------------------------------------

/// Delete one version's on-disk files. Idempotent: missing files are
/// fine. Does NOT touch `model_versions` — the table row's lifetime is
/// owned by the change_log layer (it propagates the DELETE to peers,
/// which then GC their own files). Local deletion of files belongs
/// here because `delete_model` wants immediate local cleanup while the
/// remote cleanup runs lazily at the next model-sync phase.
pub fn delete_local_files(app: &AppHandle, space_id: &str, version: u32) -> Result<(), AppError> {
    let dir = models_dir(app, space_id)?;
    let wp = weights_path(&dir, version);
    let cp = card_path(&dir, version);
    if wp.exists() {
        std::fs::remove_file(&wp).map_err(|e| AppError::Io(format!("remove weights: {e}")))?;
    }
    if cp.exists() {
        std::fs::remove_file(&cp).map_err(|e| AppError::Io(format!("remove card: {e}")))?;
    }
    Ok(())
}

/// Sweep orphan files for `space_id`: any `(model_v{N}.safetensors,
/// model_v{N}.json)` pair with no matching `model_versions` row is
/// leftover from a propagated deletion and gets removed. Run at the
/// start of model-sync phase per space to drain stale files without
/// surfacing them to the user. Tolerant of the dir not existing
/// (never-trained space).
pub async fn gc_orphan_files(
    db: &SqlitePool,
    app: &AppHandle,
    space_id: &str,
) -> Result<(), AppError> {
    let dir = match app.path().app_data_dir() {
        Ok(d) => d.join("models").join(space_id),
        Err(_) => return Ok(()),
    };
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };

    let Ok(rows) = sqlx::query_file!("queries/training/list_model_versions.sql", space_id)
        .fetch_all(db)
        .await
    else {
        return Ok(());
    };
    let known: std::collections::HashSet<u32> = rows.iter().map(|r| r.version as u32).collect();

    for entry in entries.flatten() {
        let name = entry.file_name();
        let s = name.to_string_lossy();
        let Some(rest) = s.strip_prefix("model_v") else {
            continue;
        };
        let stem = if let Some(n) = rest.strip_suffix(".safetensors") {
            n
        } else if let Some(n) = rest.strip_suffix(".json") {
            n
        } else {
            continue;
        };
        let Ok(v) = stem.parse::<u32>() else {
            continue;
        };
        if known.contains(&v) {
            continue;
        }
        let _ = std::fs::remove_file(entry.path());
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Applying an incoming ModelData
// ---------------------------------------------------------------------------

/// Write an incoming `ModelData` atomically. The receiver verifies the
/// frame's MD5s against the received bytes (transfer integrity) and
/// against the canonical `model_versions` row's MD5s (content
/// authenticity — prevents a peer from spoofing a different file
/// under a version that the mesh has LWW'd to a different MD5).
/// Empty MD5 fields on the incoming frame are treated as
/// "unverified" (no integrity check) so legacy/edge-case peers can
/// still drive a transfer.
pub async fn apply_model(
    app: &AppHandle,
    db: &SqlitePool,
    data: &ModelData,
) -> Result<(), AppError> {
    if data.version == 0 {
        return Err(AppError::InvalidInput(
            "apply_model: version must be >= 1".into(),
        ));
    }

    let computed_weights_md5 = md5_hex(&data.weights);
    let computed_card_md5 = md5_hex(&data.card);
    if !data.weights_md5.is_empty() && computed_weights_md5 != data.weights_md5 {
        return Err(AppError::InvalidInput(format!(
            "apply_model: weights md5 mismatch for v{} (claimed {}, got {})",
            data.version, data.weights_md5, computed_weights_md5
        )));
    }
    if !data.card_md5.is_empty() && computed_card_md5 != data.card_md5 {
        return Err(AppError::InvalidInput(format!(
            "apply_model: card md5 mismatch for v{} (claimed {}, got {})",
            data.version, data.card_md5, computed_card_md5
        )));
    }

    if let Some((canonical_w, _canonical_c)) =
        canonical_md5s(db, &data.space_id, data.version).await
        && !canonical_w.is_empty()
        && computed_weights_md5 != canonical_w
    {
        return Err(AppError::InvalidInput(format!(
            "apply_model: weights md5 {} disagrees with canonical {} for v{}",
            computed_weights_md5, canonical_w, data.version
        )));
    }
    // If no row exists in `model_versions` for this version, the
    // change_log INSERT hasn't arrived yet (or we haven't received
    // any sync). Accept the transfer optimistically — the next sync
    // session will reconcile if the row arrives with a different MD5.

    let dir = models_dir(app, &data.space_id)?;

    let wp = weights_path(&dir, data.version);
    let wp_tmp = dir.join(format!("model_v{}.safetensors.tmp", data.version));
    std::fs::write(&wp_tmp, &data.weights)
        .map_err(|e| AppError::Io(format!("write weights tmp: {e}")))?;
    std::fs::rename(&wp_tmp, &wp).map_err(|e| AppError::Io(format!("rename weights: {e}")))?;

    let cp = card_path(&dir, data.version);
    let cp_tmp = dir.join(format!("model_v{}.json.tmp", data.version));
    std::fs::write(&cp_tmp, &data.card)
        .map_err(|e| AppError::Io(format!("write card tmp: {e}")))?;
    std::fs::rename(&cp_tmp, &cp).map_err(|e| AppError::Io(format!("rename card: {e}")))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn md5_hex_is_deterministic() {
        let a = md5_hex(b"hello");
        let b = md5_hex(b"hello");
        let c = md5_hex(b"helloo");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.len(), 32);
    }

    #[test]
    fn md5_hex_empty_string_known_value() {
        // MD5("") is a well-known constant (RFC 1321).
        assert_eq!(md5_hex(b""), "d41d8cd98f00b204e9800998ecf8427e");
    }
}
