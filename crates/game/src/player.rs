use std::time::{SystemTime, UNIX_EPOCH};

use crate::{Color, Error, Game, Move, Status};

/// Something that chooses moves: a human at a UI, a search, a network peer, etc.
///
/// Players must not block. [`Match::tick`] calls `request_move` once when it becomes this
/// player's turn, then calls `poll_move` on every tick until a move is returned.
pub trait Player {
    fn request_move(&mut self, game: &Game);
    fn poll_move(&mut self) -> Option<Move>;
}

/// Drives a [`Game`] between two players.
pub struct Match {
    game: Game,
    white: Box<dyn Player>,
    black: Box<dyn Player>,
    /// Whether the player to move has been sent `request_move` for this turn.
    requested: bool,
}

impl Match {
    pub fn new(white: Box<dyn Player>, black: Box<dyn Player>) -> Self {
        Self::from_game(Game::new(), white, black)
    }

    pub fn from_game(game: Game, white: Box<dyn Player>, black: Box<dyn Player>) -> Self {
        Self {
            game,
            white,
            black,
            requested: false,
        }
    }

    pub fn game(&self) -> &Game {
        &self.game
    }

    /// Advances the match by at most one move.
    ///
    /// Returns the move played this tick, if any. An illegal move from a player is rejected
    /// with `Err(Error::InvalidMove)` and that player is asked again on the next tick.
    pub fn tick(&mut self) -> Result<Option<Move>, Error> {
        if self.game.status() != Status::Ongoing {
            return Ok(None);
        }

        let player = match self.game.position().side_to_move() {
            Color::White => &mut self.white,
            Color::Black => &mut self.black,
        };

        if !self.requested {
            player.request_move(&self.game);
            self.requested = true;
        }

        let Some(m) = player.poll_move() else {
            return Ok(None);
        };
        self.requested = false;
        self.game.play(m)?;
        Ok(Some(m))
    }
}

/// Plays a uniformly random legal move.
pub struct RandomPlayer {
    rng: XorShift64,
    chosen: Option<Move>,
}

impl RandomPlayer {
    /// Seeded from the system clock, so each game differs.
    pub fn new() -> Self {
        Self {
            rng: XorShift64::from_clock(),
            chosen: None,
        }
    }

    /// Same seed, same moves. Useful for reproducible tests.
    pub fn with_seed(seed: u64) -> Self {
        Self {
            rng: XorShift64::new(seed),
            chosen: None,
        }
    }
}

impl Default for RandomPlayer {
    fn default() -> Self {
        Self::new()
    }
}

impl Player for RandomPlayer {
    fn request_move(&mut self, game: &Game) {
        let moves = game.position().legal_moves();
        self.chosen = if moves.is_empty() {
            None
        } else {
            Some(moves[self.rng.below(moves.len() as u64) as usize])
        };
    }

    fn poll_move(&mut self) -> Option<Move> {
        self.chosen.take()
    }
}

pub(crate) struct XorShift64(u64);

impl XorShift64 {
    pub(crate) fn new(seed: u64) -> Self {
        // A zero state would only ever produce zeros.
        Self(seed | 1)
    }

    pub(crate) fn from_clock() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64);
        Self::new(nanos)
    }

    pub(crate) fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    pub(crate) fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    /// True with probability `p`, from 0 to 1.
    pub(crate) fn chance(&mut self, p: f64) -> bool {
        let unit = (self.next() >> 11) as f64 / (1u64 << 53) as f64;
        unit < p
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DrawReason;

    /// Plays a fixed list of moves, then nothing.
    struct Scripted(Vec<Move>);

    impl Player for Scripted {
        fn request_move(&mut self, _game: &Game) {}
        fn poll_move(&mut self) -> Option<Move> {
            if self.0.is_empty() {
                None
            } else {
                Some(self.0.remove(0))
            }
        }
    }

    fn mv(s: &str) -> Move {
        s.parse().unwrap()
    }

    #[test]
    fn random_vs_random_finishes() {
        let mut m = Match::new(
            Box::new(RandomPlayer::with_seed(1)),
            Box::new(RandomPlayer::with_seed(2)),
        );
        // A random game always ends: the fifty-move rule caps it.
        for _ in 0..10_000 {
            m.tick().unwrap();
            if m.game().status() != Status::Ongoing {
                return;
            }
        }
        panic!("random game did not finish");
    }

    #[test]
    fn scripted_fools_mate() {
        let mut m = Match::new(
            Box::new(Scripted(vec![mv("f2-f3"), mv("g2-g4")])),
            Box::new(Scripted(vec![mv("e7-e5"), mv("d8-h4")])),
        );
        for _ in 0..4 {
            assert!(m.tick().unwrap().is_some());
        }
        assert_eq!(
            m.game().status(),
            Status::Checkmate {
                winner: Color::Black
            }
        );
        assert_eq!(m.tick().unwrap(), None);
    }

    #[test]
    fn waiting_player_stalls_without_error() {
        let mut m = Match::new(Box::new(Scripted(vec![])), Box::new(Scripted(vec![])));
        assert_eq!(m.tick().unwrap(), None);
        assert!(m.game().moves().is_empty());
        assert_eq!(m.game().status(), Status::Ongoing);
    }

    #[test]
    fn illegal_move_is_rejected_and_retried() {
        let mut m = Match::new(
            Box::new(Scripted(vec![mv("e2-e5"), mv("e2-e4")])),
            Box::new(Scripted(vec![])),
        );
        assert!(matches!(m.tick(), Err(Error::InvalidMove)));
        assert_eq!(m.tick().unwrap(), Some(mv("e2-e4")));
    }

    #[test]
    fn match_reports_repetition() {
        let white = Scripted(vec![mv("g1-f3"), mv("f3-g1"), mv("g1-f3"), mv("f3-g1")]);
        let black = Scripted(vec![mv("g8-f6"), mv("f6-g8"), mv("g8-f6"), mv("f6-g8")]);
        let mut m = Match::new(Box::new(white), Box::new(black));
        for _ in 0..8 {
            m.tick().unwrap();
        }
        assert_eq!(
            m.game().status(),
            Status::Draw(DrawReason::ThreefoldRepetition)
        );
    }
}
