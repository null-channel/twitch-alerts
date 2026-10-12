use std::str::FromStr;

use sqlx::{
    migrate::Migrator,
    sqlite::{SqliteConnectOptions, SqlitePool},
};

/// Open the application's database and apply the migrations embedded by that application.
pub async fn setup_sqlite_migrations(
    db: String,
    migrations: &Migrator,
) -> anyhow::Result<SqlitePool> {
    let options = SqliteConnectOptions::from_str(&db)?.create_if_missing(true);
    let pool = SqlitePool::connect_with(options).await?;
    migrations.run(&pool).await?;
    Ok(pool)
}
