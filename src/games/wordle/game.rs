use core::panic;
use std::collections::{HashMap, HashSet};
use std::convert::TryInto;

use crate::games::twitch_chat::messages::{ChatMessages, Messages};
use crate::games::wordle::view::row::Row;
use crate::twitch_chat::messages::Message;
use crate::wordle::view::row::RowState;
use anathema::backend::tui::TuiBackend;
use anathema::backend::Backend;
use anathema::component::{Children, Component, ComponentId, Context, Emitter, MouseEvent};
use anathema::runtime::Runtime;
use anathema::state::{State, Value};
use anathema::templates::Document;
use random_word::Lang;
use tokio::sync::mpsc::UnboundedReceiver;

use super::view::row::{Cell, LetterStatus, RowMessage};

pub struct Index;

#[derive(Debug, Clone)]
pub enum IndexMessage {
    Row(RowUpdate),
    Timer(u8),
    Round(u8),
}

#[derive(Debug, Clone)]
pub struct RowUpdate {
    pub row: u8,
    pub row_message: RowMessage,
}

#[derive(Debug, State, Default)]
pub struct IndexState {
    pub round: Value<u8>,
    pub timer: Value<u8>,
}

impl Component for Index {
    type Message = IndexMessage;
    type State = IndexState;

    fn on_message(
        &mut self,
        message: Self::Message,
        state: &mut Self::State,
        mut children: Children<'_, '_>,
        mut context: Context<'_, '_, Self::State>,
    ) {
        match message {
            IndexMessage::Row(row_update) => {
                context
                    .components
                    .by_attribute("id", row_update.row)
                    .send(row_update.row_message);
            }
            IndexMessage::Timer(timer) => {
                state.timer.set(timer);
            }
            IndexMessage::Round(round) => {
                state.round.set(round);
            }
        }
    }
}

pub fn start_game(chat: UnboundedReceiver<Message>) {
    let doc = Document::new("@index");
    let mut backend = TuiBackend::builder()
        .enable_raw_mode()
        //.enable_alt_screen()
        .enable_mouse()
        .hide_cursor()
        .clear()
        .finish()
        .unwrap();
    backend.finalize();

    let mut builder = Runtime::builder(doc, &backend);

    let twitch_chat_id = builder
        .default::<Messages>("messages", "src/games/twitch_chat/templates/message.aml")
        .expect("failed to register messages component");

    builder
        .prototype(
            "stack",
            "src/games/wordle/templates/row.aml",
            || Row::default(),
            || RowState::default(),
        )
        .expect("failed to register row component");

    let main_id = builder
        .component(
            "index",
            "src/games/wordle/templates/main.aml",
            Index,
            IndexState::default(),
        )
        .expect("failed to register index component");
    let emitter = builder.emitter();
    tokio::spawn(async move {
        game_loop(chat, main_id, twitch_chat_id, emitter, get_spell_checker()).await
    });
    builder
        .finish(&mut backend, |runtime, backend| runtime.run(backend))
        .unwrap();
}

fn get_spell_checker() -> HashSet<String> {
    // Read the spell checker file from the src/games/wordle/assets/wordlist.txt and read each line
    // into the hashmap
    let mut spell_checker = HashSet::new();
    let file_path = "src/games/wordle/assets/combined_unique.txt";
    if let Ok(lines) = std::fs::read_to_string(file_path) {
        for line in lines.lines() {
            let word = line.trim().to_uppercase();
            if !word.is_empty() {
                spell_checker.insert(word);
            }
        }
    } else {
        panic!("Failed to read the spell checker file");
    }
    spell_checker
}

fn get_random_word(spell_checker: &HashSet<String>) -> Word {
    let mut word = "ABCDE".to_string();

    while !spell_checker.contains(&word) {
        // Generate a random word of length 5
        // This is a placeholder, you should replace it with your own logic to get a random
        word = random_word::get_len(5, Lang::En)
            .expect("Failed to get a random word from the spell checker")
            .to_uppercase()
    }
    // Get a random word from the spell checker

    word.as_bytes()
        .try_into()
        .expect("Failed to convert random word to [u8; 5]")
}

