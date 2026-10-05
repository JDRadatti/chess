//! Parsing and formatting for the UCI protocol, kept free of I/O so it can be tested.

use std::time::Duration;

use game::engine::{EngineSettings, Level, Score, SearchInfo};
use game::{Color, Game, Move, PieceKind};

/// How strongly the engine plays: one of the app's levels, or everything it has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Strength {
    /// Plays like the level in the app, but never thinks longer than the clock allows.
    Level(Level),
    /// Searches as deep as its share of the clock allows.
    Full,
}

impl Strength {
    pub(crate) const ALL: [Strength; 5] = [
        Strength::Level(Level::Beginner),
        Strength::Level(Level::Easy),
        Strength::Level(Level::Medium),
        Strength::Level(Level::Hard),
        Strength::Full,
    ];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Strength::Level(level) => level.name(),
            Strength::Full => "Full",
        }
    }

    pub(crate) fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|s| s.name().eq_ignore_ascii_case(name))
    }
}

/// Arguments of a `go` command. Times are in milliseconds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Go {
    pub wtime: Option<u64>,
    pub btime: Option<u64>,
    pub winc: Option<u64>,
    pub binc: Option<u64>,
    pub movestogo: Option<u64>,
    pub movetime: Option<u64>,
    pub depth: Option<u8>,
    pub infinite: bool,
}

impl Go {
    /// Reads the words after `go`, ignoring ones it doesn't use, such as `ponder` or `nodes`.
    pub(crate) fn parse(args: &[&str]) -> Self {
        let mut go = Go::default();
        let mut words = args.iter();
        while let Some(&word) = words.next() {
            let mut number = || words.next().and_then(|w| w.parse::<u64>().ok());
            match word {
                "wtime" => go.wtime = number(),
                "btime" => go.btime = number(),
                "winc" => go.winc = number(),
                "binc" => go.binc = number(),
                "movestogo" => go.movestogo = number(),
                "movetime" => go.movetime = number(),
                "depth" => go.depth = number().map(|d| d.clamp(1, 64) as u8),
                "infinite" => go.infinite = true,
                _ => {}
            }
        }
        go
    }

    /// How long `side` may think, or `None` to think until told to stop.
    ///
    /// Plans for `movestogo` more moves (30 if unknown) plus most of the increment, and always
    /// keeps `overhead` in hand for the time it takes the move to reach the GUI.
    pub(crate) fn time_budget(&self, side: Color, overhead: Duration) -> Option<Duration> {
        if self.infinite {
            return None;
        }
        let at_least_1ms = |ms: u64| Duration::from_millis(ms.max(1));
        let overhead = u64::try_from(overhead.as_millis()).unwrap_or(u64::MAX);
        if let Some(movetime) = self.movetime {
            return Some(at_least_1ms(movetime.saturating_sub(overhead)));
        }
        let (time, increment) = match side {
            Color::White => (self.wtime?, self.winc.unwrap_or(0)),
            Color::Black => (self.btime?, self.binc.unwrap_or(0)),
        };
        let moves_left = self.movestogo.unwrap_or(30).max(1);
        let planned = time / moves_left + increment * 3 / 4;
        Some(at_least_1ms(planned.min(time.saturating_sub(overhead))))
    }
}

/// Search settings for one move at `strength`.
pub(crate) fn settings(
    strength: Strength,
    go: &Go,
    side: Color,
    overhead: Duration,
) -> EngineSettings {
    let mut settings = match strength {
        Strength::Level(level) => level.settings(),
        Strength::Full => EngineSettings {
            max_depth: 64,
            time_per_move: None,
            casual_chance: 0.0,
        },
    };
    settings.time_per_move = match (settings.time_per_move, go.time_budget(side, overhead)) {
        (Some(level), Some(clock)) => Some(level.min(clock)),
        (level, clock) => level.or(clock),
    };
    if let Some(depth) = go.depth {
        settings.max_depth = settings.max_depth.min(depth);
    }
    settings
}

