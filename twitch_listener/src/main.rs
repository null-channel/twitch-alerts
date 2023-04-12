#![warn(clippy::unwrap_in_result)]
pub mod opts;
pub mod util;
pub mod websocket;

use ai_manager::AIManager;
use clap::Parser;
pub use opts::Secret;
use twitch_oauth2::UserToken;

use std::{env, path::Path, sync::Arc, thread};

use opts::Opts;

use eyre::Context;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePool};
use std::sync::mpsc;
use tokio::{sync::RwLock, task::JoinHandle};
use twitch_api::{client::ClientDefault, HelixClient};

#[tokio::main]
async fn main() -> Result<(), eyre::Report> {
    util::install_utils()?;
    let opts = Opts::parse();

    tracing::debug!(
        "App started!\n{}",
        Opts::try_parse_from(["app", "--version"])
            .unwrap_err()
            .to_string()
    );

    tracing::debug!(opts = ?opts);


    Ok(())
}


async fn event_queue(rx: mpsc::Receiver<String>) -> eyre::Result<()> {
    Ok(())
}

async fn setup_sqlite(db: String) -> eyre::Result<SqlitePool> {
    // will create the db if needed
    let url = SqliteConnectOptions::new()
        .filename(db)
        .create_if_missing(true);
    let pool = SqlitePool::connect_with(url).await?;

    // Run migrations
    let migrations = if env::var("RUST_ENV") == Ok("production".to_string()) {
        // Productions migrations dir
        std::env::current_exe()?.join("./migrations")
    } else {
        // Development migrations dir
        let crate_dir = std::env::var("CARGO_MANIFEST_DIR")?;
        Path::new(&crate_dir).join("./migrations")
    };

    sqlx::migrate::Migrator::new(migrations)
        .await?
        .run(&pool)
        .await?;

    // Return the connection manager
    Ok(pool)
}

async fn flatten<T>(handle: JoinHandle<Result<T, eyre::Report>>) -> Result<T, eyre::Report> {
    match handle.await {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(err)) => Err(err),
        Err(e) => Err(e).wrap_err_with(|| "handling failed"),
    }
}
