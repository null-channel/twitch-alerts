mod app;
mod messages;

use messages::Message;
use tokio::sync::mpsc;
use twitch_irc::login::StaticLoginCredentials;
use twitch_irc::TwitchIRCClient;
use twitch_irc::{ClientConfig, SecureTCPTransport};
use twitch_irc::message::{AsRawIRC,ServerMessage, IRCMessage, PrivmsgMessage};
use twitch_irc::irc;

// Tui Stuff
use ratatui::prelude::Rect;
use crossterm::{
    event::{self, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{
    prelude::{CrosstermBackend, Stylize, Terminal},
    widgets::Paragraph,
};
use std::io::{stdout, Result};


#[tokio::main]
pub async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();


    // default configuration is to join chat as anonymous.
    let user = std::env::var("TC_USER")?;
    let pass = std::env::var("TC_PASS")?;
    let config = ClientConfig::new_simple(StaticLoginCredentials::new(user, Some(pass)));
    // let config = ClientConfig::default();
    let (mut incoming_messages, client) =
        TwitchIRCClient::<SecureTCPTransport, StaticLoginCredentials>::new(config);

    // first thing you should do: start consuming incoming messages,
    // otherwise they will back up.
   
    let (app_sender,app_receiver) = mpsc::unbounded_channel();
    let (chat_sender,chat_receiver) = mpsc::unbounded_channel();
    let c = client.clone();
    let join_handle = tokio::spawn(async move {
        while let Some(message) = incoming_messages.recv().await {

            match message {
                ServerMessage::Privmsg(msg) => {
                    let _ = app_sender.send(Message::new_twitch_message(msg.message_text.clone()));
                    if msg.message_text == "!say_hello" {
                        let ircmsg = irc!["PRIVMSG", "#marekcounts", "beep boop I am your friendly robot"];
                        let res = c.send_message(ircmsg).await;
                        match res {
                            Ok(_) => {
                                let _ = app_sender.send(Message::new_debug_message("did not expect this".to_string()));
                            }
                            Err(e) => { 
                                let _ = app_sender.send(Message::new_debug_message(format!("expected this: {}", e)));
                            }
                        }

                    }
                }
                ServerMessage::Ping(_) => {
                    let _ = app_sender.send(Message::new_debug_message("We got pinged".to_string()));
                }
                ServerMessage::Pong(_) => {
                    let _ = app_sender.send(Message::new_debug_message("We got a Pong".to_string()));
                }
                ServerMessage::Notice(msg) => { 
                    let _ = app_sender.send(Message::new_debug_message(format!("notice: {}", msg.message_text)));
                }
                ServerMessage::UserNotice(msg) => {
                    let _ = app_sender.send(Message::new_debug_message(format!("user notice: {}", msg.system_message)));
                }
                _ => {
                    let _ = app_sender.send(Message::new_debug_message("other message".to_string()));
                }
            };
        }
    });

    // join a channel
    // This function only returns an error if the passed channel login name is malformed,
    // so in this simple case where the channel name is hardcoded we can ignore the potential
    // error with `unwrap`.
    client.join("marekcounts".to_owned()).unwrap();
    let mut app = app::App::new(app_receiver, chat_sender);
    // Tui Stuff
    let tui_handle = tokio::spawn(async move {
        let _ = app.start();
    });

    // keep the tokio executor alive.
    // If you return instead of waiting the background task will exit.
    join_handle.await.unwrap();

    Ok(())
}


