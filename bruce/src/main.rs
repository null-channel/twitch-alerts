#![warn(clippy::unwrap_in_result)]
mod opts;
mod utils;
mod wordle;

use anyhow::Context as _;
use clap::Parser;
use opts::{ChatArgs, Cli, Commands, GamesSubCommand, PlatformConfig};
use random_word::WordList;
use twitch_common::chat::twitch_chat::TwitchChat;
use twitch_common::chat::youtube_chat::YoutubeChat;
use twitch_common::chat::ChatHub;

static MIGRATIONS: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

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

    tracing::debug!("Starting bruce {}", env!("CARGO_PKG_VERSION"));

    match cmd.command {
        Commands::Game(opts) => {
            run_game(&opts).await?;
        }
    }

    Ok(())
}

pub async fn run_game(opts: &GamesSubCommand) -> anyhow::Result<()> {
    match opts {
        GamesSubCommand::Wordle(args) => {
            let chat = connect_chat(&args.chat).await?;
            let platforms = chat.platforms();
            let chat_receiver = chat.spawn();

            let sqlite_pool =
                sql_common::setup::setup_sqlite_migrations(args.db_path.clone(), &MIGRATIONS)
                    .await?;
            let word_list = WordList::from(args.word_list.as_str());

            // Anathema's `runtime.run` blocks a thread for the whole session. Running it on a
            // Tokio worker can strand `tokio::spawn` tasks (e.g. `game_loop`, chat sources) on
            // that same worker, so drive the TUI from the blocking pool instead.
            tokio::task::spawn_blocking(move || {
                crate::wordle::game::start_game(chat_receiver, platforms, word_list, sqlite_pool)
            })
            .await
            .map_err(|e| anyhow::anyhow!("Wordle TUI thread panicked or was cancelled: {e}"))??;
        }
        GamesSubCommand::Dragons(_) => {
            anyhow::bail!("Dragons game is not implemented yet");
        }
    }
    Ok(())
}

/// Connect to every configured chat platform.
///
/// Connections are made before the game starts so that bad credentials, an offline YouTube
/// broadcast and similar problems are reported on the terminal instead of inside the TUI.
pub async fn connect_chat(args: &ChatArgs) -> anyhow::Result<ChatHub> {
    let mut hub = ChatHub::new();
    for platform in args.platforms()? {
        match platform {
            PlatformConfig::Twitch(config) => {
                let source = TwitchChat::connect(config)
                    .await
                    .context("connect to Twitch chat")?;
                tracing::info!(source = %source_name(&source), "chat source ready");
                hub.add(source);
            }
            PlatformConfig::Youtube(config) => {
                let source = YoutubeChat::connect(config)
                    .await
                    .context("connect to YouTube live chat")?;
                tracing::info!(source = %source_name(&source), "chat source ready");
                hub.add(source);
            }
        }
    }
    Ok(hub)
}

fn source_name(source: &impl twitch_common::chat::ChatSource) -> String {
    source.name()
}
