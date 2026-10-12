use clap::Parser;

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

#[derive(clap::Args, Debug, Clone)]
pub struct Wordle {
    #[command(flatten)]
    pub twitch: TwitchChatArgs,
    #[arg(short, long, default_value = "standard", value_parser = ["standard", "challenge", "nerd"])]
    pub word_list: String,
    #[arg(
        short = 'd',
        long = "db-path",
        alias = "db-ags",
        env = "BRUCE_DB_PATH",
        default_value = "sqlite:games.db"
    )]
    pub db_path: String,
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
