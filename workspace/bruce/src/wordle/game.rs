use std::collections::{HashMap, HashSet};
use std::convert::TryInto;
use std::iter::FromIterator;

use super::scoreboard::{add_points, get_player_score, top_scores, LeaderboardEntry};
use super::view::chat::{ChatMessages, Messages};
use crate::wordle::view::row::Row;
use crate::wordle::view::row::RowState;
use anathema::backend::tui::TuiBackend;
use anathema::backend::Backend;
use anathema::component::{Children, Component, ComponentId, Context, Emitter};
use anathema::runtime::Runtime;
use anathema::state::{List, State, Value};
use anathema::templates::{Document, ToSourceKind};
use anyhow::Context as _;
use random_word::WordList;
use sqlx::{Pool, Sqlite};
use tokio::sync::mpsc::UnboundedReceiver;
use twitch_common::chat::messages::{Message, MessagePlatform};

use super::view::row::{Cell, LetterStatus, RowMessage};

const GAME_ROUND_TIME_LIMIT: u8 = 30; // seconds

// Points awarded for the word that lands on the board each row.
const CORRECT_LETTER_POINTS: u32 = 2;
const PRESENT_LETTER_POINTS: u32 = 1;
// Solving the word earns `SOLVE_BONUS_BASE - SOLVE_BONUS_PENALTY_PER_ROW * row`.
const SOLVE_BONUS_BASE: u32 = 40;
const SOLVE_BONUS_PENALTY_PER_ROW: u32 = 5;

// How many players the on-screen leaderboard shows.
const LEADER_BOARD_SIZE: u32 = 10;

pub struct Index;

#[derive(Debug, Clone)]
pub enum IndexMessage {
    Row(RowUpdate),
    Timer(u8),
    Round(u8),
    LeaderBoard(Vec<LeaderboardEntry>),
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
    pub leaders: Value<List<LeaderState>>,
}

#[derive(Debug, State)]
pub struct LeaderState {
    pub rank: Value<usize>,
    pub name: Value<String>,
    pub points: Value<i64>,
}

impl Component for Index {
    type Message = IndexMessage;
    type State = IndexState;

    fn on_message(
        &mut self,
        message: Self::Message,
        state: &mut Self::State,
        _: Children<'_, '_>,
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
            IndexMessage::LeaderBoard(entries) => {
                let leaders = entries
                    .into_iter()
                    .enumerate()
                    .map(|(index, entry)| LeaderState {
                        rank: Value::new(index + 1),
                        name: Value::new(entry.username),
                        points: Value::new(entry.total_points),
                    });
                state.leaders.set(List::from_iter(leaders));
            }
        }
    }
}

pub fn start_game(
    chat: UnboundedReceiver<Message>,
    word_list: WordList,
    leader_board: Pool<Sqlite>,
) -> anyhow::Result<()> {
    let doc = Document::new("@index");
    let mut backend = TuiBackend::builder()
        .enable_raw_mode()
        .enable_mouse()
        .hide_cursor()
        .clear()
        .finish()
        .context("initialize Wordle terminal")?;
    backend.finalize();

    let mut builder = Runtime::builder(doc, &backend);

    let twitch_chat_id = builder
        .default::<Messages>(
            "messages",
            include_str!("templates/message.aml").to_template(),
        )
        .context("register messages component")?;

    builder
        .prototype(
            "stack",
            include_str!("templates/row.aml").to_template(),
            || Row,
            RowState::default,
        )
        .context("register row component")?;

    let main_id = builder
        .component(
            "index",
            include_str!("templates/main.aml").to_template(),
            Index,
            IndexState::default(),
        )
        .context("register index component")?;
    let emitter = builder.emitter();
    let game_task = tokio::spawn(async move {
        if let Err(error) = game_loop(
            chat,
            main_id,
            twitch_chat_id,
            emitter,
            get_spell_checker(word_list),
            word_list,
            leader_board,
        )
        .await
        {
            tracing::error!(%error, "Wordle game loop stopped");
        }
    });
    let result = builder.finish(&mut backend, |runtime, backend| runtime.run(backend));
    game_task.abort();
    result.context("run Wordle terminal")
}

fn get_spell_checker(word_list: WordList) -> HashSet<String> {
    random_word::all(word_list)
        .iter()
        .map(|word| word.trim().to_uppercase())
        .filter(|word| !word.is_empty())
        .collect()
}

