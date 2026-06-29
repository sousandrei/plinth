use std::path::{Path, PathBuf};

use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};
use tauri::Manager;

use crate::sync::model_sync::md5_hex;

pub type DbPool = SqlitePool;

pub async fn setup(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = app.path().app_data_dir()?;
    std::fs::create_dir_all(&data_dir)?;
    let db_path = data_dir.join("plinth.db");
    let pool = init(&db_path).await?;
    app.manage(pool);
    Ok(())
}

async fn init(path: &Path) -> Result<DbPool, Box<dyn std::error::Error>> {
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(std::time::Duration::from_secs(5));

    let pool = SqlitePoolOptions::new()
        .min_connections(1)
        .connect_with(opts)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;
    init_sync_settings(&pool).await?;
    backfill_model_versions(path, &pool).await?;

    Ok(pool)
}

/// One-time backfill: scan every space's models dir on disk and INSERT
/// any `(version, weights_md5, card_md5)` pair that's missing from
/// `model_versions`. The migration to v3 creates the table but leaves
/// rows for already-trained models empty; this pass fills them in
/// without re-training. Sync engines already running on this host
/// will treat the rows as authored-by-this-device (which is correct
/// — the on-disk files are this host's). Safe to run on every
/// startup: the upsert query is a no-op when the row already exists.
///
/// `db_path` is `<data_dir>/plinth.db`; the function derives
/// `<data_dir>/models` from it so we don't need a separate
/// `AppHandle` here.
async fn backfill_model_versions(
    db_path: &Path,
    pool: &DbPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(data_dir) = db_path.parent() else {
        return Ok(());
    };
    let data_dir = data_dir.join("models");
    if !data_dir.exists() {
        return Ok(());
    }

    let entries = match std::fs::read_dir(&data_dir) {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };

    for space_entry in entries.flatten() {
        let space_dir = space_entry.path();
        if !space_dir.is_dir() {
            continue;
        }
        let space_id = match space_dir.file_name().and_then(|n| n.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };

        let files = match std::fs::read_dir(&space_dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        let mut versions: std::collections::BTreeMap<u32, (PathBuf, PathBuf)> =
            std::collections::BTreeMap::new();
        for file in files.flatten() {
            let name = file.file_name();
            let s = name.to_string_lossy();
            let Some(rest) = s.strip_prefix("model_v") else {
                continue;
            };
            let (v_str, ext) = if let Some(n) = rest.strip_suffix(".safetensors") {
                (n, "weights")
            } else if let Some(n) = rest.strip_suffix(".json") {
                (n, "card")
            } else {
                continue;
            };
            let Ok(v) = v_str.parse::<u32>() else {
                continue;
            };
            let entry = versions.entry(v).or_insert_with(|| {
                (
                    space_dir.join(format!("model_v{v_str}.safetensors")),
                    space_dir.join(format!("model_v{v_str}.json")),
                )
            });
            if ext == "weights" {
                entry.0 = file.path();
            } else {
                entry.1 = file.path();
            }
        }

        for (v, (wp, cp)) in versions {
            if !wp.is_file() || !cp.is_file() {
                continue;
            }
            let weights = match std::fs::read(&wp) {
                Ok(b) => b,
                Err(_) => continue,
            };
            let card = match std::fs::read(&cp) {
                Ok(b) => b,
                Err(_) => continue,
            };
            let trained_at: String = serde_json::from_slice::<serde_json::Value>(&card)
                .ok()
                .and_then(|v| {
                    v.get("trained_at")
                        .and_then(|t| t.as_str())
                        .map(String::from)
                })
                .unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string());
            let weights_md5 = md5_hex(&weights);
            let card_md5 = md5_hex(&card);

            let _ = sqlx::query_file!(
                "queries/training/upsert_model_version.sql",
                space_id,
                v,
                weights_md5,
                card_md5,
                trained_at
            )
            .execute(pool)
            .await;
        }
    }

    Ok(())
}

/// Initializes the `app_settings` keys required by the P2P sync engine on
/// first launch. These keys are read by the `change_log` triggers on every
/// mutation, so they MUST exist before any synced table is written to.
///
///   - `device_id`  stable UUID identifying this physical install
///   - `sync_seq`   per-device monotonic counter (stored as TEXT, cast to INTEGER)
async fn init_sync_settings(pool: &DbPool) -> Result<(), Box<dyn std::error::Error>> {
    let device_id = uuid::Uuid::new_v4().to_string();
    sqlx::query_file!("queries/settings/init_device_id.sql", device_id)
        .execute(pool)
        .await?;

    sqlx::query_file!("queries/settings/init_sync_seq.sql")
        .execute(pool)
        .await?;

    Ok(())
}
