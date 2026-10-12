use std::time::Duration;

use clap::Parser;
use twitch_common::chat::twitch_chat::TwitchChatConfig;
use twitch_common::chat::youtube_chat::{Broadcast, YoutubeChatConfig};

#[derive(Parser, Debug, Clone)]
#[clap(name = "bruce", version)]
#[clap(about = "Bruce likes to play games with Twitch and YouTube chat", long_about = None)]
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
    pub chat: ChatArgs,
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
    pub chat: ChatArgs,
    pub dragons: u32,
}

/// The chat platforms a game listens to.
///
/// Each platform is enabled by setting all of its options and left out by setting none, so a
/// game can run on Twitch, on YouTube, or on both at once. At least one is required.
#[derive(clap::Args, Debug, Clone)]
pub struct ChatArgs {
    #[command(flatten)]
    pub twitch: TwitchChatArgs,
    #[command(flatten)]
    pub youtube: YoutubeChatArgs,
}

/// The connection settings for one enabled platform. Add a variant per supported platform.
#[derive(Debug, Clone)]
pub enum PlatformConfig {
    Twitch(TwitchChatConfig),
    Youtube(YoutubeChatConfig),
}

impl ChatArgs {
    /// Every platform that is fully configured, in a fixed order.
    ///
    /// Fails if no platform is configured, or if one is only partially configured, naming
    /// what is missing so the fix is obvious.
    pub fn platforms(&self) -> anyhow::Result<Vec<PlatformConfig>> {
        let mut platforms = Vec::new();
        if let Some(config) = self.twitch.config()? {
            platforms.push(PlatformConfig::Twitch(config));
        }
        if let Some(config) = self.youtube.config()? {
            platforms.push(PlatformConfig::Youtube(config));
        }
        if platforms.is_empty() {
            anyhow::bail!(
                "no chat platform configured: set TC_USERNAME, TC_PASSWORD and CHANNEL for \
                 Twitch and/or YT_API_KEY with YT_VIDEO_ID or YT_CHANNEL_ID for YouTube"
            );
        }
        Ok(platforms)
    }
}

#[derive(clap::Args, Debug, Clone, Default)]
#[command(next_help_heading = "Twitch chat")]
pub struct TwitchChatArgs {
    /// Twitch bot username; enables Twitch together with --tc-password and --channel
    #[clap(long, short = 'u', env, hide_env = true)]
    pub tc_username: Option<String>,
    /// Twitch chat OAuth token for the bot user
    #[clap(long, short = 'p', env, hide_env = true, hide_env_values = true)]
    pub tc_password: Option<String>,
    /// The Twitch channel to join
    #[clap(long, env, hide_env = true)]
    pub channel: Option<String>,
}

impl TwitchChatArgs {
    pub fn config(&self) -> anyhow::Result<Option<TwitchChatConfig>> {
        let missing: Vec<&str> = [
            ("TC_USERNAME", self.tc_username.is_some()),
            ("TC_PASSWORD", self.tc_password.is_some()),
            ("CHANNEL", self.channel.is_some()),
        ]
        .iter()
        .filter(|(_, set)| !set)
        .map(|(name, _)| *name)
        .collect();
        match (&self.tc_username, &self.tc_password, &self.channel) {
            (Some(username), Some(password), Some(channel)) => Ok(Some(TwitchChatConfig {
                channel: channel.clone(),
                username: username.clone(),
                password: password.clone(),
            })),
            (None, None, None) => Ok(None),
            _ => anyhow::bail!(
                "Twitch chat is only partly configured (missing {}); set all of TC_USERNAME, \
                 TC_PASSWORD and CHANNEL, or none of them",
                missing.join(", ")
            ),
        }
    }
}

#[derive(clap::Args, Debug, Clone)]
#[command(next_help_heading = "YouTube live chat")]
pub struct YoutubeChatArgs {
    /// YouTube Data API v3 key; enables YouTube together with --yt-video-id or --yt-channel-id
    #[clap(long, env = "YT_API_KEY", hide_env = true, hide_env_values = true)]
    pub yt_api_key: Option<String>,
    /// The live broadcast to read chat from (the `v=` parameter of its watch URL)
    #[clap(long, env = "YT_VIDEO_ID", hide_env = true)]
    pub yt_video_id: Option<String>,
    /// The channel whose current live broadcast to read; used when --yt-video-id is unset
    #[clap(long, env = "YT_CHANNEL_ID", hide_env = true)]
    pub yt_channel_id: Option<String>,
    /// Minimum milliseconds between YouTube chat polls; each poll costs 5 API quota units
    #[clap(
        long,
        env = "YT_POLL_INTERVAL_MS",
        hide_env = true,
        default_value_t = 5000
    )]
    pub yt_poll_interval_ms: u64,
}

