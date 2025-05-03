use tokio::sync::mpsc::{Sender, UnboundedReceiver};

use crate::games::twitch_chat::TwitchChat;

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
    pub sender: Sender<WordleGameState>,
    pub twitch_chat: UnboundedReceiver<tmi::Privmsg<'static>>,
}

impl WordleGameCommandCenter {
    pub fn new(
        sender: Sender<WordleGameState>,
        twitch_chat: UnboundedReceiver<tmi::Privmsg<'static>>,
    ) -> Self {
        Self {
            game_state: WordleGameState::WaitingForStart,
            sender,
            twitch_chat,
        }
    }

    pub async fn run(&mut self) {
        println!("Starting Wordle game...");
        self.game_state = WordleGameState::Starting;
    }
}
