#[derive(Debug, Clone)]
pub enum Message {
    TwitchMessage(String),
    Debug(String),
}

impl Message {
    pub fn new_twitch_message(message: String) -> Self {
        Self::TwitchMessage(message)
    }

    pub fn new_debug_message(message: String) -> Self {
        Self::Debug(message)
    }

    pub fn text(&self) -> String {
        match self {
            Self::TwitchMessage(message) => message.clone(),
            Self::Debug(message) => format!("DEBUG: {}", message.clone()),
        }
    }
}
