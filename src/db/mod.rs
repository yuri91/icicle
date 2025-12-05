use sqlx::{sqlite::SqlitePool, Error};

/// Initialize the SQLite database pool and run migrations
pub async fn init_database(database_path: &str) -> Result<SqlitePool, Error> {
    // Create database file if it doesn't exist
    let db_url = format!("sqlite:{}", database_path);

    // Connect to database
    let pool = SqlitePool::connect(&db_url).await?;

    // Run migrations
    sqlx::migrate!("./migrations").run(&pool).await?;

    Ok(pool)
}
