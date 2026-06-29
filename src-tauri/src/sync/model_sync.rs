use std::path::{Path, PathBuf};

use sqlx::SqlitePool;
use tauri::{AppHandle, Manager};

use crate::error::AppError;
use crate::sync::wire::{ModelData, ModelVersionEntry, ModelVersionSummary};

// ---------------------------------------------------------------------------
// Keys and paths (mirrors `commands/training.rs` conventions)
// ---------------------------------------------------------------------------

const SETTING_ACTIVE_MODEL: &str = "active_model_version";
const SETTING_ACTIVE_MODEL_WEIGHTS_MD5: &str = "active_model_weights_md5";
const SETTING_ACTIVE_MODEL_CARD_MD5: &str = "active_model_card_md5";

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

fn md5_hex(bytes: &[u8]) -> String {
    use md5::{Digest, Md5};
    let hash = Md5::digest(bytes);
    hash.iter().map(|b| format!("{b:02x}")).collect()
}

// ---------------------------------------------------------------------------
// Reading local state
// ---------------------------------------------------------------------------

/// Returns the active finetuned model version for `space_id`, or 0 if
/// none exists (base model only). Reads from `space_settings` and
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

/// Decide whether a sender should ship `ModelData` to a peer whose
/// summary reports `peer_ver` and `peer_weights_md5`. Tiebreak on
/// identical versions + divergent MD5s is `weights_md5 > peer`
/// (then device_id) so both peers apply the same rule and converge
/// without oscillation. Empty MD5s (legacy data) fall back to the
/// version-only behavior.
pub fn should_send_model(
    local_ver: u32,
    peer_ver: u32,
    local_weights_md5: &str,
    peer_weights_md5: &str,
    local_device_id: &str,
    peer_device_id: &str,
) -> bool {
    match local_ver.cmp(&peer_ver) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => {
            if local_ver == 0 || local_weights_md5.is_empty() || peer_weights_md5.is_empty() {
                return false;
            }
            if local_weights_md5 != peer_weights_md5 {
                local_weights_md5 > peer_weights_md5
            } else {
                local_device_id > peer_device_id
            }
        }
    }
}

/// `recv_half` should apply incoming `ModelData` whenever the version
/// is higher OR the version matches but the MD5 differs (the sender's
/// tiebreak decided this side is the winner, so we trust it). Empty
/// MD5s in either side fall back to the version-only behavior.
pub fn should_apply_model(
    data_version: u32,
    local_version: u32,
    data_weights_md5: &str,
    local_weights_md5: &str,
) -> bool {
    match data_version.cmp(&local_version) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => {
            if data_version == 0 || data_weights_md5.is_empty() || local_weights_md5.is_empty() {
                return false;
            }
            data_weights_md5 != local_weights_md5
        }
    }
}

/// Returns `(weights_md5, card_md5)` for `version` in `space_id`.
/// Reads from `space_settings` first; if either hash is missing
/// (pre-upgrade data), computes from the on-disk files and caches the
/// result. Returns empty strings on any failure so the caller can
/// still ship a summary entry — empty hashes force peers to skip
/// divergence comparison and treat the entry like a pre-MD5 record.
pub async fn local_md5s(
    db: &SqlitePool,
    app: &AppHandle,
    space_id: &str,
    version: u32,
) -> (String, String) {
    let cached_weights = sqlx::query_file!(
        "queries/training/get_setting.sql",
        space_id,
        SETTING_ACTIVE_MODEL_WEIGHTS_MD5
    )
    .fetch_optional(db)
    .await
    .ok()
    .flatten()
    .map(|r| r.value);
    let cached_card = sqlx::query_file!(
        "queries/training/get_setting.sql",
        space_id,
        SETTING_ACTIVE_MODEL_CARD_MD5
    )
    .fetch_optional(db)
    .await
    .ok()
    .flatten()
    .map(|r| r.value);

    if let (Some(w), Some(c)) = (cached_weights.as_ref(), cached_card.as_ref()) {
        return (w.clone(), c.clone());
    }

    let dir = match app.path().app_data_dir() {
        Ok(d) => d.join("models").join(space_id),
        Err(_) => return (String::new(), String::new()),
    };
    let wp = weights_path(&dir, version);
    let cp = card_path(&dir, version);
    let (Ok(w_bytes), Ok(c_bytes)) = (std::fs::read(&wp), std::fs::read(&cp)) else {
        return (String::new(), String::new());
    };
    let weights_md5 = md5_hex(&w_bytes);
    let card_md5 = md5_hex(&c_bytes);

    let _ = sqlx::query_file!(
        "queries/training/upsert_setting.sql",
        space_id,
        SETTING_ACTIVE_MODEL_WEIGHTS_MD5,
        weights_md5
    )
    .execute(db)
    .await;
    let _ = sqlx::query_file!(
        "queries/training/upsert_setting.sql",
        space_id,
        SETTING_ACTIVE_MODEL_CARD_MD5,
        card_md5
    )
    .execute(db)
    .await;

    (weights_md5, card_md5)
}

