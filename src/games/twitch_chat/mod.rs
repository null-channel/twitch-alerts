use tmi::{irc, Privmsg};
use tokio::sync::mpsc::{self, UnboundedSender};

pub struct TwitchChat {
    pub client: tmi::Client,
    pub channel: String,
    pub sender: UnboundedSender<Privmsg<'static>>,
    pub receiver: mpsc::UnboundedReceiver<tmi::Privmsg<'static>>,
    //TODO: add a history of messages
    pub history: Vec<tmi::Privmsg<'static>>,
}

/// A struct representing a Twitch chat client.
/// It does not actually connect to the Twitch chat server until the `run` method is called.
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

    pub async fn run(
        &mut self,
        sender: UnboundedSender<Privmsg<'_>>,
        channel: String,
    ) -> anyhow::Result<()> {
        self.client.join(&channel).await?;

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
        Ok(())
    }
}

pub async fn on_msg(
    client: &mut tmi::Client,
    msg: tmi::Privmsg<'_>,
    postman: &UnboundedSender<tmi::Privmsg<'_>>,
) -> anyhow::Result<()> {
    if msg.text() == "!say_hello" {
        client
            .privmsg(msg.channel(), "beep boop I am your friendly robot")
            .reply_to(msg.id())
            .send()
            .await?;
    } else {
        let _ = postman.send(msg.into_owned());
    }
    Ok(())
}