/// The game a `position` command describes: `startpos` or `fen <six fields>`, then optionally
/// `moves` followed by moves in UCI notation.
pub(crate) fn parse_position(args: &[&str]) -> Result<Game, String> {
    let moves_at = args.iter().position(|&w| w == "moves");
    let (setup, moves) = match moves_at {
        Some(i) => (&args[..i], &args[i + 1..]),
        None => (args, &[][..]),
    };
    let mut game = match setup {
        ["startpos"] => Game::new(),
        ["fen", fen @ ..] => {
            let mut fen = fen.join(" ");
            if fen.split_whitespace().count() == 4 {
                fen.push_str(" 0 1");
            }
            Game::from_position(fen.parse().map_err(|e| format!("bad FEN: {e}"))?)
        }
        _ => return Err("expected `startpos` or `fen`".to_string()),
    };
    for text in moves {
        let m = parse_move(text)?;
        game.play(m)
            .map_err(|e| format!("can't play {text}: {e}"))?;
    }
    Ok(game)
}

/// `e2e4` or `e7e8q`. Castling is the king's two-square move, as in UCI.
pub(crate) fn parse_move(text: &str) -> Result<Move, String> {
    let bad = || format!("bad move `{text}`");
    if !text.is_ascii() || !(4..=5).contains(&text.len()) {
        return Err(bad());
    }
    let (from, rest) = text.split_at(2);
    let (to, promotion) = rest.split_at(2);
    let ours = if promotion.is_empty() {
        format!("{from}-{to}")
    } else {
        format!("{from}-{to}-{promotion}")
    };
    ours.parse().map_err(|_| bad())
}

pub(crate) fn format_move(m: Move) -> String {
    let square = |sq: u8| format!("{}{}", (b'a' + sq % 8) as char, (b'8' - sq / 8) as char);
    let promotion = match m.promotion() {
        Some(PieceKind::Queen) => "q",
        Some(PieceKind::Rook) => "r",
        Some(PieceKind::Bishop) => "b",
        Some(PieceKind::Knight) => "n",
        _ => "",
    };
    format!("{}{}{promotion}", square(m.src()), square(m.dst()))
}

/// An `info` line for a finished depth. UCI scores are from the side to move's point of view;
/// ours are from White's.
pub(crate) fn format_info(info: &SearchInfo, side: Color) -> String {
    let sign = if side == Color::White { 1 } else { -1 };
    let score = match info.score {
        Score::Centipawns(cp) => format!("cp {}", sign * cp),
        Score::Mate(moves) => format!("mate {}", sign * moves),
    };
    let millis = info.elapsed.as_millis().max(1);
    let nps = u128::from(info.nodes) * 1000 / millis;
    let pv: Vec<String> = info.pv.iter().map(|&m| format_move(m)).collect();
    format!(
        "info depth {} score {score} nodes {} nps {nps} time {millis} pv {}",
        info.depth,
        info.nodes,
        pv.join(" ")
    )
}

