use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions},
    Error,
};
use std::str::FromStr;

/// Initialize the SQLite database pool and run migrations
pub async fn init_database(database_path: &str) -> Result<SqlitePool, Error> {
    // Create SQLite connection options
    let options = SqliteConnectOptions::from_str(database_path)?.create_if_missing(true);

    // Create connection pool
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    // Run migrations
    sqlx::migrate!("./migrations").run(&pool).await?;

    Ok(pool)
}
