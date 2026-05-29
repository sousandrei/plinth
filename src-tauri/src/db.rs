use std::path::Path;

use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    SqlitePool,
};
use tauri::Manager;

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

    Ok(pool)
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
