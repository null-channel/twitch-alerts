use crate::games::twitch_chat::TwitchChat;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use crate::games::wordle::gamestarting::{CountDown, CountdownProps};
use iocraft::prelude::*;

/// Start game -> get word from wordlist
/// Start game countdown timer
/// Start voting
/// Collect votes
/// Select word (Most voted for word, or random selection from top tied votes, or random word if no
/// one votes and life is sad)
/// display word to players
/// Start next round

pub struct WordleGame {
    pub word: String,
    pub attempts: Vec<String>,
    pub current_attempt: String,
    pub game_over: bool,
    pub game_won: bool,
}

pub enum WordleGameState {
    WaitingForStart,
    Starting,
    WaitingForGuess(WordleGame),
    SelectedGuess(WordleGame),
    GameOver(WordleGame),
}

pub struct WordleGameCommandCenter {
    pub game_state: WordleGameState,
    // Channels to frontend
    pub sender: UnboundedSender<WordleGameState>,
    pub receiver: UnboundedReceiver<WordleGameState>,
    pub twitch_chat: UnboundedReceiver<tmi::Privmsg<'static>>,
}

impl WordleGameCommandCenter {
    pub fn new(twitch_chat: UnboundedReceiver<tmi::Privmsg<'static>>) -> Self {
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        Self {
            game_state: WordleGameState::WaitingForStart,
            sender,
            receiver,
            twitch_chat,
        }
    }

    pub async fn run(&mut self) {
        println!("Starting Wordle game...");
        self.game_state = WordleGameState::Starting;

        let (sender, _) = tokio::sync::mpsc::channel(1);
        let props = CountdownProps {
            gamestate_message_receiver: None,
            one_shot: Some(sender),
        };
        let mut system: Element<CountDown> = element! {CountDown ( gamestate_message_receiver: props.gamestate_message_receiver, one_shot: props.one_shot ) };
        let r = tokio::join!(system.fullscreen());
        if let Err(e) = r.0 {
            eprintln!("Error: {}", e);
        };
    }
}
