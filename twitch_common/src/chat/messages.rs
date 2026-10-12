use tmi::Privmsg;

use std::convert::TryFrom;
use std::fmt;

impl TryFrom<u8> for MessagePlatform {
    type Error = ();

    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            x if x == MessagePlatform::Twitch as u8 => Ok(MessagePlatform::Twitch),
            x if x == MessagePlatform::Youtube as u8 => Ok(MessagePlatform::Youtube),
            x if x == MessagePlatform::Discord as u8 => Ok(MessagePlatform::Discord),
            x if x == MessagePlatform::Debug as u8 => Ok(MessagePlatform::Debug),
            _ => Err(()),
        }
    }
}

/// The platform a chat message came from.
///
/// The discriminant is persisted (scoreboards key players by `(platform, platform_user_id)`)
/// and indexes the colour palette in the chat UI, so only append new platforms at the end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MessagePlatform {
    Twitch,
    Youtube,
    Discord,
    Debug,
}

impl MessagePlatform {
    /// Short lowercase name used in logs, configuration and the UI.
    pub fn name(self) -> &'static str {
        match self {
            Self::Twitch => "twitch",
            Self::Youtube => "youtube",
            Self::Discord => "discord",
            Self::Debug => "debug",
        }
    }
}

impl fmt::Display for MessagePlatform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A message from YouTube live chat, reduced to what games need.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YoutubeChatMessage {
    /// YouTube's id for the chat message.
    pub id: String,
    /// The author's YouTube channel id; stable across display-name changes.
    pub author_channel_id: String,
    /// The author's current display name.
    pub author_name: String,
    /// The message as YouTube displays it.
    pub text: String,
}

#[derive(Debug, Clone)]
pub enum Message {
    TwitchMessage(Box<Privmsg<'static>>),
    Youtube(YoutubeChatMessage),
    Debug(String),
}

impl Message {
    pub fn new_twitch_message(message: Privmsg<'static>) -> Self {
        Self::TwitchMessage(Box::new(message))
    }

    pub fn new_youtube_message(message: YoutubeChatMessage) -> Self {
        Self::Youtube(message)
    }

    pub fn new_debug_message(message: String) -> Self {
        Self::Debug(message)
    }

    pub fn text(&self) -> String {
        match self {
            Self::TwitchMessage(message) => {
                format!("{}:{}", message.sender().name(), message.text())
            }
            Self::Youtube(message) => format!("{}:{}", message.author_name, message.text),
            Self::Debug(message) => format!("DEBUG: {}", message),
        }
    }

    pub fn content(&self) -> String {
        match self {
            Self::TwitchMessage(message) => message.text().to_string(),
            Self::Youtube(message) => message.text.clone(),
            Self::Debug(message) => message.clone(),
        }
    }

    pub fn platform(&self) -> MessagePlatform {
        match self {
            Self::TwitchMessage(_) => MessagePlatform::Twitch,
            Self::Youtube(_) => MessagePlatform::Youtube,
            Self::Debug(_) => MessagePlatform::Debug,
        }
    }

    /// The sender's id on their platform. Unique within a platform, not across platforms.
    pub fn platform_id(&self) -> String {
        match self {
            Self::TwitchMessage(message) => message.sender().id().into(),
            Self::Youtube(message) => message.author_channel_id.clone(),
            Self::Debug(_) => "debug id".into(),
        }
    }

    pub fn sender(&self) -> String {
        match self {
            Self::TwitchMessage(message) => message.sender().name().to_string(),
            Self::Youtube(message) => message.author_name.clone(),
            Self::Debug(_) => "Debug".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_discriminants_round_trip() {
        for platform in [
            MessagePlatform::Twitch,
            MessagePlatform::Youtube,
            MessagePlatform::Discord,
            MessagePlatform::Debug,
        ] {
            assert_eq!(MessagePlatform::try_from(platform as u8), Ok(platform));
        }
        assert_eq!(MessagePlatform::Youtube as u8, 1);
        assert_eq!(MessagePlatform::try_from(200), Err(()));
    }

    #[test]
    fn youtube_messages_expose_author_and_text() {
        let message = Message::new_youtube_message(YoutubeChatMessage {
            id: "msg".into(),
            author_channel_id: "UC123".into(),
            author_name: "Viewer".into(),
            text: "crane".into(),
        });
        assert_eq!(message.platform(), MessagePlatform::Youtube);
        assert_eq!(message.platform_id(), "UC123");
        assert_eq!(message.sender(), "Viewer");
        assert_eq!(message.content(), "crane");
        assert_eq!(message.text(), "Viewer:crane");
    }
}
