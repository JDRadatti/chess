use std::sync::mpsc::{self, Sender};

use game::{Color, Match, Move, RandomPlayer, Square, Status};

use crate::human::HumanPlayer;

/// Application.
pub struct App {
    /// should the application exit?
    pub should_quit: bool,
    pub game_match: Match,
    pub human_color: Color,
    /// Moves chosen on the board are sent here and picked up by the [`HumanPlayer`].
    human_moves: Sender<Move>,
    /// Square under the keyboard cursor.
    pub cursor: Square,
    /// Square of the piece the human picked up, if any.
    pub selected: Option<Square>,
    /// Moves played so far, in SAN.
    pub san_history: Vec<String>,
}

impl App {
    /// Constructs a new instance of [`App`].
    pub fn new() -> Self {
        let human_color = Color::White;
        let (human_moves, receiver) = mpsc::channel();
        Self {
            should_quit: false,
            game_match: new_match(human_color, HumanPlayer::new(receiver)),
            human_color,
            human_moves,
            cursor: Square(52), // e2
            selected: None,
            san_history: vec![],
        }
    }

    /// Advances the match; called after every terminal event.
    pub fn tick(&mut self) {
        let before = *self.game_match.game().position();
        if let Ok(Some(m)) = self.game_match.tick() {
            self.san_history.push(before.to_san(&m));
        }
    }

    /// Set should_quit to true to quit the application.
    pub fn quit(&mut self) {
        self.should_quit = true;
    }

    pub fn new_game(&mut self) {
        *self = Self::new();
    }

    pub fn is_human_turn(&self) -> bool {
        let game = self.game_match.game();
        game.status() == Status::Ongoing && game.position().side_to_move() == self.human_color
    }

    /// Moves the cursor by whole ranks/files, clamped to the board.
    /// Positive `rank_delta` moves toward rank 8.
    pub fn move_cursor(&mut self, rank_delta: i8, file_delta: i8) {
        let row = (self.cursor.0 / 8) as i8;
        let file = (self.cursor.0 % 8) as i8;
        let row = (row - rank_delta).clamp(0, 7);
        let file = (file + file_delta).clamp(0, 7);
        self.cursor = Square((row * 8 + file) as u8);
    }

    /// Picks up the piece under the cursor, or drops the selected piece there.
    pub fn select(&mut self) {
        if !self.is_human_turn() {
            return;
        }
        let position = *self.game_match.game().position();

        if let Some(src) = self.selected {
            let chosen = position
                .legal_moves_from(src)
                .into_iter()
                // Promotions are generated queen first; always promote to a queen for now.
                .find(|m| m.dst() == self.cursor.0);
            if let Some(m) = chosen {
                self.selected = None;
                let _ = self.human_moves.send(m);
                return;
            }
        }

        self.selected = if self.selected != Some(self.cursor)
            && !position.legal_moves_from(self.cursor).is_empty()
        {
            Some(self.cursor)
        } else {
            None
        };
    }

    pub fn cancel_selection(&mut self) -> bool {
        self.selected.take().is_some()
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

fn new_match(human_color: Color, human: HumanPlayer) -> Match {
    let human = Box::new(human);
    let computer = Box::new(RandomPlayer::new());
    match human_color {
        Color::White => Match::new(human, computer),
        Color::Black => Match::new(computer, human),
    }
}
