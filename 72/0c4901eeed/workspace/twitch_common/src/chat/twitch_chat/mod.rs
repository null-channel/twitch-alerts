use std::future::Future;
use std::pin::Pin;

use tmi::Privmsg;
use tokio::sync::mpsc::{self, UnboundedSender};

use crate::chat::messages::{Message, MessagePlatform};
use crate::chat::source::ChatSource;

/// Everything needed to join one Twitch channel as a bot user.
#[derive(Debug, Clone)]
pub struct TwitchChatConfig {
    /// The channel to join, with or without the leading `#`.
    pub channel: String,
    /// The bot's Twitch login.
    pub username: String,
    /// The bot's chat OAuth token.
    pub password: String,
}

pub struct TwitchChat {
    pub client: tmi::Client,
    pub channel: String,
    pub sender: UnboundedSender<Privmsg<'static>>,
    pub receiver: mpsc::UnboundedReceiver<tmi::Privmsg<'static>>,
    //TODO: add a history of messages
    pub history: Vec<tmi::Privmsg<'static>>,
}

/// A struct representing a Twitch chat client.
/// It connects to Twitch in `new` but only joins `channel` once `run` is called.
impl TwitchChat {
    pub async fn new(channel: String, username: String, password: String) -> anyhow::Result<Self> {
        let creds = tmi::client::Credentials::new(username, password);

        println!("Connecting as {}", creds.login());
        let client = tmi::Client::builder().credentials(creds).connect().await?;

        let (sender, receiver) = mpsc::unbounded_channel();
        Ok(Self {
            client,
            channel,
            sender,
            receiver,
            history: Vec::new(),
        })
    }

    pub async fn connect(config: TwitchChatConfig) -> anyhow::Result<Self> {
        Self::new(config.channel, config.username, config.password).await
    }

    /// Join the configured channel and forward its messages until the connection fails.
    pub async fn run(&mut self, sender: UnboundedSender<Message>) -> anyhow::Result<()> {
        let channel = self.channel.clone();
        self.client.join(&channel).await?;

        loop {
            let message = self.client.recv().await;
            let msg = message?;
            match msg.as_typed()? {
                tmi::Message::Privmsg(msg) => {
                    let msg = msg.into_owned();
                    self.history.push(msg.clone());
                    on_msg(&mut self.client, msg, &sender).await?;
                }
                tmi::Message::Reconnect => {
                    self.client.reconnect().await?;
                    self.client.join(&channel).await?;
                }
                tmi::Message::Ping(ping) => self.client.pong(&ping).await?,
                _ => {}
            };
        }
    }
}

impl ChatSource for TwitchChat {
    fn platform(&self) -> MessagePlatform {
        MessagePlatform::Twitch
    }

    fn name(&self) -> String {
        format!("twitch:#{}", self.channel.trim_start_matches('#'))
    }

    fn run(
        mut self: Box<Self>,
        sender: UnboundedSender<Message>,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>> {
        Box::pin(async move { TwitchChat::run(&mut self, sender).await })
    }
}

pub async fn on_msg(
    client: &mut tmi::Client,
    msg: tmi::Privmsg<'_>,
    postman: &UnboundedSender<Message>,
) -> anyhow::Result<()> {
    if msg.text() == "!say_hello" {
        client
            .privmsg(msg.channel(), "beep boop I am your friendly robot")
            .reply_to(msg.id())
            .send()
            .await?;
    } else {
        let _ = postman.send(Message::new_twitch_message(msg.into_owned()));
    }
    Ok(())
}
