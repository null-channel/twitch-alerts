use twitch_irc::message::PrivmsgMessage;

#[derive(Debug, Clone)]
pub enum Message {
    TwitchMessage(PrivmsgMessage),
    Debug(String),
}

impl Message {
    pub fn new_twitch_message(message: PrivmsgMessage) -> Self {
        Self::TwitchMessage(message)
    }

    pub fn new_debug_message(message: String) -> Self {
        Self::Debug(message)
    }

    pub fn text(&self) -> String {
        match self {
            Self::TwitchMessage(message) => {
                format!("{}:{}", message.sender.name, message.message_text.clone())
            }
            Self::Debug(message) => format!("DEBUG: {}", message.clone()),
        }
    }
}
