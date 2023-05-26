#![warn(clippy::unwrap_in_result)]
mod util;
use ai_manager_service::AIManager;
use clap::Parser;
use forntend_api_lib::FrontendApi;
use twitch_listener_service_lib::opts::Opts;
use twitch_listener_service_lib::websocket::WebsocketClient;
use twitch_api::twitch_oauth2::UserToken;

use std::{env, path::Path, sync::Arc};

use eyre::Context;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePool};
use tokio::{
    sync::{mpsc, RwLock},
    task::JoinHandle,
};
use twitch_api::{client::ClientDefault, HelixClient};

#[tokio::main(flavor = "multi_thread", worker_threads = 32)]
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

    run(&opts)
        .await
        .with_context(|| "when running application")?;

    Ok(())
}

pub async fn run(opts: &Opts) -> eyre::Result<()> {


    let client: HelixClient<'static, _> = twitch_api::HelixClient::with_client(
        <reqwest::Client>::default_client_with_name(Some(
            "twitch-rs/eventsub"
                .parse()
                .wrap_err_with(|| "when creating header name")
                .unwrap(),
        ))
        .wrap_err_with(|| "when creating client")?,
    );

    let token = util::get_access_token(client.get_client(), opts).await?;
    let token: Arc<RwLock<UserToken>> = Arc::new(RwLock::new(token));
    let retainer = Arc::new(retainer::Cache::<String, ()>::new());
    let ret = retainer.clone();
    let retainer_cleanup = async move {
        ret.monitor(10, 0.50, tokio::time::Duration::from_secs(86400 / 2))
            .await;
        Ok::<(), eyre::Report>(())
    };
    let user_id = if let Some(ref id) = opts.channel_id {
        id.clone().into()
    } else if let Some(ref login) = opts.channel_login {
        client
            .get_user_from_login(login, &*token.read().await)
            .await?
            .ok_or_else(|| eyre::eyre!("no user found with name {login}"))?
            .id
    } else {
        token.read().await.user_id.clone()
    };

    let Some(gpt_key) = opts.gpt_key.clone() else {
        eyre::bail!("GPT key is required");
    };

    // set up sqlite database

    let Some(db_path) = opts.db_path.clone() else {
        eyre::bail!("db path is required");
    };

    let sqlite_pool = setup_sqlite(db_path.clone()).await?;

    let (sender, receiver) = mpsc::unbounded_channel();
    let (frentend_sender, frontend_receiver) = mpsc::unbounded_channel();

    let ai_manager_res = AIManager::new(sqlite_pool, gpt_key, frentend_sender);

    let Ok(ai_manager) = ai_manager_res else {
        panic!("failed to create the ai manager");
    };

    let twitch_websocket_client = WebsocketClient {
        session_id: None,
        token,
        client,
        user_id,
        connect_url: twitch_api::TWITCH_EVENTSUB_WEBSOCKET_URL.clone(),
        sender,
    };

    let frontend_api = FrontendApi::new("127.0.0.1:9000".into());

    /*
       let websocket_client_bla = {
           let opts = opts.clone();
           let sender = sender.clone();
           let websocket_client = websocket_client.clone();
           async move {  }
       };
    */
    let twithc_clinet = twitch_websocket_client.clone();

    let r = tokio::try_join!(
        flatten(tokio::spawn(retainer_cleanup)),
        flatten(tokio::spawn(async move {
            let clinet = twithc_clinet.clone();
            clinet.run().await
        })),
        flatten(tokio::spawn(async move { ai_manager.run(receiver).await })),
        flatten(tokio::spawn(async move {
            frontend_api.run(frontend_receiver).await
        })),
    );
    r?;
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