/// Build the `ModelVersionSummary` to send to a peer: one entry per
/// shared space with the local active version plus MD5s of the
/// weights and card for divergence detection.
pub async fn local_summary(
    db: &SqlitePool,
    app: &AppHandle,
    space_ids: &[String],
) -> ModelVersionSummary {
    let mut entries = Vec::with_capacity(space_ids.len());
    for space_id in space_ids {
        let version = local_version(db, app, space_id).await;
        let (weights_md5, card_md5) = if version > 0 {
            local_md5s(db, app, space_id, version).await
        } else {
            (String::new(), String::new())
        };
        entries.push(ModelVersionEntry {
            space_id: space_id.clone(),
            version,
            weights_md5,
            card_md5,
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

/// Recompute and store both MD5s in `space_settings` for the given
/// `version`. Used by `fine_tune` immediately after the weights + card
/// files are written, so `local_summary` doesn't have to re-read and
/// hash the files on every sync session.
pub async fn cache_md5s_for_version(
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
    let card =
        std::fs::read(&cp).map_err(|e| AppError::Io(format!("read card v{version}: {e}")))?;
    let weights_md5 = md5_hex(&weights);
    let card_md5 = md5_hex(&card);

    sqlx::query_file!(
        "queries/training/upsert_setting.sql",
        space_id,
        SETTING_ACTIVE_MODEL_WEIGHTS_MD5,
        weights_md5
    )
    .execute(db)
    .await
    .map_err(|e| AppError::Db(format!("cache weights md5: {e}")))?;
    sqlx::query_file!(
        "queries/training/upsert_setting.sql",
        space_id,
        SETTING_ACTIVE_MODEL_CARD_MD5,
        card_md5
    )
    .execute(db)
    .await
    .map_err(|e| AppError::Db(format!("cache card md5: {e}")))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Applying an incoming ModelData
// ---------------------------------------------------------------------------

/// Write an incoming `ModelData` atomically, verify its MD5s, advance
/// the active version in `space_settings`, and cache the MD5s. Writes
/// to temp files first, then renames so a crash mid-transfer leaves
/// the previous version intact. Empty MD5 fields on the incoming
/// frame are treated as "unverified" (no integrity check) so older
/// peers can still drive a transfer.
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

    sqlx::query_file!(
        "queries/training/upsert_setting.sql",
        data.space_id,
        SETTING_ACTIVE_MODEL_WEIGHTS_MD5,
        computed_weights_md5
    )
    .execute(db)
    .await
    .map_err(|e| AppError::Db(format!("cache weights md5: {e}")))?;
    sqlx::query_file!(
        "queries/training/upsert_setting.sql",
        data.space_id,
        SETTING_ACTIVE_MODEL_CARD_MD5,
        computed_card_md5
    )
    .execute(db)
    .await
    .map_err(|e| AppError::Db(format!("cache card md5: {e}")))?;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_send_higher_version() {
        assert!(should_send_model(5, 3, "a", "b", "local", "peer"));
    }

    #[test]
    fn should_not_send_lower_version() {
        assert!(!should_send_model(3, 5, "a", "b", "local", "peer"));
    }

    /// Base model (v0) on both sides — never a transfer regardless of MD5s.
    #[test]
    fn should_not_send_when_both_at_zero() {
        assert!(!should_send_model(0, 0, "a", "b", "local", "peer"));
        assert!(!should_send_model(0, 0, "", "", "local", "peer"));
    }

    /// Same version, same MD5 → already in sync, no transfer.
    #[test]
    fn should_not_send_when_in_sync() {
        assert!(!should_send_model(5, 5, "aaaa", "aaaa", "local", "peer"));
    }

    /// Same version, divergent MD5 → tiebreak by MD5 lex order so both
    /// peers agree on the winner without oscillation.
    #[test]
    fn should_send_only_winning_md5() {
        assert!(should_send_model(5, 5, "bbbb", "aaaa", "local", "peer"));
        assert!(!should_send_model(5, 5, "aaaa", "bbbb", "local", "peer"));
    }

    /// MD5 collision (highly improbable) → fall back to device_id so
    /// the choice is still deterministic.
    #[test]
    fn should_tiebreak_md5_with_device_id() {
        assert!(should_send_model(5, 5, "same", "same", "z-local", "a-peer"));
        assert!(!should_send_model(
            5, 5, "same", "same", "a-local", "z-peer"
        ));
    }

    /// Legacy data — one side has no MD5 → fall back to version-only
    /// behavior (equal versions, no transfer).
    #[test]
    fn should_skip_md5_compare_when_either_md5_missing() {
        assert!(!should_send_model(5, 5, "aaaa", "", "local", "peer"));
        assert!(!should_send_model(5, 5, "", "aaaa", "local", "peer"));
        assert!(!should_send_model(5, 5, "", "", "local", "peer"));
    }

    #[test]
    fn should_apply_higher_version() {
        assert!(should_apply_model(5, 3, "any", "any"));
    }

    #[test]
    fn should_not_apply_lower_version() {
        assert!(!should_apply_model(3, 5, "any", "any"));
    }

    #[test]
    fn should_apply_divergence_regardless_of_winner() {
        assert!(should_apply_model(5, 5, "aaaa", "bbbb"));
        assert!(should_apply_model(5, 5, "bbbb", "aaaa"));
    }

    #[test]
    fn should_not_apply_when_in_sync() {
        assert!(!should_apply_model(5, 5, "aaaa", "aaaa"));
    }

    #[test]
    fn apply_skips_md5_compare_when_either_md5_missing() {
        assert!(!should_apply_model(5, 5, "aaaa", ""));
        assert!(!should_apply_model(5, 5, "", "aaaa"));
        assert!(!should_apply_model(5, 5, "", ""));
    }
}
