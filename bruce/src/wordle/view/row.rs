use std::iter::FromIterator;

use anathema::component::{Children, Component, Context};
use anathema::state::{List, State, Value};
use crossterm::terminal::LeaveAlternateScreen;

#[derive(Debug, Clone, Copy)]
pub struct Cell {
    pub letter: u8,
    pub status: LetterStatus,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            letter: " ".as_bytes()[0],
            status: LetterStatus::Absent,
        }
    }
}

impl From<(u8, LetterStatus)> for Cell {
    fn from((letter, status): (u8, LetterStatus)) -> Self {
        Self { letter, status }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LetterStatus {
    Correct,
    Present,
    #[default]
    Absent,
}

#[derive(Debug, Clone, Copy)]
pub struct RowMessage {
    pub data: [Cell; 5],
}

impl From<RowMessage> for RowState {
    fn from(message: RowMessage) -> Self {
        let cells = message.data.iter().map(|cell| CellState::from(*cell));
        Self {
            cells: Value::new(List::from_iter(cells)),
        }
    }
}

impl RowMessage {
    pub fn new(data: [Cell; 5]) -> Self {
        Self { data }
    }
}

#[derive(State)]
struct CellState {
    letter: Value<char>,
    status: Value<u8>,
}

impl Default for CellState {
    fn default() -> Self {
        Self {
            letter: Value::new(' '),
            status: Value::new(LetterStatus::Absent as u8),
        }
    }
}

impl From<Cell> for CellState {
    fn from(cell: Cell) -> Self {
        Self {
            letter: Value::new(cell.letter.into()),
            status: Value::new(cell.status as u8),
        }
    }
}

#[derive(State)]
pub struct RowState {
    pub cells: Value<List<CellState>>,
}

impl Default for RowState {
    fn default() -> Self {
        Self {
            cells: Value::new(List::from_iter((0..5).map(|_| CellState::default()))),
        }
    }
}

impl RowState {
    pub fn copy_from(&mut self, other: &RowMessage) {
        let cells = other.data.iter().map(|cell| CellState::from(*cell));
        self.cells.set(List::from_iter(cells));
    }
}
#[derive(Default)]
pub struct Row;

impl Component for Row {
    type Message = RowMessage;
    type State = RowState;

    fn on_message(
        &mut self,
        message: Self::Message,
        state: &mut Self::State,
        _: Children<'_, '_>,
        _: Context<'_, '_, Self::State>,
    ) {
        // Convert the message to the state
        state.copy_from(&message);
    }
}

fn color_to_string(color: u8) -> String {
    match color {
        0 => "red".to_string(),
        1 => "yellow".to_string(),
        2 => "green".to_string(),
        _ => "gray".to_string(),
    }
}
