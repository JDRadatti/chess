//! Iterative-deepening negamax with alpha-beta pruning and a quiescence search.

use std::cmp::Reverse;
use std::fmt::{self, Display, Formatter};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::eval::{evaluate, piece_value};
use crate::{
    Color, EMPTY, FIFTY_MOVE_RULE_PLIES, Game, Move, PAWN_TYPE, Position, QUEEN_TYPE, Square,
    Status, piece_type, sq_file,
};

/// Score for delivering mate right now. Mates further away score a little less, so the
/// search prefers the quickest mate and the slowest loss.
const MATE: i32 = 30_000;
/// Greater than any real score.
const INFINITY: i32 = 32_000;
/// Deepest ply the search ever reaches, quiescence included.
const MAX_PLY: usize = 64;
/// Scores beyond this are mates.
const MATE_BOUND: i32 = MATE - MAX_PLY as i32;
/// How many nodes pass between checks of the clock and the stop flag.
const CHECK_INTERVAL: u64 = 1024;

/// When to stop searching.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Deepest full-width search, in plies.
    pub max_depth: u8,
    /// Time to spend on the move. The first ply is always finished, however long it takes.
    pub time: Option<Duration>,
}

/// An evaluation from White's point of view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Score {
    /// Hundredths of a pawn; positive means White is better.
    Centipawns(i32),
    /// Moves until mate; positive when White mates, negative when Black does.
    Mate(i32),
}

impl Score {
    /// Converts a search score, relative to `side`, to White's point of view.
    fn new(score: i32, side: Color) -> Self {
        let sign = if side == Color::White { 1 } else { -1 };
        if score > MATE_BOUND {
            Score::Mate(sign * (MATE - score + 1) / 2)
        } else if score < -MATE_BOUND {
            Score::Mate(-sign * (MATE + score) / 2)
        } else {
            Score::Centipawns(sign * score)
        }
    }
}

/// `+0.35`, `-1.20`, `M3` (White mates in three), or `-M2` (Black mates in two).
impl Display for Score {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match *self {
            Score::Centipawns(cp) => {
                let sign = if cp < 0 { '-' } else { '+' };
                write!(f, "{sign}{}.{:02}", cp.abs() / 100, cp.abs() % 100)
            }
            Score::Mate(moves) if moves < 0 => write!(f, "-M{}", -moves),
            Score::Mate(moves) => write!(f, "M{moves}"),
        }
    }
}

/// Progress report after each completed depth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchInfo {
    pub depth: u8,
    pub score: Score,
    /// Positions visited so far in this search.
    pub nodes: u64,
    pub elapsed: Duration,
    /// Best line found: the move to play, the expected reply, and so on.
    pub pv: Vec<Move>,
}

/// Finds the best move for the side to move in `game`.
///
/// Searches one ply deeper at a time until `limits` or `stop` end it, reporting each finished
/// depth to `on_info`. Returns `None` only when the game is over.
pub fn search(
    game: &Game,
    limits: &Limits,
    stop: &AtomicBool,
    mut on_info: impl FnMut(&SearchInfo),
) -> Option<Move> {
    if game.status() != Status::Ongoing {
        return None;
    }
    let root = *game.position();
    let moves = root.legal_moves();
    if let [only] = moves.as_slice() {
        return Some(*only);
    }

    let start = Instant::now();
    let history = game.history();
    // The root pushes its own key.
    let mut searcher = Searcher::new(stop, &history[..history.len() - 1]);
    let mut best = moves[0];

    for depth in 1..=limits.max_depth.max(1) {
        searcher.deadline = if depth == 1 {
            None
        } else {
            limits.time.map(|time| start + time)
        };
        let score = searcher.negamax(&root, i32::from(depth), 0, -INFINITY, INFINITY, true);
        if searcher.aborted {
            // The root only records fully searched moves, and the previous best is searched
            // first, so whatever it recorded is at least as good as that.
            if let Some(&m) = searcher.pv[0].first() {
                best = m;
            }
            break;
        }
        best = searcher.pv[0][0];
        searcher.previous_pv = searcher.pv[0].clone();
        on_info(&SearchInfo {
            depth,
            score: Score::new(score, root.side_to_move()),
            nodes: searcher.nodes,
            elapsed: start.elapsed(),
            pv: searcher.previous_pv.clone(),
        });

        let mate_found = score.abs() > MATE_BOUND && i32::from(depth) >= MATE - score.abs();
        // The next depth takes several times longer than this one; don't start what can't finish.
        let out_of_time = limits.time.is_some_and(|time| start.elapsed() * 2 > time);
        if mate_found || out_of_time {
            break;
        }
    }
    Some(best)
}

