#![warn(clippy::unwrap_in_result)]
mod ai_manager;
mod auth;
mod frontend;
mod messages;
mod opts;
mod utils;

use crate::frontend::{FrontendApi, HostInfo};
use ai_manager::AIManager;
use clap::Parser;
use opts::{Cli, Commands, ServerArgs};
use tokio::sync::mpsc;
use twitch_api::HelixClient;
use twitch_common::twitch_listener::websocket::WebsocketClient;

static MIGRATIONS: sqlx::migrate::Migrator = sqlx::migrate!("./src/migrations");

fn main() -> anyhow::Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(32)
        .enable_all()
        .build()?
        .block_on(run())
}

async fn run() -> anyhow::Result<()> {
    utils::install_utils()?;
    let cmd = Cli::parse();

    tracing::debug!("Starting twitch_alerts {}", env!("CARGO_PKG_VERSION"));

    match cmd.command {
        Commands::Server(opts) => {
            run_server(&opts).await?;
        }
        Commands::Authorize(opts) => auth::authorize(&opts).await?,
    }

    Ok(())
}

pub async fn run_server(opts: &ServerArgs) -> anyhow::Result<()> {
    let client: HelixClient<'static, _> =
        twitch_api::HelixClient::with_client(auth::http_client()?);
    let mut auth = auth::TokenManager::load(client.get_client().clone(), &opts.twitch).await?;
    let token = auth.token.clone();
    let user_id = if let Some(ref id) = opts.twitch.channel_id {
        id.clone().into()
    } else if let Some(ref login) = opts.twitch.channel_login {
        client
            .get_user_from_login(login, &*token.read().await)
            .await?
            .ok_or_else(|| anyhow::anyhow!("no user found with name {login}"))?
            .id
    } else {
        token.read().await.user_id.clone()
    };
    anyhow::ensure!(
        user_id == token.read().await.user_id,
        "authorize as the broadcaster whose alerts you want to monitor"
    );

    let Some(gpt_key) = opts.llm.openai.openai_key.clone() else {
        anyhow::bail!("GPT key is required");
    };

    // set up sqlite database
    // TODO: Make this required in the args you fool
    let Some(db_path) = opts.db_path.clone() else {
        anyhow::bail!("db path is required");
    };

    let sqlite_pool =
        sql_common::setup::setup_sqlite_migrations(db_path.clone(), &MIGRATIONS).await?;

    let (sender, receiver) = mpsc::unbounded_channel();
    let (frontend_sender, frontend_receiver) = mpsc::unbounded_channel();

    let ai_manager = AIManager::new(
        sqlite_pool,
        gpt_key,
        opts.llm.openai.openai_model.clone(),
        frontend_sender,
    )?;

    let mut twitch_client = WebsocketClient {
        session_id: None,
        token,
        client,
        user_id,
        connect_url: twitch_api::TWITCH_EVENTSUB_WEBSOCKET_URL.clone(),
        sender,
    };

    println!(
        "Starting frontend api on http port: {} and ws port: {}, and host name: {}",
        opts.http.port, opts.ws.ws_port, opts.ws.ws_host,
    );

    let host_info = HostInfo {
        websocket_host: opts.ws.ws_host.clone(),
        ws_port: opts.ws.ws_port,
        http_port: opts.http.port,
        http_host: opts.http.host.clone(),
    };

    let frontend_api = FrontendApi::new(host_info, opts.frontend_assets.clone());

    tokio::try_join!(
        auth.run(),
        twitch_client.run(),
        ai_manager.run(receiver),
        frontend_api.run(frontend_receiver),
    )?;
    Ok(())
}
