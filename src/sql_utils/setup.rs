use std::{
    env,
    path::{Path, PathBuf},
    str::FromStr,
};

use sqlx::sqlite::{SqliteConnectOptions, SqlitePool};

pub async fn setup_sqlite_migrations(
    db: String,
    migrations_dir: PathBuf,
) -> anyhow::Result<SqlitePool> {
    // will create the db if needed
    println!("DB: {}", db.clone());
    println!("Migrations: {:?}", migrations_dir);
    let path = env::current_dir()?;
    println!("The current directory is {}", path.display());
    let url = SqliteConnectOptions::from_str(&db)
        .map_err(|e| anyhow::anyhow!(e))?
        .create_if_missing(true);
    let pool = SqlitePool::connect_with(url).await?;

    println!("Running migrations from: {:?}", migrations_dir.clone());

    sqlx::migrate::Migrator::new(migrations_dir)
        .await?
        .run(&pool)
        .await?;
    // Return the connection manager
    Ok(pool)
}

pub fn get_migrations_from_cargo_dir(dir: &str) -> anyhow::Result<PathBuf> {
    let crate_dir = std::env::var("CARGO_MANIFEST_DIR")?;
    Ok(Path::new(&crate_dir).join(dir))
}
