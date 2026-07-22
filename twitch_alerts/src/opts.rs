use clap::{builder::ArgPredicate, Parser};
use random_word::WordList;

/*
* null-twitch cli
* TODO: add moderation options to mock cheap viewer ads
*
* twinkle server
* twinkle bot
* twinkle chat
* twinkle game
* twinkle game --mode=dnd
*
* bruce server
*
* tx server
* tx bot
* tx chat
* tx game
* tx game --mode=dnd
* */

#[derive(clap::Args, Debug, Clone)]
pub struct HttpServer {
    /// The host to run the server on
    #[clap(long, env, hide_env = true, default_value = "localhost")]
    pub host: String,
    /// The port to run the server on
    #[clap(long, env, hide_env = true, default_value = "8080")]
    pub port: u16,
}

#[derive(clap::Args, Debug, Clone)]
pub struct WSHttpServer {
    /// The host to run the server on
    #[clap(long, env, hide_env = true, default_value = "localhost")]
    pub ws_host: String,
    /// The port to run the server on
    #[clap(long, env, hide_env = true, default_value = "9000")]
    pub ws_port: u16,
}

#[derive(clap::Args, Debug, Clone)]
pub struct TwitchBotArgs {
    /// OAuth2 Access token
    #[clap(
        long,
        env,
        hide_env = true,
        group = "token",
        required_unless_present = "service"
    )]
    pub access_token: Option<String>,
    /// Name of channel to monitor. If left out, defaults to owner of access token.
    #[clap(long, env, hide_env = true, group = "channel")]
    pub channel_login: Option<String>,
    /// User ID of channel to monitor. If left out, defaults to owner of access token.
    #[clap(long, env, hide_env = true, group = "channel")]
    pub channel_id: Option<String>,
    /// URL to service that provides OAuth2 token. Called on start and whenever the token needs to be refreshed.
    #[clap(long, env, hide_env = true, group = "service",
        value_parser = url::Url::parse, required_unless_present = "token"
        )]
    pub oauth2_service_url: Option<url::Url>,
    /// Bearer key for authorizing on the OAuth2 service url.
    #[clap(long, env, hide_env = true, group = "service")]
    pub oauth2_service_key: Option<Secret>,
    /// Grab token by pointer. See https://tools.ietf.org/html/rfc6901
    #[clap(
        long,
        env,
        hide_env = true,
        group = "service",
        default_value_if("oauth2_service_url", ArgPredicate::IsPresent, Some("/access_token"))
    )]
    pub oauth2_service_pointer: Option<String>,
    /// Grab a new token from the OAuth2 service this many seconds before it actually expires. Default is 30 seconds
    #[clap(
        long,
        env,
        hide_env = true,
        group = "service",
        default_value_if("oauth2_service_url", ArgPredicate::IsPresent, Some("30"))
    )]
    pub oauth2_service_refresh: Option<u64>,
}

#[derive(Parser, Debug, Clone)]
#[clap(name = "null-twitch")]
#[clap(about = "Marek's Great Twitch Tool", long_about = None)]
pub struct Cli {
    #[clap(subcommand)]
    pub command: Commands,
}

#[derive(clap::Subcommand, Debug, Clone)]
pub enum Commands {
    Server(ServerArgs),
    Chat,
    Bot,
    #[command(subcommand)]
    Game(GamesSubCommand),
}

#[derive(clap::Args, Debug, Clone)]
pub struct LLMArgs {
    #[clap(flatten)]
    pub openai: OpenAIArgs,
}

#[derive(clap::Args, Debug, Clone)]
pub struct OpenAIArgs {
    #[clap(long, env, hide_env = true)]
    pub openai_key: Option<String>,
    #[clap(long, env, hide_env = true)]
    pub openai_model: Option<String>,
}

#[derive(clap::Args, Debug, Clone)]
pub struct ServerArgs {
    /// The Twitch API client
    #[clap(flatten)]
    pub twitch: TwitchBotArgs,

    /// The HTTP server
    #[clap(flatten)]
    pub http: HttpServer,

    /// The Websocket Server
    #[clap(flatten)]
    pub ws: WSHttpServer,

    /// The LLM to use
    #[clap(flatten)]
    pub llm: LLMArgs,
    /// The path to the database file
    #[clap(long, env, hide_env = true, default_value = "alerts.db")]
    pub db_path: Option<String>,
    /// The path to the frontend assets
    #[clap(long, env, hide_env = true, default_value = "frontend_api/assets")]
    pub frontend_assets: String,
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
    #[arg(short, long, default_value = "standard")]
    pub word_list: String,
    #[arg(
        short,
        long,
        default_value = "sqlite:src/games/wordle/wordle_scoreboard.db"
    )]
    pub db_ags: String,
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

pub fn is_token(s: String) -> eyre::Result<()> {
    if s.starts_with("oauth:") {
        eyre::bail!("token should not have `oauth:` as a prefix")
    }
    if s.len() != 30 {
        eyre::bail!("token needs to be 30 characters long")
    }
    Ok(())
}

#[derive(Clone)]
pub struct Secret(String);

impl Secret {
    pub fn secret(&self) -> &str {
        &self.0
    }
}

impl std::str::FromStr for Secret {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_string()))
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[secret]")
    }
}
