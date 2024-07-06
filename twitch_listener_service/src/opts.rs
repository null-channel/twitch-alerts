use clap::{builder::ArgPredicate, ArgGroup, Parser};

#[derive(Parser, Debug, Clone)]
#[clap(about, version,
    group = ArgGroup::new("token").multiple(false).required(false),
    //Why do these break the run???
    //group = ArgGroup::new("service").multiple(true).requires("oauth2-service-url"),
    group = ArgGroup::new("channel").multiple(true).required(false),
    //Why do these break the run???
    //group = ArgGroup::new("gpt").multiple(true).requires("gpt-key"),
)]
pub struct Opts {
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

    #[clap(long, env, hide_env = true, group = "gpt")]
    pub gpt_key: Option<String>,

    #[clap(long, env, hide_env = true, group = "db", default_value = "alerts.db")]
    pub db_path: Option<String>,

    #[clap(long, env, hide_env = true, default_value = "80")]
    pub http_port: String,

    #[clap(long, env, hide_env = true, default_value = "9000")]
    pub ws_port: String,
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
