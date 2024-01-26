mod messages;

use messages::Message;
use tokio::sync::mpsc;
use twitch_irc::irc;
use twitch_irc::login::StaticLoginCredentials;
use twitch_irc::message::{AsRawIRC, IRCMessage, PrivmsgMessage, ServerMessage};
use twitch_irc::TwitchIRCClient;
use twitch_irc::{ClientConfig, SecureTCPTransport};

// Anathema Stuff
use std::fs::read_to_string;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex};

use anathema::runtime::Runtime;
use anathema::vm::Templates;

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

    let (app_sender, app_receiver) = mpsc::unbounded_channel();
    //let (chat_sender, chat_receiver) = mpsc::unbounded_channel();
    let c = client.clone();
    let join_handle = tokio::spawn(async move {
        while let Some(message) = incoming_messages.recv().await {
            match message {
                ServerMessage::Privmsg(msg) => {
                    let _ = app_sender.send(Message::new_twitch_message(msg.message_text.clone()));
                    if msg.message_text == "!say_hello" {
                        let ircmsg = irc![
                            "PRIVMSG",
                            "#marekcounts",
                            "beep boop I am your friendly robot"
                        ];
                        let res = c.send_message(ircmsg).await;
                        match res {
                            Ok(_) => {
                                let _ = app_sender.send(Message::new_debug_message(
                                    "did not expect this".to_string(),
                                ));
                            }
                            Err(e) => {
                                let _ = app_sender.send(Message::new_debug_message(format!(
                                    "expected this: {}",
                                    e
                                )));
                            }
                        }
                    }
                }
                ServerMessage::Ping(_) => {
                    let _ =
                        app_sender.send(Message::new_debug_message("We got pinged".to_string()));
                }
                ServerMessage::Pong(_) => {
                    let _ =
                        app_sender.send(Message::new_debug_message("We got a Pong".to_string()));
                }
                ServerMessage::Notice(msg) => {
                    let _ = app_sender.send(Message::new_debug_message(format!(
                        "notice: {}",
                        msg.message_text
                    )));
                }
                ServerMessage::UserNotice(msg) => {
                    let _ = app_sender.send(Message::new_debug_message(format!(
                        "user notice: {}",
                        msg.system_message
                    )));
                }
                _ => {
                    let _ =
                        app_sender.send(Message::new_debug_message("other message".to_string()));
                }
            };
        }
    });

    // join a channel
    // This function only returns an error if the passed channel login name is malformed,
    // so in this simple case where the channel name is hardcoded we can ignore the potential
    // error with `unwrap`.
    client.join("marekcounts".to_owned()).unwrap();

    // Anathema tests
    // Step one: Load and compile templates
    let my_view = MyView();
    let template = read_to_string("index.aml").unwrap();
    let mut templates = Templates::new(template, my_view);
    let templates = templates.compile().unwrap();

    // Step two: Runtime
    let runtime = Runtime::new(&templates).unwrap();

    // Step three: start the runtime
    runtime.run().unwrap();

    // keep the tokio executor alive.
    // If you return instead of waiting the background task will exit.
    join_handle.await.unwrap();

    Ok(())
}

// TODO: delete this
use anathema::core::{Event, KeyCode, Nodes, View, Color};
use anathema::values::{State, StateValue, List};

#[derive(State)]
struct MyState {
    chats: List<String>,
}

#[derive(Clone)]
struct MyView {
    state: MyState,

}

impl MyView {
    fn new(state: MyState) -> Self {
        Self(state)
    }

    fn new_chat(&mut self, chat: String) {
        let mut state = self.0;
        state.chats.push(chat.clone());
    }
}   

impl View for MyView {
    fn on_event(&mut self, event: Event, _: &mut Nodes<'_>) -> Event {
        match event {
            Event::KeyPress(KeyCode::Up, ..) => {
                let mut state = self.0;
                state.chats.push("up".to_string());
                Event::Noop
            }
            _ => {
                Event::Noop
            }
        }
    }

    fn tick(&mut self) {
        let mut state = self.0.lock().unwrap();
        state.push("tick".to_string());
    }

    fn state(&self) -> &dyn State {
        let thing = self.0;
    }
}
