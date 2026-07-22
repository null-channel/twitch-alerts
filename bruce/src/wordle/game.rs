use core::panic;
use std::collections::{HashMap, HashSet};
use std::convert::TryInto;
use std::ops::DerefMut;
use std::sync::LazyLock;

use crate::wordle::view::row::Row;
use crate::wordle::view::row::RowState;
use anathema::backend::tui::TuiBackend;
use anathema::backend::Backend;
use anathema::component::{Children, Component, ComponentId, Context, Emitter};
use anathema::derive;
use anathema::runtime::Runtime;
use anathema::state::{State, Value};
use anathema::templates::Document;
use random_word::WordList;
use sqlx::{pool::PoolConnection, Pool, Sqlite};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::Mutex;
use twitch_common::chat::messages::{ChatMessages, MessagePlatform, Messages};
use twitch_common::twitch_chat::messages::Message;

use super::view::row::{Cell, LetterStatus, RowMessage};

// Game round time limit
const GAME_ROUND_TIME_LIMIT: u8 = 30; // seconds
                                      // TODO: Make this a thing
const START_AI_ROUND_TIME_LIMIT: u64 = 120; // seconds

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

// twitch_[username],games_played,guesses,correct_guesses,total_points

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
                if row_update.row == 7 {
                    //println!("Solution row: {:?}", row_update.row_message.data);
                }
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

pub fn start_game(
    chat: UnboundedReceiver<Message>,
    word_list: WordList,
    leader_board: Pool<Sqlite>,
) {
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
        .default::<Messages>("messages", "../twitch_common/src/chat/twitch_chat/templates/message.aml")
        .expect("failed to register messages component");

    builder
        .prototype(
            "stack",
            "src/wordle/templates/row.aml",
            || Row::default(),
            || RowState::default(),
        )
        .expect("failed to register row component");

    let main_id = builder
        .component(
            "index",
            "src/wordle/templates/main.aml",
            Index,
            IndexState::default(),
        )
        .expect("failed to register index component");
    let emitter = builder.emitter();
    tokio::spawn(async move {
        game_loop(
            chat,
            main_id,
            twitch_chat_id,
            emitter,
            get_spell_checker(word_list),
            word_list,
            leader_board,
        )
        .await
    });
    builder
        .finish(&mut backend, |runtime, backend| runtime.run(backend))
        .unwrap();
}

fn get_spell_checker(word_list: WordList) -> HashSet<String> {
    // Read the spell checker file from the src/games/wordle/assets/wordlist.txt and read each line
    // into the hashmap
    let mut spell_checker = HashSet::new();
    let file_path = match word_list {
        WordList::Challenge => "../wordle_word/src/txt/challenge.txt",
        WordList::Nerd => "../wordle_word/src/txt/nerd.txt",
        WordList::Standard => "../wordle_word/src/txt/standard.txt",
    };

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

fn get_random_word(list: WordList) -> Word {
    random_word::get_len(5, list)
        .expect("Failed to get a random word from the spell checker")
        .to_uppercase()
        .as_bytes()
        .try_into()
        .expect("Failed to convert random word to [u8; 5]")
}

// Easy is to dirive copy/clone
#[derive(Clone, Copy)]
pub struct UserScore {
    id: i64,
    total_points: i64,
}

impl UserScore {
    pub fn new(id: i64, total_points: i64) -> Self {
        Self { id, total_points }
    }
}

#[derive(Clone)]
pub struct User {
    pub platform: MessagePlatform,
    pub platform_user_id: String,
    pub platform_user_name: String,
    pub score: UserScore,
}

// Twitch Users Hashmap<(platform)Twitch_userID, (Interal ID, Current Score)>
static TWITCH_USERS: LazyLock<Mutex<HashMap<String, UserScore>>> = LazyLock::new(|| {
    let m = Mutex::new(HashMap::new());
    m
});
static YOUTUBE_USERS: LazyLock<Mutex<HashMap<i32, &'static str>>> = LazyLock::new(|| {
    let m = Mutex::new(HashMap::new());
    m
});
static DISCORD_USERS: LazyLock<Mutex<HashMap<i32, &'static str>>> = LazyLock::new(|| {
    let m = Mutex::new(HashMap::new());
    m
});
// Should get our internal ID from the database matching the players platform and platform_user_id.
// Should cache the player's ID in memory and should check that before hitting the database
async fn get_player_id(
    platform: MessagePlatform,
    platform_user_id: String,
    platform_user_name: String,
    conn: &mut PoolConnection<Sqlite>,
) -> anyhow::Result<UserScore> {
    match platform {
        MessagePlatform::Twitch => {
            let twitch_users = TWITCH_USERS.lock().await;
            // Does user exist in cashe
            if let Some(user) = twitch_users.get(&platform_user_id) {
                Ok(user.clone())
            } else {
                // Does user exist in database?
                let platform_u8 = platform as u8;
                let rez = sqlx::query!(
                    r#"
                SELECT id, total_points FROM wordle_highscores WHERE platform = ? AND platform_user_id = ?
                        "#,
                    platform_u8,
                    platform_user_id,
                )
                .fetch_one( conn.deref_mut())
                .await;
                // if no user, insert a new user.
                let Ok(rez) = rez else {
                    // Insert into
                    let req = sqlx::query!(
                        r#"
                INSERT INTO wordle_highscores ( username, platform_user_id, platform, total_points )
                VALUES ( ?, ?, ?, ? )
                        "#,
                        platform_user_name,
                        platform_user_id,
                        platform_u8,
                        1,
                    )
                    .execute(conn.deref_mut())
                    .await?;

                    // request last row to get real ID
                    let last_row_id = req.last_insert_rowid();
                    let rez = sqlx::query!(
                        r#"
                    SELECT id, total_points FROM wordle_highscores WHERE rowid = ?
                            "#,
                        last_row_id,
                    )
                    .fetch_one(conn.deref_mut())
                    .await?;

                    return Ok(UserScore {
                        id: rez.id,
                        total_points: rez.total_points,
                    });
                };
                return Ok(UserScore::new(rez.id, rez.total_points));
                //twitch_users.insert(platform_user_id.to_string(), UserScore::new(rez.id, rez.total_points));
            }
        }
        _ => {
            //TODO: deal with youtube users
            return Ok(UserScore {
                id: 0,
                total_points: 0,
            });
        }
    }
}

