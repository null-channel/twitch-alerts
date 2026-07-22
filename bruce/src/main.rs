#![warn(clippy::unwrap_in_result)]
/// Mods
mod wordle;
mod opts;
mod utils;

use twitch_common::chat::twitch_chat::TwitchChat;
use clap::Parser;
use opts::{Cli, Commands, GamesSubCommand, TwitchChatArgs};
use random_word::WordList;


use tokio::{
    task::JoinHandle,
};

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
            let channel = args.twitch.channel.clone();
            tokio::spawn(async move {
                match twitch.run(chat_sender, channel).await {
                    Ok(()) => {}
                    Err(e) => tracing::error!(error = %e, "Twitch chat task exited with error"),
                }
            });

            let sqlite_pool = sql_common::setup::setup_sqlite_migrations(
                args.db_ags.clone(),
                sql_common::setup::get_migrations_from_cargo_dir("migrations")?,
            )
            .await?;
            let word_list = WordList::from(args.word_list.as_str());

            // Anathema's `runtime.run` blocks a thread for the whole session. Running it on a
            // Tokio worker can strand `tokio::spawn` tasks (e.g. `game_loop`, IRC) on that same
            // worker, so drive the TUI from the blocking pool instead.
            tokio::task::spawn_blocking(move || {
                crate::wordle::game::start_game(chat_receiver, word_list, sqlite_pool)
            })
            .await
            .map_err(|e| anyhow::anyhow!("Wordle TUI thread panicked or was cancelled: {e}"))?;
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

async fn flatten<T>(handle: JoinHandle<anyhow::Result<T>>) -> anyhow::Result<T> {
    match handle.await {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(err)) => Err(err),
        Err(e) => Err(anyhow::anyhow!(e)),
    }
}
