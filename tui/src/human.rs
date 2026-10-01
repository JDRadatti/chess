use std::sync::mpsc::Receiver;

use game::{Move, Player, Position};

/// A player whose moves come from the keyboard, via the sender kept in [`crate::app::App`].
pub struct HumanPlayer {
    moves: Receiver<Move>,
}

impl HumanPlayer {
    pub fn new(moves: Receiver<Move>) -> Self {
        Self { moves }
    }
}

impl Player for HumanPlayer {
    fn request_move(&mut self, _position: &Position) {}

    fn poll_move(&mut self) -> Option<Move> {
        self.moves.try_recv().ok()
    }
}