async fn game_loop(
    mut chat: UnboundedReceiver<Message>,
    main_id: ComponentId<IndexMessage>,
    twitch_chat_id: ComponentId<ChatMessages>,
    emitter: Emitter,
    spell_checker: HashSet<String>,
    word_list: WordList,
    leader_board: Pool<Sqlite>,
) {
    let guess_duration = std::time::Duration::from_secs(GAME_ROUND_TIME_LIMIT as u64);
    loop {
        let mut correct_cells = [Cell::default(); 5];
        let mut round_count = 0;
        increment_round_count(&mut round_count, &emitter, main_id).await;
        // Not actually "starting" the game now.
        let mut start_time = std::time::Instant::now();
        update_timer(GAME_ROUND_TIME_LIMIT, &emitter, main_id).await;

        let word = get_random_word(word_list);

        let mut game_round = GameRound::new(word, &spell_checker);
        let mut game_over = false;
        let mut round_started = false;
        let mut leader_board_conn = leader_board.acquire().await.unwrap();

        // Play a round
        while round_count <= 6 && !game_over {
            // Process incoming messages from the chat
            // TODO: Currently if processing falls behind the game will not progress to next round
            while let Ok(message) = chat.try_recv() {
                let player = message.sender();
                let guess = message.content();
                let player_id = get_player_id(
                    message.platform(),
                    message.platform_id(),
                    player.clone(),
                    &mut leader_board_conn,
                );
                let valid = game_round.new_guess(player, guess);
                // emit the chat message to the twitch chat component
                if valid && !round_started {
                    round_started = true;
                    start_time = std::time::Instant::now();
                }

                emitter
                    .emit_async(
                        twitch_chat_id,
                        ChatMessages::new(
                            message.sender().to_string(),
                            message.content().to_string(),
                            message.platform(),
                        ),
                    )
                    .await
                    .expect("failed to send chat messages message");
            }

            if !round_started {
                // If the round has not started, we can wait for a message to start the round
                tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
                continue;
            }

            // Check if the time limit for the current guess has been reached
            if start_time.elapsed() >= guess_duration {
                // increment the guess count and reset the timer
                let winning_word = game_round.select_round_winner(word);

                // println!("Round {round_count}: Winning word is {winning_word:?}");
                // Update the UI with the winning word
                let row_message = game_round.build_cells(winning_word.unwrap_or([97; 5]));
                emitter
                    .emit(
                        main_id,
                        IndexMessage::Row(RowUpdate {
                            row: round_count,
                            row_message,
                        }),
                    )
                    .expect("failed to send winning word message");

                update_correct_cells(&mut correct_cells, &row_message.data);

                emitter
                    .emit_async(
                        main_id,
                        IndexMessage::Row(RowUpdate {
                            row: 7, // Solution row
                            row_message: RowMessage {
                                data: correct_cells,
                            },
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

                // Calculate score here
                // for (user_id, guess) in game_round.players_votes {
                //score = calculate_score(user_id, winning_word, guess);
                // Score updating could be batched... but that is a 100000 follower issue.
                //update_users_score(user_id, score);
                //}

                if did_win {
                    game_over = true;
                }

                // They won
                if game_over {
                    break;
                }
                // Reset the game round for the next guess
                start_time = std::time::Instant::now();
                update_timer(GAME_ROUND_TIME_LIMIT, &emitter, main_id).await;

                next_round(
                    &mut round_count,
                    &emitter,
                    main_id,
                    &mut game_round,
                    &mut round_started,
                )
                .await;
                continue;
            }

            // Emit the timer update
            let timer = (guess_duration.as_secs() - start_time.elapsed().as_secs()) as u8;
            update_timer(timer, &emitter, main_id).await;

            // If no messages are received, we can continue the game loop
            if start_time.elapsed() <= guess_duration {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }

        let correct_word = game_round.build_cells(word);

        // Toggle bit has a bug in this UI threading.
        // messages are not recived in the order they are sent
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        //update the UI with the final state of the game
        for i in 0..5 {
            if correct_cells[i].status != LetterStatus::Correct {
                correct_cells[i] = Cell {
                    letter: word[i],
                    status: LetterStatus::Absent,
                };
            }
        }

        emitter
            .emit_async(
                main_id,
                IndexMessage::Row(RowUpdate {
                    row: 7, // Solution row
                    row_message: RowMessage {
                        data: correct_cells,
                    },
                }),
            )
            .await
            .expect("failed to send winning word message");
        // build correct cells from the winning word
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        // Reset the board for the next round
        update_timer(GAME_ROUND_TIME_LIMIT, &emitter, main_id).await;
        reset_board(&emitter, main_id).await;
    } // Game loop
}

/* For each round each user that guessed the selected word gets 2 points for every correct letter and 1 point for every correct letter in the wrong position.
the users that guessed the word first gets an extra 5 points
if the word is the corret word, the first user to guess gets an extra 10 point and everyone that guessed it gets 10 points
not batching writes to database at first.
*/
fn calculate_score(user_id: String, correct_cells: &[Cell; 5], their_guess: &[Cell; 5]) -> u8 {
    // Check if the user guessed the word correctly
    5
}

fn update_correct_cells(correct_cells: &mut [Cell; 5], row_data: &[Cell; 5]) {
    // Update the correct cells with the new row data
    for (i, cell) in row_data.iter().enumerate() {
        if cell.status == LetterStatus::Correct {
            correct_cells[i] = *cell;
        }
    }
}

async fn next_round(
    round_count: &mut u8,
    emitter: &Emitter,
    main_id: ComponentId<IndexMessage>,
    game_round: &mut GameRound<'_>,
    round_started: &mut bool,
) {
    // Increment the round count
    increment_round_count(round_count, emitter, main_id).await;
    *round_started = false;
    game_round.reset_votes();
}

async fn reset_board(emitter: &Emitter, main_id: ComponentId<IndexMessage>) {
    // Reset the board by emitting a reset message
    for i in 1..=7 {
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
    players_votes: HashMap<String, ([u8; 5], u32)>,
    spell_checker: &'a HashSet<String>,
    guess_position: u32,
}

type Word = [u8; 5];

struct VoteIndexer {
    votes: u32,
    voters: Vec<String>,
    position_in_round: u32,
}

impl<'a> GameRound<'a> {
    // Creates a new game round with a predefined word
    // word MUST be 5 characters long
    pub fn new(word: Word, spell_checker: &'a HashSet<String>) -> Self {
        Self {
            word,
            players_votes: HashMap::new(),
            spell_checker,
            guess_position: 0,
        }
    }

    pub fn new_guess(&mut self, player: String, guess: String) -> bool {
        let upper = guess.to_uppercase();
        if !self.validate_guess(&upper) {
            return false;
        }
        self.guess_position += 1;
        let mut arrays_of_u8 = Word::default();
        arrays_of_u8.copy_from_slice(upper.as_bytes());
        self.players_votes
            .insert(player, (arrays_of_u8, self.guess_position));
        true
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
    pub fn select_round_winner(&self, word: Word) -> Option<Word> {
        // If there are multiple words with the same number of votes, first check to see if any of
        // them are the correct word, and pick that word.
        // Second if there are no correct words, pick the lowest position word with the most votes.
        let mut total_votes = HashMap::new();
        let mut highest_vote = 0;

        // Build a map of all votes
        for (name, (voted_word, pos)) in self.players_votes.iter() {
            let votes = total_votes.entry(*voted_word).or_insert(VoteIndexer {
                votes: 0,
                voters: Vec::new(),
                position_in_round: 10000,
            });
            votes.votes += 1;
            votes.voters.push(name.clone());
            if pos < &votes.position_in_round {
                votes.position_in_round = *pos;
            }
            if votes.votes > highest_vote {
                highest_vote = votes.votes;
            }
        }

        // Find the word with the most votes
        // If there is a tie, pick the word with the lowest position in the round
        let mut winning_word = ([0; 5], 1000000);
        for (voted_word, vote_indexer) in total_votes.iter() {
            if vote_indexer.votes == highest_vote {
                // Check if the voted word is the correct word
                if *voted_word == word {
                    return Some(*voted_word);
                }
                if winning_word.1 > vote_indexer.position_in_round {
                    winning_word = (*voted_word, vote_indexer.position_in_round);
                }
            }
        }
        Some(winning_word.0)
    }

    pub fn reset_votes(&mut self) {
        self.players_votes.clear();
    }
}
