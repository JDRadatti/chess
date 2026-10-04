//! The fixed positions every run measures. Changing them makes old results incomparable, so
//! bump [`SUITE_VERSION`] when you do.

pub const SUITE_VERSION: u32 = 1;

/// FEN, depth, and the known leaf count, which doubles as a correctness check.
pub const PERFT_CASES: [(&str, u32, u64); 4] = [
    (
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        4,
        197_281,
    ),
    (
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        3,
        97_862,
    ),
    ("8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1", 5, 674_624),
    (
        "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
        4,
        422_333,
    ),
];

/// Depth of the fixed-depth search over [`SEARCH_POSITIONS`]. Depth 4 takes several seconds
/// per pass today; raise it (and the suite version) once the search gets faster.
pub const SEARCH_DEPTH: u8 = 3;

/// Openings, middlegames, and endgames, mostly from Stockfish's `bench` command.
pub const SEARCH_POSITIONS: [&str; 24] = [
    "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
    "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 10",
    "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 11",
    "4rrk1/pp1n3p/3q2pQ/2p1pb2/2PP4/2P3N1/P2B2PP/4RRK1 b - - 7 19",
    "rq3rk1/ppp2ppp/1bnpb3/3N2B1/3NP3/7P/PPPQ1PP1/2KR3R w - - 7 14",
    "r1bq1r1k/1pp1n1pp/1p1p4/4p2Q/4Pp2/1BNP4/PPP2PPP/3R1RK1 w - - 2 14",
    "r3r1k1/2p2ppp/p1p1bn2/8/1q2P3/2NPQN2/PPP3PP/R4RK1 b - - 2 15",
    "r1bbk1nr/pp3p1p/2n5/1N4p1/2Np1B2/8/PPP2PPP/2KR1B1R w kq - 0 13",
    "r1bq1rk1/ppp1nppp/4n3/3p3Q/3P4/1BP1B3/PP1N2PP/R4RK1 w - - 1 16",
    "4r1k1/r1q2ppp/ppp2n2/4P3/5Rb1/1N1BQ3/PPP3PP/R5K1 w - - 1 17",
    "2rqkb1r/ppp2p2/2npb1p1/1N1Nn2p/2P1PP2/8/PP2B1PP/R1BQK2R b KQ - 0 11",
    "r1bq1r1k/b1p1npp1/p2p3p/1p6/3PP3/1B2NN2/PP3PPP/R2Q1RK1 w - - 1 16",
    "3r1rk1/p5pp/bpp1pp2/8/q1PP1P2/b3P3/P2NQRPP/1R2B1K1 b - - 6 22",
    "r1q2rk1/2p1bppp/2Pp4/p6b/Q1PNp3/4B3/PP1R1PPP/2K4R w - - 2 18",
    "4k2r/1pb2ppp/1p2p3/1R1p4/3P4/2r1PN2/P4PPP/1R4K1 b - - 3 22",
    "3q2k1/pb3p1p/4pbp1/2r5/PpN2N2/1P2P2P/5PP1/Q2R2K1 b - - 4 26",
    "6k1/6p1/6Pp/ppp5/3pn2P/1P3K2/1PP2P2/8 b - - 0 1",
    "8/8/8/8/5kp1/P7/8/1K1N4 w - - 0 1",
    "8/8/8/5N2/8/p7/8/2NK3k w - - 0 1",
    // The Lucena position.
    "1K1k4/1P6/8/8/8/8/r7/2R5 w - - 0 1",
    "8/8/1P6/5pr1/8/4R3/7k/2K5 w - - 0 1",
    "8/2p4P/8/kr6/6R1/8/8/1K6 w - - 0 1",
    "6k1/3b3r/1p1p4/p1n2p2/1PPNpP1q/P3Q1p1/1R1RB1P1/5K2 b - - 0 1",
    "r2r1n2/pp2bk2/2p1p2p/3q4/3PN1QP/2P3R1/P4PP1/5RK1 w - - 0 1",
];

/// Positions for measuring how deep the engine gets in a fixed time: the start, two
/// middlegames, and an endgame.
pub const TIMED_POSITIONS: [&str; 4] = [
    SEARCH_POSITIONS[0],
    SEARCH_POSITIONS[4],
    SEARCH_POSITIONS[9],
    SEARCH_POSITIONS[16],
];

/// Thinking times for the depth-reached measurement, matching the Medium and Hard levels.
pub const TIMED_MILLIS: [u64; 2] = [500, 2000];

#[cfg(test)]
mod tests {
    use super::*;
    use game::{Position, Status};

    #[test]
    fn positions_are_valid_and_playable() {
        for fen in SEARCH_POSITIONS
            .iter()
            .chain(PERFT_CASES.iter().map(|c| &c.0))
        {
            let position: Position = fen.parse().unwrap_or_else(|e| panic!("{fen}: {e:?}"));
            assert_eq!(position.status(), Status::Ongoing, "{fen}");
            assert!(position.legal_moves().len() > 1, "{fen}");
        }
    }
}
