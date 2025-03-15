use tokio::sync::mpsc::UnboundedSender;

use crate::messages::Message;

pub async fn run(
    mut client: tmi::Client,
    postman: UnboundedSender<Message>,
    channel: String,
) -> anyhow::Result<()> {
    loop {
        let msg = client.recv().await?;
        match msg.as_typed()? {
            tmi::Message::Privmsg(msg) => on_msg(&mut client, msg, &postman).await?,
            tmi::Message::Reconnect => {
                client.reconnect().await?;
                client.join(&channel).await?;
            }
            tmi::Message::Ping(ping) => client.pong(&ping).await?,
            _ => {}
        };
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
        postman.send(Message::new_twitch_message(msg.into_owned()));
    }
    Ok(())
}
