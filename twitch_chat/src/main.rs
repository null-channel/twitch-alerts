use twitch_irc::login::StaticLoginCredentials;
use twitch_irc::TwitchIRCClient;
use twitch_irc::{ClientConfig, SecureTCPTransport};
use twitch_irc::message::{AsRawIRC,ServerMessage, IRCMessage, PrivmsgMessage};
use twitch_irc::irc;

#[tokio::main]
pub async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();


    // default configuration is to join chat as anonymous.
    let user = std::env::var("TC_ID_USER")?;
    let pass = std::env::var("TC_TOKEN")?;
    let config = ClientConfig::new_simple(StaticLoginCredentials::new(user, Some(pass)));
    // let config = ClientConfig::default();
    let (mut incoming_messages, client) =
        TwitchIRCClient::<SecureTCPTransport, StaticLoginCredentials>::new(config);

    // first thing you should do: start consuming incoming messages,
    // otherwise they will back up.
    
    let c = client.clone();
    let join_handle = tokio::spawn(async move {
        while let Some(message) = incoming_messages.recv().await {

            match message {
                ServerMessage::Privmsg(msg) => {
                    println!("sender: {}, msg: {}", msg.sender.name, msg.message_text);

                    if msg.message_text == "!say_hello" {
                        let ircmsg = irc!["PRIVMSG", "#klavenx", "beep boop I am your friendly robot"];
                        let res = c.send_message(ircmsg).await;
                        match res {
                            
                            Ok(_) => println!("did not expect this"),
                            Err(e) => println!("expected this: {}", e),
                        }

                    }
                }
                ServerMessage::Ping(_) => println!("We got pinged"),
                ServerMessage::Pong(_) => println!("We got a pong?"),
                ServerMessage::Notice(msg) => println!("notice: {}", msg.message_text),
                ServerMessage::UserNotice(msg) => println!("user notice: {:?}", msg),
                _ => println!("other message"),
            };
        }
    });

    // join a channel
    // This function only returns an error if the passed channel login name is malformed,
    // so in this simple case where the channel name is hardcoded we can ignore the potential
    // error with `unwrap`.
    client.join("klavenx".to_owned()).unwrap();

    // keep the tokio executor alive.
    // If you return instead of waiting the background task will exit.
    join_handle.await.unwrap();

    Ok(())
}


fn deal_with_message(msg: PrivmsgMessage) {


}
