use crate::games::wordle::{gamestarting::CountDown, view::WordleGameView};
use iocraft::prelude::*;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

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
    pub guesses: Vec<String>,
    pub current_attempt: i8,
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

pub async fn run_game(twitch_chat: UnboundedReceiver<tmi::Privmsg<'static>>) {
    println!("Starting Wordle game...");
    let mut system: Element<CountDown> = element! {CountDown};
    let r = tokio::join!(system.fullscreen());
    println!("hold up. we don't need that one shot now do we.");
    if let Err(e) = r.0 {
        eprintln!("Error: {}", e);
    };
    let mut wordle_game = WordleGame {
        word: "words".to_owned(),
        guesses: Vec::new(),
        current_attempt: 0,
        game_over: false,
        game_won: false,
    };
    let mut game_state = WordleGameState::WaitingForGuess(wordle_game);

    let (game_state_sender, gamestate_rec) = tokio::sync::mpsc::unbounded_channel();
    let (frontend_sender, frontend_rec) = tokio::sync::mpsc::unbounded_channel();
    let mut system: Element<WordleGameView> = element! {WordleGameView (gamestate_message_receiver: Some(gamestate_rec), chat_message_receiver: Some(frontend_rec))};
    let h = tokio::spawn(async move {
        game_loop(game_state, twitch_chat, frontend_sender, game_state_sender).await
    });
    let r = tokio::join!(system.fullscreen());
}

async fn game_loop(
    game_state: WordleGameState,
    twitch_chat: UnboundedReceiver<tmi::Privmsg<'static>>,
    twitch_chat_frontend_sender: UnboundedSender<String>,
    game_state_sender: UnboundedSender<WordleGameState>,
) {
}