fn get_random_word(list: WordList) -> Word {
    random_word::get_len(5, list)
        .expect("Failed to get a random word from the spell checker")
        .to_uppercase()
        .as_bytes()
        .try_into()
        .expect("Failed to convert random word to [u8; 5]")
}

async fn game_loop(
    mut chat: UnboundedReceiver<Message>,
    main_id: ComponentId<IndexMessage>,
    twitch_chat_id: ComponentId<ChatMessages>,
    emitter: Emitter,
    spell_checker: HashSet<String>,
    word_list: WordList,
    leader_board: Pool<Sqlite>,
) -> anyhow::Result<()> {
    let guess_duration = std::time::Duration::from_secs(GAME_ROUND_TIME_LIMIT as u64);
    refresh_leader_board(&leader_board, &emitter, main_id).await?;
    loop {
        let mut correct_cells = [Cell::default(); 5];
        let mut round_count = 0;
        increment_round_count(&mut round_count, &emitter, main_id).await?;
        // Not actually "starting" the game now.
        let mut start_time = std::time::Instant::now();
        update_timer(GAME_ROUND_TIME_LIMIT, &emitter, main_id).await?;

        let word = get_random_word(word_list);

        let mut game_round = GameRound::new(word, &spell_checker);
        let mut game_over = false;
        let mut round_started = false;

        // Play a round
        while round_count <= 6 && !game_over {
            // Process incoming messages from the chat
            // TODO: Currently if processing falls behind the game will not progress to next round
            while let Ok(message) = chat.try_recv() {
                let player = Player::from(&message);
                let guess = message.content();
                let valid = game_round.new_guess(player.clone(), guess);
                if valid && player.is_scored() {
                    let score =
                        get_player_score(&leader_board, player.platform, &player.id, &player.name)
                            .await?;
                    tracing::debug!(
                        player_id = score.id,
                        total_points = score.total_points,
                        "Wordle player registered"
                    );
                }
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
                    .context("send chat message")?;
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
                    .context("send winning word")?;

                update_correct_cells(&mut correct_cells, &row_message.data);

                // Everyone who voted for the word on the board shares its points.
                if let Some(winning_word) = winning_word {
                    let points = score_row(&row_message.data, round_count);
                    let awarded =
                        award_points(&leader_board, game_round.voters_for(winning_word), points)
                            .await?;
                    if awarded {
                        refresh_leader_board(&leader_board, &emitter, main_id).await?;
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
                    .context("send winning word")?;
                game_over = row_message
                    .data
                    .iter()
                    .all(|cell| cell.status == LetterStatus::Correct);

                // They won
                if game_over {
                    break;
                }
                // Reset the game round for the next guess
                start_time = std::time::Instant::now();
                update_timer(GAME_ROUND_TIME_LIMIT, &emitter, main_id).await?;

                next_round(
                    &mut round_count,
                    &emitter,
                    main_id,
                    &mut game_round,
                    &mut round_started,
                )
                .await?;
                continue;
            }

            // Emit the timer update
            let timer = (guess_duration.as_secs() - start_time.elapsed().as_secs()) as u8;
            update_timer(timer, &emitter, main_id).await?;

            // If no messages are received, we can continue the game loop
            if start_time.elapsed() <= guess_duration {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }

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
            .context("send winning word")?;
        // build correct cells from the winning word
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        // Reset the board for the next round
        update_timer(GAME_ROUND_TIME_LIMIT, &emitter, main_id).await?;
        reset_board(&emitter, main_id).await?;
    } // Game loop
}

/// Points earned by a row: 2 per correctly placed letter, 1 per letter in the wrong spot,
/// plus a solve bonus of 40 minus 5 for every row used (row 1 = 35, row 6 = 10).
fn score_row(cells: &[Cell; 5], row: u8) -> u32 {
    let letter_points: u32 = cells
        .iter()
        .map(|cell| match cell.status {
            LetterStatus::Correct => CORRECT_LETTER_POINTS,
            LetterStatus::Present => PRESENT_LETTER_POINTS,
            LetterStatus::Absent => 0,
        })
        .sum();
    let solved = cells
        .iter()
        .all(|cell| cell.status == LetterStatus::Correct);
    let solve_bonus = if solved {
        SOLVE_BONUS_BASE.saturating_sub(SOLVE_BONUS_PENALTY_PER_ROW * u32::from(row))
    } else {
        0
    };
    letter_points + solve_bonus
}

/// Returns whether any stored score changed.
async fn award_points(
    leader_board: &Pool<Sqlite>,
    players: Vec<&Player>,
    points: u32,
) -> anyhow::Result<bool> {
    if points == 0 {
        return Ok(false);
    }
    let mut awarded = false;
    for player in players.into_iter().filter(|player| player.is_scored()) {
        awarded = true;
        let score = add_points(
            leader_board,
            player.platform,
            &player.id,
            &player.name,
            i64::from(points),
        )
        .await?;
        tracing::info!(
            player = %player.name,
            points,
            total_points = score.total_points,
            "Wordle points awarded"
        );
    }
    Ok(awarded)
}

async fn refresh_leader_board(
    leader_board: &Pool<Sqlite>,
    emitter: &Emitter,
    main_id: ComponentId<IndexMessage>,
) -> anyhow::Result<()> {
    let entries = top_scores(leader_board, LEADER_BOARD_SIZE).await?;
    emitter
        .emit_async(main_id, IndexMessage::LeaderBoard(entries))
        .await
        .context("send leader board update")
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
) -> anyhow::Result<()> {
    // Increment the round count
    increment_round_count(round_count, emitter, main_id).await?;
    *round_started = false;
    game_round.reset_votes();
    Ok(())
}

async fn reset_board(emitter: &Emitter, main_id: ComponentId<IndexMessage>) -> anyhow::Result<()> {
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
            .context("send board reset")?;
    }
    Ok(())
}

async fn increment_round_count(
    round_count: &mut u8,
    emitter: &Emitter,
    main_id: ComponentId<IndexMessage>,
) -> anyhow::Result<()> {
    // Increment the round count
    *round_count += 1;
    // Emit the new round count
    emitter
        .emit_async(main_id, IndexMessage::Round(*round_count))
        .await
        .context("send round update")
}

async fn update_timer(
    timer: u8,
    emitter: &Emitter,
    main_id: ComponentId<IndexMessage>,
) -> anyhow::Result<()> {
    // Emit the new round count
    emitter
        .emit_async(main_id, IndexMessage::Timer(timer))
        .await
        .context("send round update")
}

/// A chat participant, identified by platform and platform user id rather than display name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Player {
    pub platform: MessagePlatform,
    pub id: String,
    pub name: String,
}

impl Player {
    /// Debug messages are for local testing and never reach the persistent scoreboard.
    pub fn is_scored(&self) -> bool {
        !matches!(self.platform, MessagePlatform::Debug)
    }
}

impl From<&Message> for Player {
    fn from(message: &Message) -> Self {
        Self {
            platform: message.platform(),
            id: message.platform_id(),
            name: message.sender(),
        }
    }
}

struct GameRound<'a> {
    word: [u8; 5],
    players_votes: HashMap<Player, (Word, u32)>,
    spell_checker: &'a HashSet<String>,
    guess_position: u32,
}

