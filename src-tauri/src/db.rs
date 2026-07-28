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
/// — the on-disk files are this host's).
///
/// Gated by a durable `model_backfill_done` marker in `app_settings`
/// so it runs exactly once. Uses `ON CONFLICT DO NOTHING` so orphan
/// files left after a remote model deletion cannot overwrite a
/// synchronized manifest or resurrect a deleted row on restart. See
/// `data/PLAN.md` Step 28.4.
async fn backfill_model_versions(
    db_path: &Path,
    pool: &DbPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let done = sqlx::query_file_scalar!("queries/settings/get_setting.sql", "model_backfill_done")
        .fetch_optional(pool)
        .await?;
    if done.is_some() {
        return Ok(());
    }

    let Some(data_dir) = db_path.parent() else {
        return Ok(());
    };
    let data_dir = data_dir.join("models");
    if !data_dir.exists() {
        // Nothing to scan — still mark as done so we don't retry on
        // every restart.
        sqlx::query_file!(
            "queries/settings/set_setting.sql",
            "model_backfill_done",
            "1"
        )
        .execute(pool)
        .await?;
        return Ok(());
    }

    let entries = match std::fs::read_dir(&data_dir) {
        Ok(e) => e,
        Err(_) => {
            sqlx::query_file!(
                "queries/settings/set_setting.sql",
                "model_backfill_done",
                "1"
            )
            .execute(pool)
            .await?;
            return Ok(());
        }
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
                "queries/training/insert_model_version_if_missing.sql",
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

    sqlx::query_file!(
        "queries/settings/set_setting.sql",
        "model_backfill_done",
        "1"
    )
    .execute(pool)
    .await?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;
    use tempfile::TempDir;

    async fn fresh_pool(dir: &TempDir) -> (SqlitePool, PathBuf) {
        let db_path = dir.path().join("plinth.db");
        let opts = SqliteConnectOptions::new()
            .filename(&db_path)
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query_file!("queries/settings/init_device_id.sql", "test-device")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query_file!("queries/settings/init_sync_seq.sql")
            .execute(&pool)
            .await
            .unwrap();
        (pool, db_path)
    }

    fn write_model_files(
        db_path: &Path,
        space_id: &str,
        version: u32,
        weights: &[u8],
        card: &[u8],
    ) {
        let models_dir = db_path.parent().unwrap().join("models");
        let space_dir = models_dir.join(space_id);
        std::fs::create_dir_all(&space_dir).unwrap();
        std::fs::write(
            space_dir.join(format!("model_v{version}.safetensors")),
            weights,
        )
        .unwrap();
        std::fs::write(space_dir.join(format!("model_v{version}.json")), card).unwrap();
    }

    /// Restart after a remote model deletion must not recreate the
    /// manifest. The backfill runs once (marked by `model_backfill_done`),
    /// uses `ON CONFLICT DO NOTHING`, so a second call is a no-op even
    /// if orphan files remain on disk.
    #[tokio::test]
    async fn backfill_does_not_recreate_deleted_manifest() {
        let dir = TempDir::new().unwrap();
        let (pool, db_path) = fresh_pool(&dir).await;

        let ts = "2024-01-01T00:00:00Z";
        sqlx::query_file!(
            "queries/tests/insert_space_fixture.sql",
            "s1",
            "test",
            ts,
            ts
        )
        .execute(&pool)
        .await
        .unwrap();

        let weights = b"weights-v1";
        let card = br#"{"trained_at":"2024-01-01T00:00:00Z"}"#;
        write_model_files(&db_path, "s1", 1, weights, card);

        backfill_model_versions(&db_path, &pool).await.unwrap();

        let count: i64 =
            sqlx::query_scalar!("SELECT COUNT(*) FROM model_versions WHERE space_id = 's1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 1, "backfill should insert the model version");

        // Simulate remote deletion: remove the DB row but leave orphan files.
        sqlx::query!("DELETE FROM model_versions WHERE space_id = 's1' AND version = 1")
            .execute(&pool)
            .await
            .unwrap();

        // Restart — backfill must NOT recreate the deleted row.
        backfill_model_versions(&db_path, &pool).await.unwrap();

        let count_after: i64 =
            sqlx::query_scalar!("SELECT COUNT(*) FROM model_versions WHERE space_id = 's1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            count_after, 0,
            "backfill must not resurrect a remotely deleted manifest"
        );
    }

    /// Existing manifest hashes are not changed by startup scanning.
    /// The backfill uses `ON CONFLICT DO NOTHING` so a pre-existing row
    /// with different MD5s (e.g. from sync) is never overwritten by the
    /// local file scan.
    #[tokio::test]
    async fn backfill_does_not_overwrite_existing_manifest() {
        let dir = TempDir::new().unwrap();
        let (pool, db_path) = fresh_pool(&dir).await;

        let ts = "2024-01-01T00:00:00Z";
        sqlx::query_file!(
            "queries/tests/insert_space_fixture.sql",
            "s1",
            "test",
            ts,
            ts
        )
        .execute(&pool)
        .await
        .unwrap();

        // Pre-insert a model_versions row with "synced" MD5s.
        sqlx::query_file!(
            "queries/training/upsert_model_version.sql",
            "s1",
            1u32,
            "synced-weights-md5",
            "synced-card-md5",
            ts
        )
        .execute(&pool)
        .await
        .unwrap();

        // Write model files with different content — backfill would
        // compute different MD5s if it overwrote.
        let weights = b"different-weights";
        let card = br#"{"trained_at":"2024-01-01T00:00:00Z"}"#;
        write_model_files(&db_path, "s1", 1, weights, card);

        backfill_model_versions(&db_path, &pool).await.unwrap();

        let row = sqlx::query!(
            "SELECT weights_md5, card_md5 FROM model_versions WHERE space_id = 's1' AND version = 1"
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(
            row.weights_md5, "synced-weights-md5",
            "backfill must not overwrite a synced manifest"
        );
        assert_eq!(
            row.card_md5, "synced-card-md5",
            "backfill must not overwrite a synced manifest"
        );
    }
}
