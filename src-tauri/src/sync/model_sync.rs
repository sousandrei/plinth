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

// ---------------------------------------------------------------------------
// Reading local state
// ---------------------------------------------------------------------------

/// Returns the active finetuned model version for `space_id`, or 0 if
/// none exists (base model only). Reads from `space_settings`.
pub async fn local_version(db: &SqlitePool, space_id: &str) -> u32 {
    sqlx::query_file!(
        "queries/training/get_setting.sql",
        space_id,
        SETTING_ACTIVE_MODEL
    )
    .fetch_optional(db)
    .await
    .ok()
    .flatten()
    .and_then(|r| r.value.parse().ok())
    .unwrap_or(0)
}

/// Build the `ModelVersionSummary` to send to a peer: one entry per
/// shared space with the local active version.
pub async fn local_summary(db: &SqlitePool, space_ids: &[String]) -> ModelVersionSummary {
    let mut entries = Vec::with_capacity(space_ids.len());
    for space_id in space_ids {
        let version = local_version(db, space_id).await;
        entries.push(ModelVersionEntry {
            space_id: space_id.clone(),
            version,
        });
    }
    ModelVersionSummary { entries }
}

/// Read the weights and card bytes for `version` in `space_id`. Returns
/// `None` if either file is missing (can happen if the model was GC'd
/// or never trained).
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
        weights,
        card,
    }))
}

// ---------------------------------------------------------------------------
// Applying an incoming ModelData
// ---------------------------------------------------------------------------

/// Write an incoming `ModelData` atomically and advance the active version
/// in `space_settings`. Writes to temp files first, then renames so a
/// crash mid-transfer leaves the previous version intact.
pub async fn apply_model(
    app: &AppHandle,
    db: &SqlitePool,
    data: &ModelData,
) -> Result<(), AppError> {
    let dir = models_dir(app, &data.space_id)?;

    // Write weights atomically: temp → rename.
    let wp = weights_path(&dir, data.version);
    let wp_tmp = dir.join(format!("model_v{}.safetensors.tmp", data.version));
    std::fs::write(&wp_tmp, &data.weights)
        .map_err(|e| AppError::Io(format!("write weights tmp: {e}")))?;
    std::fs::rename(&wp_tmp, &wp).map_err(|e| AppError::Io(format!("rename weights: {e}")))?;

    // Write card atomically: temp → rename.
    let cp = card_path(&dir, data.version);
    let cp_tmp = dir.join(format!("model_v{}.json.tmp", data.version));
    std::fs::write(&cp_tmp, &data.card)
        .map_err(|e| AppError::Io(format!("write card tmp: {e}")))?;
    std::fs::rename(&cp_tmp, &cp).map_err(|e| AppError::Io(format!("rename card: {e}")))?;

    // Advance active version in space_settings.
    let v = data.version.to_string();
    sqlx::query_file!(
        "queries/training/upsert_setting.sql",
        data.space_id,
        SETTING_ACTIVE_MODEL,
        v
    )
    .execute(db)
    .await
    .map_err(|e| AppError::Db(format!("set active model version: {e}")))?;

    Ok(())
}