/// Scores every legal move with a one-ply search plus quiescence, from the mover's side.
pub(crate) fn shallow_scores(game: &Game) -> Vec<(Move, i32)> {
    let stop = AtomicBool::new(false);
    let mut searcher = Searcher::new(&stop, game.history());
    let root = *game.position();
    root.legal_moves()
        .into_iter()
        .map(|m| {
            let mut child = root;
            child.apply_move_unchecked(m);
            let score = -searcher.negamax(&child, 0, 1, -INFINITY, INFINITY, false);
            (m, score)
        })
        .collect()
}

struct Searcher<'a> {
    stop: &'a AtomicBool,
    deadline: Option<Instant>,
    nodes: u64,
    /// Set once the search runs out of time or is stopped; every score after that is garbage.
    aborted: bool,
    /// Repetition keys of the game so far followed by the current search path.
    keys: Vec<[u8; 34]>,
    /// `pv[ply]` is the best line found from the node at `ply`.
    pv: Vec<Vec<Move>>,
    /// Best line from the previous depth, searched first.
    previous_pv: Vec<Move>,
    /// `move_lists[ply]` holds the moves of the node at `ply`, reused so searching doesn't
    /// allocate.
    move_lists: Vec<Vec<Move>>,
}

impl<'a> Searcher<'a> {
    /// `keys` are the repetition keys of the positions before the first one searched.
    fn new(stop: &'a AtomicBool, keys: &[[u8; 34]]) -> Self {
        Self {
            stop,
            deadline: None,
            nodes: 0,
            aborted: false,
            keys: keys.to_vec(),
            pv: vec![vec![]; MAX_PLY + 1],
            previous_pv: vec![],
            move_lists: vec![vec![]; MAX_PLY + 1],
        }
    }

    /// The legal moves of `position`, in the list reserved for `ply`. Hand the list back with
    /// [`Self::return_moves`] when done.
    fn take_moves(&mut self, position: &Position, ply: usize) -> Vec<Move> {
        let mut moves = std::mem::take(&mut self.move_lists[ply]);
        moves.clear();
        position.generate_legal_moves(!0, &mut moves);
        moves
    }

    fn return_moves(&mut self, ply: usize, moves: Vec<Move>) {
        self.move_lists[ply] = moves;
    }
    /// Best score for the side to move, searching `depth` more plies.
    ///
    /// `on_pv` is true while following the previous depth's best line from the root.
    fn negamax(
        &mut self,
        position: &Position,
        depth: i32,
        ply: usize,
        mut alpha: i32,
        beta: i32,
        on_pv: bool,
    ) -> i32 {
        self.pv[ply].clear();
        if self.should_stop() {
            return 0;
        }

        let key = position.repetition_key();
        if ply > 0 && (Self::is_draw(position) || self.is_repetition(position, &key)) {
            return 0;
        }

        let in_check = position.is_check();
        // Don't stop searching while in check: the position isn't quiet, and mates are often
        // just past the horizon.
        let depth = if in_check { depth + 1 } else { depth };
        if depth <= 0 || ply >= MAX_PLY / 2 {
            return self.quiesce(position, ply, alpha, beta);
        }

        let mut moves = self.take_moves(position, ply);
        if moves.is_empty() {
            self.return_moves(ply, moves);
            return if in_check { -MATE + ply as i32 } else { 0 };
        }
        let pv_move = if on_pv {
            self.previous_pv.get(ply).copied()
        } else {
            None
        };
        order_moves(position, &mut moves, pv_move);

        self.keys.push(key);
        let mut best = -INFINITY;
        for &m in &moves {
            let mut child = *position;
            child.apply_move_unchecked(m);
            let child_on_pv = pv_move == Some(m);
            let score = -self.negamax(&child, depth - 1, ply + 1, -beta, -alpha, child_on_pv);
            if self.aborted {
                break;
            }
            best = best.max(score);
            if score > alpha {
                alpha = score;
                self.record_pv(ply, m);
                if alpha >= beta {
                    break;
                }
            }
        }
        self.keys.pop();
        self.return_moves(ply, moves);
        best
    }

