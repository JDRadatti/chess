//! A computer opponent: alpha-beta search over a material and piece-square evaluation.

mod eval;
mod search;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub use search::{Limits, Score, SearchInfo, search};

use crate::player::XorShift64;
use crate::{Game, Move, Player};

/// How far from the best move a casual move may be, in centipawns.
const CASUAL_MARGIN: i32 = 150;

/// Preset strengths, weakest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Beginner,
    Easy,
    Medium,
    Hard,
}

impl Level {
    pub const ALL: [Level; 4] = [Level::Beginner, Level::Easy, Level::Medium, Level::Hard];

    pub const fn name(self) -> &'static str {
        match self {
            Level::Beginner => "Beginner",
            Level::Easy => "Easy",
            Level::Medium => "Medium",
            Level::Hard => "Hard",
        }
    }

    pub fn settings(self) -> EngineSettings {
        let (max_depth, time_per_move, casual_chance) = match self {
            Level::Beginner => (1, None, 0.3),
            Level::Easy => (2, None, 0.0),
            Level::Medium => (64, Some(Duration::from_millis(500)), 0.0),
            Level::Hard => (64, Some(Duration::from_secs(2)), 0.0),
        };
        EngineSettings {
            max_depth,
            time_per_move,
            casual_chance,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EngineSettings {
    /// Deepest search, in plies.
    pub max_depth: u8,
    pub time_per_move: Option<Duration>,
    /// Chance, from 0 to 1, of playing a random move that is nearly as good as the best one
    /// at a glance, instead of the searched move.
    pub casual_chance: f64,
}

/// The latest [`SearchInfo`] from an [`EnginePlayer`], readable from another thread.
#[derive(Debug, Clone, Default)]
pub struct EngineInfo(Arc<Mutex<Option<SearchInfo>>>);

impl EngineInfo {
    /// Progress of the current search, or of the last one once it has finished. `None` until
    /// the first depth of a search completes.
    pub fn latest(&self) -> Option<SearchInfo> {
        self.0.lock().ok()?.clone()
    }

    fn set(&self, info: Option<SearchInfo>) {
        if let Ok(mut latest) = self.0.lock() {
            *latest = info;
        }
    }
}

/// Searches on a background thread so [`Player::poll_move`] never blocks.
pub struct EnginePlayer {
    settings: EngineSettings,
    rng: XorShift64,
    info: EngineInfo,
    thinking: Option<Thinking>,
}

/// A search in progress.
struct Thinking {
    stop: Arc<AtomicBool>,
    result: Receiver<Option<Move>>,
}

impl EnginePlayer {
    pub fn new(settings: EngineSettings) -> Self {
        Self {
            settings,
            rng: XorShift64::from_clock(),
            info: EngineInfo::default(),
            thinking: None,
        }
    }

    pub fn with_level(level: Level) -> Self {
        Self::new(level.settings())
    }

    /// A handle for watching the search from elsewhere, such as a UI.
    pub fn info(&self) -> EngineInfo {
        self.info.clone()
    }

    fn stop_thinking(&mut self) {
        if let Some(thinking) = self.thinking.take() {
            thinking.stop.store(true, Ordering::Relaxed);
        }
    }
}

impl Player for EnginePlayer {
    fn request_move(&mut self, game: &Game) {
        self.stop_thinking();
        self.info.set(None);

        let stop = Arc::new(AtomicBool::new(false));
        let (sender, result) = mpsc::channel();
        let game = game.clone();
        let settings = self.settings;
        let info = self.info.clone();
        let seed = self.rng.next();
        thread::spawn({
            let stop = Arc::clone(&stop);
            move || {
                let chosen = think(&game, &settings, &stop, seed, |progress| {
                    info.set(Some(progress.clone()));
                });
                // The player may have been dropped, for example by starting a new game.
                let _ = sender.send(chosen);
            }
        });
        self.thinking = Some(Thinking { stop, result });
    }

    fn poll_move(&mut self) -> Option<Move> {
        let thinking = self.thinking.as_ref()?;
        match thinking.result.try_recv() {
            Ok(chosen) => {
                self.thinking = None;
                chosen
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.thinking = None;
                None
            }
        }
    }
}

impl Drop for EnginePlayer {
    fn drop(&mut self) {
        self.stop_thinking();
    }
}

/// Picks a move the way a player with `settings` does: searches, then with the settings'
/// casual chance swaps in a random move that looks nearly as good. `seed` drives that
/// randomness. Blocks until done; `stop` ends the search early with the best move so far.
pub fn think(
    game: &Game,
    settings: &EngineSettings,
    stop: &AtomicBool,
    seed: u64,
    on_info: impl FnMut(&SearchInfo),
) -> Option<Move> {
    let limits = Limits {
        max_depth: settings.max_depth,
        time: settings.time_per_move,
    };
    let searched = search(game, &limits, stop, on_info);
    let mut rng = XorShift64::new(seed);
    if rng.chance(settings.casual_chance) {
        casual_move(game, &mut rng).or(searched)
    } else {
        searched
    }
}

/// A random move among those that look nearly as good as the best one.
fn casual_move(game: &Game, rng: &mut XorShift64) -> Option<Move> {
    let scores = search::shallow_scores(game);
    let best = scores.iter().map(|&(_, score)| score).max()?;
    let candidates: Vec<Move> = scores
        .into_iter()
        .filter(|&(_, score)| score >= best - CASUAL_MARGIN)
        .map(|(m, _)| m)
        .collect();
    Some(candidates[rng.below(candidates.len() as u64) as usize])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Color, Match, RandomPlayer, Status};
    use std::time::Instant;

    /// Ticks `m` until the game ends or `max_plies` moves have been played.
    fn play_out(m: &mut Match, max_plies: usize) {
        let deadline = Instant::now() + Duration::from_secs(120);
        while m.game().status() == Status::Ongoing && m.game().moves().len() < max_plies {
            assert!(Instant::now() < deadline, "match took too long");
            if m.tick().unwrap().is_none() {
                thread::sleep(Duration::from_millis(1));
            }
        }
    }

    #[test]
    fn engine_player_finds_mate() {
        let game = Game::from_position("6k1/5ppp/8/8/8/8/8/R5K1 w - - 0 1".parse().unwrap());
        let engine = EnginePlayer::with_level(Level::Easy);
        let info = engine.info();
        let mut m = Match::from_game(game, Box::new(engine), Box::new(RandomPlayer::with_seed(1)));
        play_out(&mut m, 1);
        assert_eq!(
            m.game().status(),
            Status::Checkmate {
                winner: Color::White
            }
        );
        assert_eq!(info.latest().unwrap().score, Score::Mate(1));
    }

    #[test]
    fn easy_engine_beats_random_player() {
        let mut m = Match::new(
            Box::new(EnginePlayer::with_level(Level::Easy)),
            Box::new(RandomPlayer::with_seed(7)),
        );
        play_out(&mut m, 300);
        assert_eq!(
            m.game().status(),
            Status::Checkmate {
                winner: Color::White
            },
            "after {} plies",
            m.game().moves().len()
        );
    }

    #[test]
    fn casual_moves_are_reasonable() {
        // Taking the free queen is the only move within the margin.
        let game = Game::from_position("4k3/8/8/3q4/8/8/3R4/4K3 w - - 0 1".parse().unwrap());
        let mut rng = XorShift64::new(3);
        for _ in 0..10 {
            assert_eq!(casual_move(&game, &mut rng), Some("d2-d5".parse().unwrap()));
        }
    }

    #[test]
    fn dropping_the_player_stops_its_search() {
        let mut engine = EnginePlayer::new(EngineSettings {
            max_depth: 64,
            time_per_move: None,
            casual_chance: 0.0,
        });
        engine.request_move(&Game::new());
        let stop = Arc::clone(&engine.thinking.as_ref().unwrap().stop);
        drop(engine);
        assert!(stop.load(Ordering::Relaxed));
    }
}
