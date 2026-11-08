use clap::{builder::ArgPredicate, Parser};
use std::{net::SocketAddr, path::PathBuf};
#[derive(clap::Args, Debug, Clone)]
pub struct HttpServer {
    /// The host to run the server on
    #[clap(long, env = "HTTP_HOST", hide_env = true, default_value = "0.0.0.0")]
    pub host: String,
    /// The port to run the server on
    #[clap(long, env = "HTTP_PORT", hide_env = true, default_value = "8080")]
    pub port: u16,
}

#[derive(clap::Args, Debug, Clone)]
pub struct WSHttpServer {
    /// The host browsers use for the WebSocket connection
    #[clap(
        long,
        env = "WEBSOCKET_HOST",
        hide_env = true,
        default_value = "localhost"
    )]
    pub ws_host: String,
    /// The port to run the server on
    #[clap(long, env = "WEBSOCKET_PORT", hide_env = true, default_value = "9000")]
    pub ws_port: u16,
}

#[derive(clap::Args, Debug, Clone)]
pub struct TwitchBotArgs {
    #[clap(flatten)]
    pub oauth: OAuthArgs,
    /// OAuth2 Access token
    #[clap(
        long,
        env,
        hide_env = true,
        hide_env_values = true,
        conflicts_with = "oauth2_service_url"
    )]
    pub access_token: Option<String>,
    /// Name of channel to monitor. If left out, defaults to owner of access token.
    #[clap(long, env, hide_env = true, group = "channel")]
    pub channel_login: Option<String>,
    /// User ID of channel to monitor. If left out, defaults to owner of access token.
    #[clap(long, env, hide_env = true, group = "channel")]
    pub channel_id: Option<String>,
    /// URL to service that provides OAuth2 token. Called on start and whenever the token needs to be refreshed.
    #[clap(long, env, hide_env = true, value_parser = url::Url::parse)]
    pub oauth2_service_url: Option<url::Url>,
    /// Bearer key for authorizing on the OAuth2 service url.
    #[clap(long, env, hide_env = true, requires = "oauth2_service_url")]
    pub oauth2_service_key: Option<Secret>,
    /// Grab token by pointer. See https://tools.ietf.org/html/rfc6901
    #[clap(
        long,
        env,
        hide_env = true,
        requires = "oauth2_service_url",
        default_value_if("oauth2_service_url", ArgPredicate::IsPresent, Some("/access_token"))
    )]
    pub oauth2_service_pointer: Option<String>,
    /// Grab a new token from the OAuth2 service this many seconds before it actually expires. Default is 30 seconds
    #[clap(
        long,
        env,
        hide_env = true,
        requires = "oauth2_service_url",
        default_value_if("oauth2_service_url", ArgPredicate::IsPresent, Some("30"))
    )]
    pub oauth2_service_refresh: Option<u64>,
}

#[derive(Parser, Debug, Clone)]
#[clap(name = "twitch_alerts", version)]
#[clap(about = "Twitch alerts server", long_about = None)]
pub struct Cli {
    #[clap(subcommand)]
    pub command: Commands,
}

#[derive(clap::Subcommand, Debug, Clone)]
pub enum Commands {
    Server(Box<ServerArgs>),
    /// Authorize Twitch once and save refreshable credentials for the server.
    Authorize(AuthorizeArgs),
}

#[derive(clap::Args, Debug, Clone)]
pub struct OAuthArgs {
    /// Client ID of your own registered Twitch application.
    #[clap(long, env, hide_env = true)]
    pub twitch_client_id: Option<String>,
    /// Client secret of the same Twitch application (kept out of the token file).
    #[clap(long, env, hide_env = true, hide_env_values = true)]
    pub twitch_client_secret: Option<Secret>,
    /// Persistent, private file holding Twitch access and refresh tokens.
    #[clap(long, env = "TWITCH_TOKEN_FILE", default_value = "twitch-tokens.json")]
    pub token_file: PathBuf,
}

#[derive(clap::Args, Debug, Clone)]
pub struct AuthorizeArgs {
    #[clap(flatten)]
    pub oauth: OAuthArgs,
    /// Must exactly match a redirect URL registered in the Twitch console.
    #[clap(
        long,
        env = "TWITCH_REDIRECT_URL",
        default_value = "http://localhost:3000/auth/twitch/callback"
    )]
    pub redirect_url: url::Url,
    /// Local callback listener; use an SSH tunnel or HTTPS reverse proxy remotely.
    #[clap(long, default_value = "127.0.0.1:3000")]
    pub listen_addr: SocketAddr,
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
    /// Chat Completions model supporting reasoning_effort=none for fast alert stories.
    #[clap(long, env, hide_env = true, default_value = "gpt-6-luna", value_parser = clap::builder::NonEmptyStringValueParser::new())]
    pub openai_model: String,
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
    #[clap(long, env, hide_env = true, default_value = concat!(env!("CARGO_MANIFEST_DIR"), "/src/frontend/assets"))]
    pub frontend_assets: String,
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