    /// Searches only captures and queen promotions (or every evasion when in check) until the
    /// position is quiet, so the evaluation never stops in the middle of an exchange.
    fn quiesce(&mut self, position: &Position, ply: usize, mut alpha: i32, beta: i32) -> i32 {
        if self.should_stop() {
            return 0;
        }
        if ply >= MAX_PLY {
            return evaluate(position);
        }
        let in_check = position.is_check();

        let mut best = -INFINITY;
        if !in_check {
            // "Standing pat": the side to move can usually do at least as well as doing nothing.
            best = evaluate(position);
            if best >= beta {
                return best;
            }
            alpha = alpha.max(best);
        }

        let mut moves = self.take_moves(position, ply);
        if moves.is_empty() {
            self.return_moves(ply, moves);
            return if in_check { -MATE + ply as i32 } else { 0 };
        }
        if !in_check {
            moves.retain(|&m| is_capture(position, m) || m.promo() == QUEEN_TYPE);
        }
        order_moves(position, &mut moves, None);

        for &m in &moves {
            let mut child = *position;
            child.apply_move_unchecked(m);
            let score = -self.quiesce(&child, ply + 1, -beta, -alpha);
            if self.aborted {
                break;
            }
            best = best.max(score);
            if score > alpha {
                alpha = score;
                if alpha >= beta {
                    break;
                }
            }
        }
        self.return_moves(ply, moves);
        best
    }

    fn should_stop(&mut self) -> bool {
        self.nodes += 1;
        if self.nodes.is_multiple_of(CHECK_INTERVAL)
            && (self.stop.load(Ordering::Relaxed)
                || self.deadline.is_some_and(|d| Instant::now() >= d))
        {
            self.aborted = true;
        }
        self.aborted
    }

    /// Draws that don't depend on the moves: the fifty-move rule and bare material.
    fn is_draw(position: &Position) -> bool {
        position.halfmove_cnt() >= FIFTY_MOVE_RULE_PLIES || position.is_insufficient_material()
    }

    /// Whether `key` occurred before, in the game or on the way here. A single repeat counts
    /// as a draw: if repeating is good once, it is good again, so the result is the same.
    fn is_repetition(&self, position: &Position, key: &[u8; 34]) -> bool {
        // Positions before the last capture or pawn move can't come back.
        let reversible = usize::from(position.halfmove_cnt());
        self.keys.iter().rev().take(reversible).any(|k| k == key)
    }

    /// Makes `m` followed by the child's best line the best line from `ply`.
    fn record_pv(&mut self, ply: usize, m: Move) {
        let (here, below) = self.pv.split_at_mut(ply + 1);
        let line = &mut here[ply];
        line.clear();
        line.push(m);
        line.extend_from_slice(&below[0]);
    }
}

fn is_capture(position: &Position, m: Move) -> bool {
    let target = position.square_bits(Square(m.dst()));
    let mover = position.square_bits(Square(m.src()));
    let en_passant = piece_type(mover) == PAWN_TYPE && sq_file(m.src()) != sq_file(m.dst());
    target != EMPTY || en_passant
}