type Word = [u8; 5];

struct VoteIndexer {
    votes: u32,
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

    pub fn new_guess(&mut self, player: Player, guess: String) -> bool {
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
        let mut letter_counts = HashMap::new();

        // First pass: mark Correct and count leftover letters
        for i in 0..5 {
            if guess[i] == self.word[i] {
                cells[i] = (guess[i], LetterStatus::Correct).into();
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
        for (voted_word, pos) in self.players_votes.values() {
            let votes = total_votes.entry(*voted_word).or_insert(VoteIndexer {
                votes: 0,
                position_in_round: 10000,
            });
            votes.votes += 1;
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

    /// Players whose current vote is `word`.
    pub fn voters_for(&self, word: Word) -> Vec<&Player> {
        self.players_votes
            .iter()
            .filter(|(_, (voted_word, _))| *voted_word == word)
            .map(|(player, _)| player)
            .collect()
    }

    pub fn reset_votes(&mut self) {
        self.players_votes.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_ui_templates_compile() -> anyhow::Result<()> {
        let mut document = Document::new("@index");
        document.add_component(
            "messages",
            include_str!("templates/message.aml").to_template(),
        )?;
        document.add_component("stack", include_str!("templates/row.aml").to_template())?;
        document.add_component("index", include_str!("templates/main.aml").to_template())?;
        document.compile()?;
        Ok(())
    }

    fn cells(statuses: [LetterStatus; 5]) -> [Cell; 5] {
        let mut cells = [Cell::default(); 5];
        for (cell, status) in cells.iter_mut().zip(statuses) {
            cell.status = status;
        }
        cells
    }

    #[test]
    fn letters_score_two_for_correct_and_one_for_present() {
        use LetterStatus::*;
        assert_eq!(score_row(&cells([Absent; 5]), 1), 0);
        assert_eq!(
            score_row(&cells([Present, Absent, Absent, Absent, Absent]), 1),
            1
        );
        assert_eq!(
            score_row(&cells([Correct, Absent, Absent, Absent, Absent]), 1),
            2
        );
        assert_eq!(
            score_row(&cells([Correct, Present, Correct, Present, Absent]), 3),
            6
        );
    }

    #[test]
    fn solving_adds_forty_minus_five_per_row() {
        let solved = cells([LetterStatus::Correct; 5]);
        assert_eq!(score_row(&solved, 1), 10 + 35);
        assert_eq!(score_row(&solved, 2), 10 + 30);
        assert_eq!(score_row(&solved, 6), 10 + 10);
    }

    fn player(id: &str) -> Player {
        Player {
            platform: MessagePlatform::Twitch,
            id: id.into(),
            name: format!("name-{id}"),
        }
    }

    #[test]
    fn voters_for_returns_players_behind_the_board_word() {
        let dictionary: HashSet<String> = ["APPLE", "GRAPE", "MANGO"]
            .iter()
            .map(|word| word.to_string())
            .collect();
        let mut round = GameRound::new(*b"APPLE", &dictionary);
        assert!(round.new_guess(player("1"), "grape".into()));
        assert!(round.new_guess(player("2"), "apple".into()));
        assert!(round.new_guess(player("3"), "mango".into()));
        // A later guess from the same player replaces their earlier vote.
        assert!(round.new_guess(player("3"), "apple".into()));
        assert!(!round.new_guess(player("4"), "zzzzz".into()));

        let winner = round.select_round_winner(*b"APPLE").unwrap();
        assert_eq!(&winner, b"APPLE");
        let mut voters: Vec<&str> = round
            .voters_for(winner)
            .into_iter()
            .map(|player| player.id.as_str())
            .collect();
        voters.sort();
        assert_eq!(voters, ["2", "3"]);
        assert_eq!(score_row(&round.build_cells(winner).data, 1), 45);
    }

    #[test]
    fn leader_board_renders_ranked_players() -> anyhow::Result<()> {
        use anathema::backend::testing::TestBackend;

        let doc = Document::new("@index");
        let mut backend = TestBackend::new((160, 40));
        backend.events().next().stop();

        let mut builder = Runtime::builder(doc, &backend);
        builder.default::<Messages>(
            "messages",
            include_str!("templates/message.aml").to_template(),
        )?;
        builder.prototype(
            "stack",
            include_str!("templates/row.aml").to_template(),
            || Row,
            RowState::default,
        )?;
        let main_id = builder.component(
            "index",
            include_str!("templates/main.aml").to_template(),
            Index,
            IndexState::default(),
        )?;
        let emitter = builder.emitter();
        emitter.emit(
            main_id,
            IndexMessage::LeaderBoard(vec![
                LeaderboardEntry {
                    username: "alice".into(),
                    total_points: 120,
                },
                LeaderboardEntry {
                    username: "bob".into(),
                    total_points: 87,
                },
            ]),
        )?;
        builder.finish(&mut backend, |runtime, backend| runtime.run(backend))?;

        let screen: Vec<String> = (0..40).map(|line| backend.line(line)).collect();
        let rendered = screen.join("\n");
        println!("{rendered}");
        assert!(rendered.contains("Leader Board"), "{}", rendered);
        let alice = screen.iter().position(|line| line.contains("alice"));
        let bob = screen.iter().position(|line| line.contains("bob"));
        assert!(alice.is_some() && bob.is_some(), "{}", rendered);
        assert!(alice < bob, "{}", rendered);
        assert!(screen[alice.unwrap()].contains("1.") && screen[alice.unwrap()].contains("120"));
        assert!(screen[bob.unwrap()].contains("2.") && screen[bob.unwrap()].contains("87"));
        Ok(())
    }

    #[test]
    fn each_dictionary_accepts_its_generated_words() {
        for list in [WordList::Standard, WordList::Challenge, WordList::Nerd] {
            let dictionary = get_spell_checker(list);
            assert!(!dictionary.is_empty());
            let word = get_random_word(list);
            let round = GameRound::new(word, &dictionary);
            assert!(round.validate_guess(std::str::from_utf8(&word).unwrap()));
        }
    }
}
