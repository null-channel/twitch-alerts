#![warn(clippy::unwrap_in_result)]
/// Mods
mod ai_manager;
mod frontend;
mod games;
mod messages;
mod opts;
mod twitch_chat;
mod twitch_listener;
mod utils;

/// TODO: Cleanup/organize the imports
use crate::frontend::{FrontendApi, HostInfo};
use ai_manager::AIManager;
use clap::Parser;
use futures::{FutureExt, StreamExt};
use games::{twitch_chat::TwitchChat, wordle};
use opts::{Cli, Commands, GamesSubCommand, ServerArgs, TwitchChatArgs};
use twitch_api::twitch_oauth2::UserToken;
use twitch_listener::websocket::WebsocketClient;

use std::{env, path::Path, sync::Arc};

use sqlx::sqlite::{SqliteConnectOptions, SqlitePool};
use tokio::{
    sync::{mpsc, RwLock},
    task::JoinHandle,
};
use twitch_api::{client::ClientDefault, HelixClient};

#[tokio::main(flavor = "multi_thread", worker_threads = 32)]
async fn main() -> anyhow::Result<()> {
    utils::install_utils()?;
    let cmd = Cli::parse();

    // WILL PRINT CREDINTIALS!!!!
    // eprintln!("Starting app with options: {:?}", cmd);

    tracing::debug!(
        "App started!\n{}",
        Cli::try_parse_from(["app", "--version"])
            .unwrap_err()
            .to_string()
    );

    // Initialize the tracing

    tracing::debug!(opts = ?cmd);

    match cmd.command {
        Commands::Server(opts) => {
            run_server(&opts).await?;
        }
        Commands::Chat => {
            run_chat().await?;
        }
        Commands::Bot => {
            run_bot().await?;
        }
        Commands::Game(opts) => {
            run_game(&opts).await?;
        }
    }

    Ok(())
}

pub async fn run_chat() -> anyhow::Result<()> {
    Ok(())
}
pub async fn run_bot() -> anyhow::Result<()> {
    Ok(())
}
pub async fn run_game(opts: &GamesSubCommand) -> anyhow::Result<()> {
    match opts {
        GamesSubCommand::Wordle(args) => {
            let mut twitch = get_twitch_chat(&args.twitch).await?;
            let (chat_sender, chat_receiver) = tokio::sync::mpsc::unbounded_channel();
            tokio::spawn(async move { twitch.run(chat_sender, "marekcounts".to_owned()).await });

            //TODO: Start the game
            games::wordle::game::start_game(chat_receiver);
        }
        GamesSubCommand::Dragons(args) => {
            anyhow::bail!("Dragons game is not implemented yet");
        }
    }
    Ok(())
}

pub async fn get_twitch_chat(opts: &TwitchChatArgs) -> anyhow::Result<TwitchChat> {
    let chat = TwitchChat::new(
        opts.channel.clone(),
        opts.tc_username.clone(),
        opts.tc_password.clone(),
    )
    .await?;
    Ok(chat)
}

pub async fn run_server(opts: &ServerArgs) -> anyhow::Result<()> {
    let client: HelixClient<'static, _> = twitch_api::HelixClient::with_client(
        <reqwest::Client>::default_client_with_name(Some("twitch-rs/eventsub".parse()?))?,
    );

    let token = utils::get_access_token(client.get_client(), &opts.twitch).await?;
    let token: Arc<RwLock<UserToken>> = Arc::new(RwLock::new(token));
    let retainer = Arc::new(retainer::Cache::<String, ()>::new());
    let ret = retainer.clone();
    let retainer_cleanup = async move {
        ret.monitor(10, 0.50, tokio::time::Duration::from_secs(86400 / 2))
            .await;
        Ok(())
    };
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

    let Some(gpt_key) = opts.llm.openai.openai_key.clone() else {
        anyhow::bail!("GPT key is required");
    };

    // set up sqlite database
    // TODO: Make this required in the args you fool
    let Some(db_path) = opts.db_path.clone() else {
        anyhow::bail!("db path is required");
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

    println!(
        "Starting frontend api on http port: {} and ws port: {}, and host name: {}",
        opts.http.port, opts.ws.ws_port, opts.ws.ws_host,
    );

    let host_info = HostInfo {
        websocket_host: opts.ws.ws_host.clone(),
        ws_port: opts.ws.ws_port,
        http_port: opts.ws.ws_port,
    };

    let frontend_api = FrontendApi::new(host_info, opts.frontend_assets.clone());

    let twithc_clinet = twitch_websocket_client.clone();

    let r = tokio::try_join!(
        flatten(tokio::spawn(retainer_cleanup)),
        flatten(tokio::spawn(async move {
            let mut clinet = twithc_clinet.clone();
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

async fn setup_sqlite(db: String) -> anyhow::Result<SqlitePool> {
    // will create the db if needed
    let url = SqliteConnectOptions::new()
        .filename(db)
        .create_if_missing(true);
    let pool = SqlitePool::connect_with(url).await?;

    // Run migrations
    let migrations = if env::var("ENV") == Ok("production".to_string()) {
        // Productions migrations dir

        let crate_dir = std::env::var("AI_MIGRATIONS_DIR")?;
        Path::new(&crate_dir).join("migrations")
    } else {
        // Development migrations dir
        let crate_dir = std::env::var("CARGO_MANIFEST_DIR")?;
        let Some(path) = Path::new(&crate_dir).parent() else {
            panic!()
        };

        path.join("ai_manager_service/migrations")
    };

    println!("Running migrations from: {:?}", migrations.clone());

    sqlx::migrate::Migrator::new(migrations)
        .await?
        .run(&pool)
        .await?;

    // Return the connection manager
    Ok(pool)
}

async fn flatten<T>(handle: JoinHandle<anyhow::Result<T>>) -> anyhow::Result<T> {
    match handle.await {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(err)) => Err(err),
        Err(e) => Err(anyhow::anyhow!(e)),
    }
}