impl YoutubeChatArgs {
    pub fn config(&self) -> anyhow::Result<Option<YoutubeChatConfig>> {
        let broadcast = self
            .yt_video_id
            .clone()
            .map(Broadcast::VideoId)
            .or_else(|| self.yt_channel_id.clone().map(Broadcast::ChannelId));
        match (&self.yt_api_key, broadcast) {
            (Some(api_key), Some(broadcast)) => Ok(Some(YoutubeChatConfig {
                api_key: api_key.clone(),
                broadcast,
                min_poll_interval: Duration::from_millis(self.yt_poll_interval_ms),
            })),
            (None, None) => Ok(None),
            (Some(_), None) => anyhow::bail!(
                "YouTube live chat is only partly configured: YT_API_KEY is set but neither \
                 YT_VIDEO_ID nor YT_CHANNEL_ID says which broadcast to join"
            ),
            (None, Some(_)) => anyhow::bail!(
                "YouTube live chat is only partly configured: a broadcast is set but YT_API_KEY \
                 is missing"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn youtube(
        api_key: Option<&str>,
        video: Option<&str>,
        channel: Option<&str>,
    ) -> YoutubeChatArgs {
        YoutubeChatArgs {
            yt_api_key: api_key.map(str::to_string),
            yt_video_id: video.map(str::to_string),
            yt_channel_id: channel.map(str::to_string),
            yt_poll_interval_ms: 7000,
        }
    }

    fn twitch(
        username: Option<&str>,
        password: Option<&str>,
        channel: Option<&str>,
    ) -> TwitchChatArgs {
        TwitchChatArgs {
            tc_username: username.map(str::to_string),
            tc_password: password.map(str::to_string),
            channel: channel.map(str::to_string),
        }
    }

    #[test]
    fn nothing_configured_is_an_error_that_names_every_platform() {
        let args = ChatArgs {
            twitch: twitch(None, None, None),
            youtube: youtube(None, None, None),
        };
        let error = args.platforms().unwrap_err().to_string();
        assert!(error.contains("TC_USERNAME"), "{}", error);
        assert!(error.contains("YT_API_KEY"), "{}", error);
    }

    #[test]
    fn partial_twitch_configuration_names_the_missing_settings() {
        let error = twitch(Some("bot"), None, Some("chan"))
            .config()
            .unwrap_err()
            .to_string();
        assert!(error.contains("missing TC_PASSWORD"), "{}", error);
        assert!(error.contains("(missing TC_PASSWORD)"), "{}", error);
    }

    #[test]
    fn partial_youtube_configuration_is_an_error() {
        assert!(youtube(Some("key"), None, None).config().is_err());
        assert!(youtube(None, Some("video"), None).config().is_err());
        assert!(youtube(None, None, Some("UC1")).config().is_err());
    }

    #[test]
    fn video_id_wins_over_channel_id() {
        let config = youtube(Some("key"), Some("video"), Some("UC1"))
            .config()
            .unwrap()
            .unwrap();
        assert_eq!(config.broadcast, Broadcast::VideoId("video".into()));
        assert_eq!(config.api_key, "key");
        assert_eq!(config.min_poll_interval, Duration::from_millis(7000));
        let config = youtube(Some("key"), None, Some("UC1"))
            .config()
            .unwrap()
            .unwrap();
        assert_eq!(config.broadcast, Broadcast::ChannelId("UC1".into()));
    }

    #[test]
    fn both_platforms_are_listed_twitch_first() {
        let args = ChatArgs {
            twitch: twitch(Some("bot"), Some("oauth:x"), Some("chan")),
            youtube: youtube(Some("key"), Some("video"), None),
        };
        let platforms = args.platforms().unwrap();
        assert_eq!(platforms.len(), 2);
        assert!(
            matches!(&platforms[0], PlatformConfig::Twitch(config) if config.channel == "chan")
        );
        assert!(matches!(&platforms[1], PlatformConfig::Youtube(_)));

        let youtube_only = ChatArgs {
            twitch: twitch(None, None, None),
            youtube: youtube(Some("key"), Some("video"), None),
        };
        assert!(matches!(
            youtube_only.platforms().unwrap().as_slice(),
            [PlatformConfig::Youtube(_)]
        ));
    }
}
