use std::fmt;

use clap::{Parser, ValueEnum};
use strum_macros::AsRefStr;

#[derive(Parser, Debug, Clone)]
#[clap(name = "bruce", version)]
#[clap(about = "Bruce likes to play games with twitch chat", long_about = None)]
pub struct Cli {
    #[clap(subcommand)]
    pub command: Commands,
}

#[derive(clap::Subcommand, Debug, Clone)]
pub enum Commands {
    #[command(subcommand)]
    Game(GamesSubCommand),
}

#[derive(clap::Subcommand, Debug, Clone)]
pub enum GamesSubCommand {
    #[command()]
    Wordle(Wordle),
    #[command()]
    Dragons(Dragons),
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug, AsRefStr)]
pub enum WordListSelection {
    Standard,
    Challenge,
    Nerd,
}

#[derive(clap::Args, Debug, Clone)]
pub struct Wordle {
    #[command(flatten)]
    pub twitch: TwitchChatArgs,
    #[arg(short, long, default_value_t = WordListSelection::Standard, value_enum, ignore_case = true)]
    pub word_list: WordListSelection,
    #[arg(
        short = 'd',
        long = "db-path",
        alias = "db-ags",
        env = "BRUCE_DB_PATH",
        default_value = "sqlite:games.db"
    )]
    pub db_path: String,
    #[arg(short = 't')]
    pub test_word: Option<String>,
}

#[derive(clap::Args, Debug, Clone)]
pub struct Dragons {
    #[command(flatten)]
    pub twitch: TwitchChatArgs,
    pub dragons: u32,
}

#[derive(clap::Args, Debug, Clone)]
pub struct TwitchChatArgs {
    #[clap(long, short = 'u', env, hide_env = true)]
    pub tc_username: String,
    #[clap(long, short = 'p', env, hide_env = true, hide_env_values = true)]
    pub tc_password: String,
    /// The channel to connect to
    #[clap(long, env, hide_env = true)]
    pub channel: String,
}
