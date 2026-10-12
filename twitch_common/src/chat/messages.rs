use tmi::Privmsg;

use std::convert::TryFrom;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MessagePlatform {
    Twitch,
    Youtube,
    Discord,
    Debug,
}

#[derive(Debug, Clone)]
pub enum Message {
    TwitchMessage(Box<Privmsg<'static>>),
    Debug(String),
}

impl Message {
    pub fn new_twitch_message(message: Privmsg<'static>) -> Self {
        Self::TwitchMessage(Box::new(message))
    }

    pub fn new_debug_message(message: String) -> Self {
        Self::Debug(message)
    }

    pub fn text(&self) -> String {
        match self {
            Self::TwitchMessage(message) => {
                format!("{}:{}", message.sender().name(), message.text())
            }
            Self::Debug(message) => format!("DEBUG: {}", message),
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
            Self::TwitchMessage(_) => MessagePlatform::Twitch,
            Self::Debug(_) => MessagePlatform::Debug,
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
