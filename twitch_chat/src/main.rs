mod messages;
mod twitch;

use anathema::values::hashmap::HashMap;
use messages::Message;
use rand::Rng;
use tokio::sync::mpsc::{self, UnboundedSender};
use twitch_irc::irc;
use twitch_irc::login::StaticLoginCredentials;
use twitch_irc::message::{AsRawIRC, IRCMessage, PrivmsgMessage, ServerMessage, TwitchUserBasics};
use twitch_irc::TwitchIRCClient;
use twitch_irc::{ClientConfig, SecureTCPTransport};

// Anathema Stuff
use std::fs::read_to_string;
use std::hash::RandomState;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex};
use std::time;

use anathema::runtime::Runtime;
use anathema::vm::Templates;

#[tokio::main]
pub async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    // default configuration is to join chat as anonymous.
    let user = std::env::var("TC_USER")?;
    let pass = std::env::var("TC_PASS")?;

    let creds = tmi::client::Credentials::new(user, pass);

    println!("Connecting as {}", creds.login());
    let mut client = tmi::Client::builder().credentials(creds).connect().await?;

    client.join("marekcounts".to_string()).await?;

    let (app_sender, app_receiver) = mpsc::unbounded_channel();
    let join_handle = tokio::spawn(async move {
        twitch::run(client, app_sender, "marekcounts".to_string())
            .await
            .unwrap();
    });

    // Anathema tests
    // Step one: Load and compile templates
    let state = MyState {
        chats: List::new(vec!["Start".to_string()]),
    };
    let my_view = MyView::new(state, app_receiver);
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

struct WordleGame {
    pub messages: Vec<String>,
    pub game_round: GameRound,
    pub the_word: String,
}

impl WordleGame {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            game_round: GameRound {
                round_number: 0,
                players_votes: HashMap::new(),
                end_time: time::Instant::now(),
            },
            the_word: String::new(),
        }
    }

    pub async fn run(&mut self) {
        loop {
            // check if the round is over
            if self.game_round.end_time < time::Instant::now() {
                self.end_round();
            }
            tokio::time::sleep(time::Duration::from_millis(13)).await;
        }
    }

    pub fn start_round(&mut self, round: GameRound) {
        self.game_round.round_number += 1;
        self.game_round.players_votes.clear();
    }

    fn get_next_round(&self) -> GameRound {
        let random_time = rand::thread_rng().gen_range(10..25);
        GameRound {
            round_number: self.game_round.round_number + 1,
            players_votes: HashMap::new(),
            end_time: time::Instant::now() + time::Duration::from_secs(random_time),
        }
    }

    pub fn end_round(&mut self) {
        // pick a winning word

        // announce the winner
        //

        // update the UI to show the winning word
        //

        // show what characters where right, wrong, or in the wrong place

        // start a new round
    }
}

struct GameRound {
    pub round_number: u32,
    // pub players_votes: HashMap<name, vote>
    pub players_votes: HashMap<TwitchUserBasics, String>,
    pub end_time: time::Instant,
}

// TODO: delete this
use anathema::core::{Color, Event, KeyCode, Nodes, View};
use anathema::values::{List, State, StateValue};

#[derive(State)]
struct MyState {
    chats: List<String>,
}

struct MyView {
    state: MyState,
    message_receiver: mpsc::UnboundedReceiver<Message>,
}

impl MyView {
    fn new(state: MyState, message_receiver: mpsc::UnboundedReceiver<Message>) -> Self {
        Self {
            state,
            message_receiver,
        }
    }

    fn new_chat(&mut self, chat: String) {
        self.state.chats.push_back(chat);
    }
}

impl View for MyView {
    fn on_event(&mut self, event: Event, _: &mut Nodes<'_>) -> Event {
        match event {
            Event::KeyPress(KeyCode::Up, ..) => {
                self.state.chats.push_back("up".to_string());
                Event::Noop
            }
            _ => Event::Noop,
        }
    }

    fn tick(&mut self) {
        match self.message_receiver.try_recv() {
            Ok(message) => {
                self.state.chats.push_back(message.text());
            }
            Err(_) => {}
        }
    }

    fn state(&self) -> &dyn State {
        &self.state
    }
}