/// The name and value of `setoption name <name> value <value>`; both may contain spaces.
pub(crate) fn parse_setoption(args: &[&str]) -> Option<(String, String)> {
    let ["name", rest @ ..] = args else {
        return None;
    };
    let value_at = rest.iter().position(|&w| w == "value");
    let (name, value) = match value_at {
        Some(i) => (&rest[..i], &rest[i + 1..]),
        None => (rest, &[][..]),
    };
    Some((name.join(" "), value.join(" ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(s: &str) -> Vec<&str> {
        s.split_whitespace().collect()
    }

    #[test]
    fn parses_go_arguments() {
        let go = Go::parse(&words(
            "wtime 60000 btime 50000 winc 600 binc 500 movestogo 20",
        ));
        assert_eq!(go.wtime, Some(60_000));
        assert_eq!(go.binc, Some(500));
        assert_eq!(go.movestogo, Some(20));
        assert!(!go.infinite);
        let go = Go::parse(&words("depth 5 ponder infinite"));
        assert_eq!(go.depth, Some(5));
        assert!(go.infinite);
    }

    #[test]
    fn budgets_time_from_the_clock() {
        let overhead = Duration::from_millis(30);
        let go = Go::parse(&words("wtime 60000 btime 30000 winc 600 binc 600"));
        assert_eq!(
            go.time_budget(Color::White, overhead),
            Some(Duration::from_millis(2000 + 450))
        );
        assert_eq!(
            go.time_budget(Color::Black, overhead),
            Some(Duration::from_millis(1000 + 450))
        );
        // Nearly flagged: never plan past what's left after the overhead.
        let go = Go::parse(&words("wtime 100 winc 1000"));
        assert_eq!(
            go.time_budget(Color::White, overhead),
            Some(Duration::from_millis(70))
        );
        let go = Go::parse(&words("movetime 1000"));
        assert_eq!(
            go.time_budget(Color::Black, overhead),
            Some(Duration::from_millis(970))
        );
        assert_eq!(
            Go::parse(&words("infinite")).time_budget(Color::White, overhead),
            None
        );
        assert_eq!(
            Go::parse(&words("depth 4")).time_budget(Color::White, overhead),
            None
        );
    }

    #[test]
    fn levels_keep_their_own_pace_unless_the_clock_is_short() {
        let overhead = Duration::from_millis(30);
        let roomy = Go::parse(&words("wtime 300000 btime 300000 winc 3000 binc 3000"));
        let hard = settings(Strength::Level(Level::Hard), &roomy, Color::White, overhead);
        assert_eq!(hard.time_per_move, Some(Duration::from_secs(2)));
        let easy = settings(Strength::Level(Level::Easy), &roomy, Color::White, overhead);
        assert_eq!(easy.max_depth, 2);

        let short = Go::parse(&words("wtime 10000 btime 10000 winc 100 binc 100"));
        let hard = settings(Strength::Level(Level::Hard), &short, Color::White, overhead);
        assert!(hard.time_per_move.unwrap() < Duration::from_millis(500));

        let full = settings(Strength::Full, &roomy, Color::White, overhead);
        assert_eq!(full.max_depth, 64);
        assert_eq!(
            full.time_per_move,
            Some(Duration::from_millis(10_000 + 2250))
        );
        let fixed = settings(
            Strength::Full,
            &Go::parse(&words("depth 3")),
            Color::White,
            overhead,
        );
        assert_eq!((fixed.max_depth, fixed.time_per_move), (3, None));
    }

    #[test]
    fn parses_positions() {
        let game = parse_position(&words("startpos moves e2e4 e7e5 g1f3")).unwrap();
        assert_eq!(game.moves().len(), 3);
        assert_eq!(game.position().side_to_move(), Color::Black);

        let game = parse_position(&words("fen 8/P6k/8/8/8/8/8/K7 w - - 0 1 moves a7a8n")).unwrap();
        assert_eq!(game.position().to_fen(), "N7/7k/8/8/8/8/8/K7 b - - 0 1");

        assert!(parse_position(&words("startpos moves e2e5")).is_err());
        assert!(parse_position(&words("somewhere")).is_err());
    }

    #[test]
    fn moves_round_trip() {
        for text in ["e2e4", "a7a8q", "h2h1n", "e1g1", "e8c8"] {
            assert_eq!(format_move(parse_move(text).unwrap()), text);
        }
        assert!(parse_move("e2").is_err());
        assert!(parse_move("e2e4qq").is_err());
        assert!(parse_move("z9e4").is_err());
    }

    #[test]
    fn info_scores_are_from_the_side_to_move() {
        let info = SearchInfo {
            depth: 4,
            score: Score::Centipawns(-35),
            nodes: 20_000,
            elapsed: Duration::from_millis(10),
            pv: vec![parse_move("e7e5").unwrap()],
        };
        assert_eq!(
            format_info(&info, Color::Black),
            "info depth 4 score cp 35 nodes 20000 nps 2000000 time 10 pv e7e5"
        );
        let mate = SearchInfo {
            score: Score::Mate(-2),
            ..info
        };
        assert!(format_info(&mate, Color::White).contains("score mate -2"));
    }

    #[test]
    fn parses_setoption() {
        assert_eq!(
            parse_setoption(&words("name Move Overhead value 50")),
            Some(("Move Overhead".into(), "50".into()))
        );
        assert_eq!(
            parse_setoption(&words("name Level value Hard")),
            Some(("Level".into(), "Hard".into()))
        );
        assert_eq!(parse_setoption(&words("value 3")), None);
    }
}
