use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    rt.block_on(async {
        let path = Path::new("dev.db");
        let opts = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true);
        let pool = SqlitePool::connect_with(opts).await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        println!("Database dev.db migrated successfully!");
        Ok(())
    })
}
