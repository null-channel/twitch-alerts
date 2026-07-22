use tmi::Privmsg;

use crate::chat::messages::MessagePlatform;

#[derive(Debug, Clone)]
pub enum Message {
    TwitchMessage(Privmsg<'static>),
    Debug(String),
}

impl Message {
    pub fn new_twitch_message(message: Privmsg<'static>) -> Self {
        Self::TwitchMessage(message)
    }

    pub fn new_debug_message(message: String) -> Self {
        Self::Debug(message)
    }

    pub fn text(&self) -> String {
        match self {
            Self::TwitchMessage(message) => {
                format!("{}:{}", message.sender().name(), message.text())
            }
            Self::Debug(message) => format!("DEBUG: {}", message.clone()),
        }
    }

    pub fn content(&self) -> String {
        match self {
            Self::TwitchMessage(message) => message.text().to_string(),
            Self::Debug(message) => message.clone(),
        }
    }

    pub fn platform(&self) -> MessagePlatform {
        match self {
            Self::TwitchMessage(message) => MessagePlatform::Twitch,
            Self::Debug(message) => MessagePlatform::Debug,
        }
    }

    pub fn platform_id(&self) -> String {
        match self {
            Self::TwitchMessage(message) => message.sender().id().into(),
            Self::Debug(_) => "debug id".into(),
        }
    }

    pub fn sender(&self) -> String {
        match self {
            Self::TwitchMessage(message) => message.sender().name().to_string(),
            Self::Debug(_) => "Debug".to_string(),
        }
    }
}
