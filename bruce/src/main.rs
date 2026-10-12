#![warn(clippy::unwrap_in_result)]
mod opts;
mod utils;
mod wordle;

use std::fs::OpenOptions;

use clap::Parser;
use opts::{Cli, Commands, GamesSubCommand, TwitchChatArgs};
use random_word::WordList;
use tracing_subscriber::{fmt, prelude::*, Registry};
use twitch_common::chat::twitch_chat::TwitchChat;

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
            let mut twitch = get_twitch_chat(&args.twitch).await?;
            let (chat_sender, chat_receiver) = tokio::sync::mpsc::unbounded_channel();
            let channel = args.twitch.channel.clone();
            tokio::spawn(async move {
                match twitch.run(chat_sender, channel).await {
                    Ok(()) => {}
                    Err(e) => tracing::error!(error = %e, "Twitch chat task exited with error"),
                }
            });

            let sqlite_pool =
                sql_common::setup::setup_sqlite_migrations(args.db_path.clone(), &MIGRATIONS)
                    .await?;
            let word_list = WordList::from(args.word_list.as_str());

            // Anathema's `runtime.run` blocks a thread for the whole session. Running it on a
            // Tokio worker can strand `tokio::spawn` tasks (e.g. `game_loop`, IRC) on that same
            // worker, so drive the TUI from the blocking pool instead.
            tokio::task::spawn_blocking(move || {
                crate::wordle::game::start_game(chat_receiver, word_list, sqlite_pool)
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

pub async fn get_twitch_chat(opts: &TwitchChatArgs) -> anyhow::Result<TwitchChat> {
    let chat = TwitchChat::new(
        opts.channel.clone(),
        opts.tc_username.clone(),
        opts.tc_password.clone(),
    )
    .await?;
    Ok(chat)
}