/// Sorts the most promising moves first, so alpha-beta can cut off the rest sooner: the best
/// move from the previous depth, then captures of big pieces by small ones, then promotions.
fn order_moves(position: &Position, moves: &mut [Move], pv_move: Option<Move>) {
    // Unstable sorting doesn't allocate; ties keep a deterministic, if arbitrary, order.
    moves.sort_unstable_by_key(|&m| {
        let priority = if Some(m) == pv_move {
            1_000_000
        } else if is_capture(position, m) {
            let victim = piece_value(piece_type(position.square_bits(Square(m.dst()))));
            let attacker = piece_value(piece_type(position.square_bits(Square(m.src()))));
            // En passant has no piece on the target square; it's a pawn for a pawn.
            let victim = if victim == 0 {
                piece_value(PAWN_TYPE)
            } else {
                victim
            };
            100_000 + 10 * victim - attacker
        } else if m.promo() != 0 {
            50_000 + piece_value(m.promo())
        } else {
            0
        };
        Reverse(priority)
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mv(s: &str) -> Move {
        s.parse().unwrap()
    }

    fn game(fen: &str) -> Game {
        Game::from_position(fen.parse().unwrap())
    }

    fn best(game: &Game, depth: u8) -> (Move, Vec<SearchInfo>) {
        let stop = AtomicBool::new(false);
        let mut infos = vec![];
        let limits = Limits {
            max_depth: depth,
            time: None,
        };
        let m = search(game, &limits, &stop, |info| infos.push(info.clone())).unwrap();
        (m, infos)
    }

    #[test]
    fn mate_in_one() {
        let g = game("6k1/5ppp/8/8/8/8/8/R5K1 w - - 0 1");
        let (m, infos) = best(&g, 3);
        assert_eq!(m, mv("a1-a8"));
        assert_eq!(infos.last().unwrap().score, Score::Mate(1));
    }

    #[test]
    fn mate_in_two() {
        // 1. Rxe8+ Qxe8 2. Rxe8#
        let g = game("3qr1k1/5ppp/8/8/8/8/4RPPP/4R1K1 w - - 0 1");
        let (m, infos) = best(&g, 4);
        assert_eq!(m, mv("e2-e8"));
        let info = infos.last().unwrap();
        assert_eq!(info.score, Score::Mate(2));
        assert_eq!(info.pv, [mv("e2-e8"), mv("d8-e8"), mv("e1-e8")]);
    }

    #[test]
    fn black_mate_is_negative() {
        let g = game("r5k1/8/8/8/8/8/5PPP/6K1 b - - 0 1");
        let (m, infos) = best(&g, 3);
        assert_eq!(m, mv("a8-a1"));
        assert_eq!(infos.last().unwrap().score, Score::Mate(-1));
    }

    #[test]
    fn takes_a_free_queen() {
        let g = game("4k3/8/8/3q4/8/8/3R4/4K3 w - - 0 1");
        assert_eq!(best(&g, 2).0, mv("d2-d5"));
    }

    #[test]
    fn does_not_trade_queen_for_defended_pawn() {
        let g = game("4k3/8/4p3/3p4/8/8/8/3QK3 w - - 0 1");
        assert_ne!(best(&g, 2).0, mv("d1-d5"));
    }

    #[test]
    fn mates_instead_of_stalemating() {
        // Qc7 would be stalemate; Qc8 is mate.
        let mut g = game("k7/8/1K6/8/8/8/8/2Q5 w - - 0 1");
        let (m, _) = best(&g, 3);
        g.play(m).unwrap();
        assert_eq!(
            g.status(),
            Status::Checkmate {
                winner: Color::White
            }
        );
    }

    /// King and queen against a bare king, both kings shuffling back and forth.
    fn shuffle(moves: &[&str]) -> Game {
        let mut g = game("7k/8/8/8/8/8/8/K2Q4 w - - 0 1");
        for m in moves {
            g.play(mv(m)).unwrap();
        }
        g
    }

    #[test]
    fn avoids_repetition_when_winning() {
        let g = shuffle(&["a1-a2", "h8-g8", "a2-a1", "g8-h8", "a1-a2", "h8-g8"]);
        assert_ne!(best(&g, 3).0, mv("a2-a1"));
    }

    #[test]
    fn repeats_when_losing() {
        // Black's g8-h8 makes the third occurrence: a draw instead of a lost endgame.
        let g = shuffle(&[
            "a1-a2", "h8-g8", "a2-a1", "g8-h8", "a1-a2", "h8-g8", "a2-a1",
        ]);
        assert_eq!(best(&g, 3).0, mv("g8-h8"));
    }

    #[test]
    fn prefers_fifty_move_draw_when_losing() {
        // Black is a queen down, but any quiet king move ends the game by the fifty-move rule.
        let g = game("7k/8/8/8/8/8/8/K2Q4 b - - 99 80");
        let (_, infos) = best(&g, 3);
        assert_eq!(infos.last().unwrap().score, Score::Centipawns(0));
    }

    #[test]
    fn returns_none_when_game_is_over() {
        let g = game("k7/1Q6/1K6/8/8/8/8/8 b - - 0 1");
        let limits = Limits {
            max_depth: 3,
            time: None,
        };
        assert_eq!(search(&g, &limits, &AtomicBool::new(false), |_| {}), None);
    }

    #[test]
    fn single_legal_move_is_played_at_once() {
        let g = game("7k/8/8/8/8/8/8/5KR1 b - - 0 1");
        let limits = Limits {
            max_depth: 64,
            time: None,
        };
        let m = search(&g, &limits, &AtomicBool::new(false), |_| {});
        assert_eq!(m, Some(mv("h8-h7")));
    }

    #[test]
    fn respects_time_limit() {
        let g = Game::new();
        let limits = Limits {
            max_depth: 64,
            time: Some(Duration::from_millis(200)),
        };
        let start = Instant::now();
        let m = search(&g, &limits, &AtomicBool::new(false), |_| {});
        assert!(g.position().legal_moves().contains(&m.unwrap()));
        assert!(
            start.elapsed() < Duration::from_millis(1000),
            "{:?}",
            start.elapsed()
        );
    }

    #[test]
    fn stop_flag_ends_search_with_a_legal_move() {
        let g = Game::new();
        let limits = Limits {
            max_depth: 64,
            time: None,
        };
        let stop = AtomicBool::new(true);
        let start = Instant::now();
        let m = search(&g, &limits, &stop, |_| {});
        assert!(g.position().legal_moves().contains(&m.unwrap()));
        assert!(start.elapsed() < Duration::from_millis(500));
    }

    #[test]
    fn score_display() {
        assert_eq!(Score::Centipawns(35).to_string(), "+0.35");
        assert_eq!(Score::Centipawns(-120).to_string(), "-1.20");
        assert_eq!(Score::Centipawns(-5).to_string(), "-0.05");
        assert_eq!(Score::Mate(3).to_string(), "M3");
        assert_eq!(Score::Mate(-2).to_string(), "-M2");
    }
}
