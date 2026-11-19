use std::path::Path;

use sqlx::{
    Connection, SqliteConnection, SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};
use tauri::Manager;

use crate::sync_preview::{IDENTIFIER, SyncPreview};

pub type DbPool = SqlitePool;
static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

pub async fn setup(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = app.path().app_data_dir()?;
    std::fs::create_dir_all(&data_dir)?;
    let preview = if app.config().identifier == IDENTIFIER {
        SyncPreview::open(data_dir.clone()).await?
    } else {
        SyncPreview::default()
    };
    let pool = init(&data_dir.join("plinth.db"), &preview).await?;
    app.manage(preview);
    app.manage(pool);
    Ok(())
}

pub(crate) async fn init(
    path: &Path,
    preview: &SyncPreview,
) -> Result<DbPool, Box<dyn std::error::Error>> {
    let preview_active = preview.active();
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(if preview_active {
            SqliteSynchronous::Full
        } else {
            SqliteSynchronous::Normal
        })
        .foreign_keys(true)
        .busy_timeout(std::time::Duration::from_secs(5));

    let mut pool_options = SqlitePoolOptions::new().min_connections(1);
    if preview_active {
        let mut conn = SqliteConnection::connect_with(&opts).await?;
        MIGRATOR.run(&mut conn).await?;
        preview.replay(&mut conn).await?;
        conn.close().await?;

        // Holding the only lease through the queue acknowledgement keeps bulk
        // writes and imports from overtaking an unfinished category edit.
        let on_open = preview.clone();
        let on_acquire = preview.clone();
        pool_options = pool_options
            .max_connections(1)
            .after_connect(move |conn, _| {
                let preview = on_open.clone();
                Box::pin(async move { preview.replay(conn).await.map_err(replay_error) })
            })
            .before_acquire(move |conn, _| {
                let preview = on_acquire.clone();
                Box::pin(async move {
                    preview.replay(conn).await.map_err(replay_error)?;
                    Ok(true)
                })
            });
    }

    let pool = pool_options.connect_with(opts).await?;
    if !preview_active {
        MIGRATOR.run(&pool).await?;
    }

    Ok(pool)
}

fn replay_error(error: crate::AppError) -> sqlx::Error {
    sqlx::Error::Protocol(format!("replay pending transaction edits: {error}"))
}
