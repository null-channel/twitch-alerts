use tmi::Privmsg;

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
                format!("{}:{}", message.sender().name(), message.text().clone())
            }
            Self::Debug(message) => format!("DEBUG: {}", message.clone()),
        }
    }
}