async fn game_loop(
    mut chat: UnboundedReceiver<Message>,
    main_id: ComponentId<IndexMessage>,
    twitch_chat_id: ComponentId<ChatMessages>,
    emitter: Emitter,
    spell_checker: HashSet<String>,
) {
    loop {
        // TODO: update round count (right now it starts at 0? do we default it to 1?)
        let mut round_count = 0;
        increment_round_count(&mut round_count, &emitter, main_id).await;
        let mut start_time = std::time::Instant::now();
        let guess_duration = std::time::Duration::from_secs(20); // 20 seconds for each guess

        let word = get_random_word(&spell_checker);

        let mut game_round = GameRound::new(word, &spell_checker);
        let mut game_over = false;

        // Play a round
        while round_count <= 6 || !game_over {
            // Check if the time limit for the current guess has been reached
            if start_time.elapsed() >= guess_duration {
                // increment the guess count and reset the timer
                let winning_word = game_round.select_round_winner();

                // println!("Round {round_count}: Winning word is {winning_word:?}");
                // Update the UI with the winning word
                let row_message = game_round.build_cells(winning_word.unwrap_or([97; 5]));
                emitter
                    .emit_async(
                        main_id,
                        IndexMessage::Row(RowUpdate {
                            row: round_count,
                            row_message,
                        }),
                    )
                    .await
                    .expect("failed to send winning word message");

                // Check win condition
                // TODO: Check if the winning word is correct
                let mut did_win = true;
                for cell in row_message.data.iter() {
                    if cell.status != LetterStatus::Correct {
                        did_win = false;
                        break;
                    }
                }

                if did_win {
                    game_over = true;
                }

                // They won
                if game_over {
                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                    break;
                }
                // Reset the game round for the next guess
                start_time = std::time::Instant::now();

                next_round(&mut round_count, &emitter, main_id, &mut game_round).await;
                continue;
            }

            // Emit the timer update
            let timer = (guess_duration.as_secs() - start_time.elapsed().as_secs()) as u8;
            update_timer(timer, &emitter, main_id).await;

            // Process incoming messages from the chat
            // TODO: Currently if processing falls behind the game will not progress to next round
            while let Ok(message) = chat.try_recv() {
                // Process the message
                // For example, you can parse the message and update the game state
                //println!("Received message: {:?}", message);
                // Here you would parse the content and update the game round
                // For example, if the content is a guess, you can add it to the game round
                let player = message.sender();
                let guess = message.content();
                game_round.new_guess(player, guess);

                // emit the chat message to the twitch chat component

                emitter
                    .emit_async(
                        twitch_chat_id,
                        ChatMessages::new(
                            message.sender().to_string(),
                            message.content().to_string(),
                        ),
                    )
                    .await
                    .expect("failed to send chat messages message");
            }

            // If no messages are received, we can continue the game loop
            if start_time.elapsed() <= guess_duration {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
        // Reset the board for the next round
        reset_board(&emitter, main_id).await;
    } // Game loop
}

async fn next_round(
    round_count: &mut u8,
    emitter: &Emitter,
    main_id: ComponentId<IndexMessage>,
    game_round: &mut GameRound<'_>,
) {
    // Increment the round count
    increment_round_count(round_count, emitter, main_id).await;
    game_round.reset_votes();
}

async fn reset_board(emitter: &Emitter, main_id: ComponentId<IndexMessage>) {
    // Reset the board by emitting a reset message
    for i in 0..6 {
        // Emit
        emitter
            .emit_async(
                main_id,
                IndexMessage::Row(RowUpdate {
                    row: i,
                    row_message: RowMessage {
                        data: [Cell::default(); 5],
                    },
                }),
            )
            .await
            .expect("failed to send reset board message");
    }
}

async fn increment_round_count(
    round_count: &mut u8,
    emitter: &Emitter,
    main_id: ComponentId<IndexMessage>,
) {
    // Increment the round count
    *round_count += 1;
    // Emit the new round count
    emitter
        .emit_async(main_id, IndexMessage::Round(*round_count))
        .await
        .expect("failed to send round count message");
}

async fn update_timer(timer: u8, emitter: &Emitter, main_id: ComponentId<IndexMessage>) {
    // Emit the new round count
    emitter
        .emit_async(main_id, IndexMessage::Timer(timer))
        .await
        .expect("failed to send round count message");
}

struct GameRound<'a> {
    word: [u8; 5],
    players_votes: std::collections::HashMap<String, [u8; 5]>,
    spell_checker: &'a HashSet<String>,
}

type Word = [u8; 5];

impl<'a> GameRound<'a> {
    // Creates a new game round with a predefined word
    // word MUST be 5 characters long
    pub fn new(word: Word, spell_checker: &'a HashSet<String>) -> Self {
        Self {
            word,
            players_votes: std::collections::HashMap::new(),
            spell_checker,
        }
    }

    pub fn new_guess(&mut self, player: String, guess: String) {
        let upper = guess.to_uppercase();
        if !self.validate_guess(&upper) {
            return;
        }
        let mut arrays_of_u8 = Word::default();
        arrays_of_u8.copy_from_slice(upper.as_bytes());
        self.players_votes.insert(player, arrays_of_u8);
    }

    // Validate guess
    pub fn validate_guess(&self, guess: &str) -> bool {
        if guess.len() != 5 {
            return false;
        }

        if guess.contains(' ') {
            return false;
        }

        // Check if the guess is in the spell checker
        if !self.spell_checker.contains(&guess.to_uppercase()) {
            return false;
        }

        // Check if the guess is a valid word
        // For now, we just check if it contains only ASCII characters
        guess.is_ascii()
    }

    pub fn build_cells(&self, guess: Word) -> RowMessage {
        let mut cells = [Cell::default(); 5];
        let mut used = [false; 5]; // Track used letters in the word
        let mut letter_counts = HashMap::new();

        // First pass: mark Correct and count leftover letters
        for i in 0..5 {
            if guess[i] == self.word[i] {
                cells[i] = (guess[i], LetterStatus::Correct).into();
                used[i] = true;
            } else {
                letter_counts
                    .entry(self.word[i])
                    .and_modify(|count| *count += 1)
                    .or_insert(1);
            }
        }

        // Second pass: mark Present or Absent
        for i in 0..5 {
            if cells[i].status == LetterStatus::Correct {
                continue;
            }

            if let Some(count) = letter_counts.get_mut(&guess[i]) {
                if *count > 0 {
                    cells[i] = (guess[i], LetterStatus::Present).into();
                    *count -= 1;
                } else {
                    cells[i] = (guess[i], LetterStatus::Absent).into();
                }
            } else {
                cells[i] = (guess[i], LetterStatus::Absent).into();
            }
        }

        RowMessage { data: cells }
    }

    // Select the winning guess based on the votes
    pub fn select_round_winner(&self) -> Option<[u8; 5]> {
        let mut votes = HashMap::new();

        // Build a map of all votes
        for word in self.players_votes.values() {
            let mut count = 0;
            {
                count = *votes.entry(word.clone()).or_insert(0);
                count += 1;
            }
            votes.insert(word.clone(), count);
        }

        // Find the word with the most votes
        // If there is a tie, pick a random one
        let mut winner = None;
        let mut max_votes = 0;

        for (word, count) in &votes {
            if *count > max_votes {
                max_votes = *count;
                winner = Some(word.clone());
            } else if *count == max_votes {
                // If there's a tie, randomly select one of the tied words
                if rand::random::<bool>() {
                    winner = Some(word.clone());
                }
            }
        }

        winner
    }

    pub fn reset_votes(&mut self) {
        self.players_votes.clear();
    }
}
