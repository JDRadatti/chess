#![expect(missing_docs, reason = "doc comments are tracked in TODO.md")]

use game::{Color, Error, Game, Move, Position, Status};
use std::str::FromStr;

// ── Seeded RNG (xorshift64*) ────────────────────────────────────────────────

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed | 1)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

// ── Minimal PGN parser ───────────────────────────────────────────────────────

fn parse_pgn_moves(pgn: &str) -> Vec<String> {
    let mut moves = Vec::new();
    let mut chars = pgn.chars().peekable();

    while let Some(&ch) = chars.peek() {
        match ch {
            '{' => {
                chars.next();
                for c in chars.by_ref() {
                    if c == '}' {
                        break;
                    }
                }
            }
            '[' => {
                // Skip PGN tag pairs like [Event "Opera Game"]
                chars.next();
                for c in chars.by_ref() {
                    if c == ']' {
                        break;
                    }
                }
            }
            '(' => {
                chars.next();
                let mut depth = 1;
                for c in chars.by_ref() {
                    match c {
                        '(' => depth += 1,
                        ')' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                }
            }
            '$' => {
                chars.next();
                while let Some(&c) = chars.peek() {
                    if c.is_ascii_digit() {
                        chars.next();
                    } else {
                        break;
                    }
                }
            }
            c if c.is_whitespace() => {
                chars.next();
            }
            _ => {
                let mut token = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_whitespace() {
                        break;
                    }
                    token.push(c);
                    chars.next();
                }
                if !is_move_number(&token) && !is_result(&token) {
                    moves.push(token);
                }
            }
        }
    }

    moves
}

fn is_move_number(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() {
        return false;
    }
    let mut has_digit = false;
    for c in s.chars() {
        if c.is_ascii_digit() {
            has_digit = true;
        } else if c != '.' {
            return false;
        }
    }
    has_digit
}

fn is_result(s: &str) -> bool {
    matches!(s, "1-0" | "0-1" | "1/2-1/2" | "*")
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn sq_to_alg(sq: u8) -> String {
    let file = sq % 8;
    let rank = sq / 8;
    format!("{}{}", (b'a' + file) as char, (b'1' + (7 - rank)) as char)
}

fn random_move(rng: &mut Rng) -> Move {
    let src = u8::try_from(rng.below(64)).expect("below 64");
    let dst = u8::try_from(rng.below(64)).expect("below 64");
    let s = format!("{}-{}", sq_to_alg(src), sq_to_alg(dst));
    Move::from_str(&s).expect("valid move string")
}

fn all_legal_moves(position: &Position) -> Vec<Move> {
    position.legal_moves()
}

fn apply_move(position: &Position, m: Move) -> Result<Position, Error> {
    let mut next = *position;
    next.apply_move(m)?;
    Ok(next)
}

fn starting_positions() -> Vec<(&'static str, Position)> {
    vec![
        (
            "startpos",
            Position::from_str("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1")
                .expect("valid FEN"),
        ),
        (
            "kiwipete",
            Position::from_str(
                "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
            )
            .expect("valid FEN"),
        ),
        (
            "position3",
            Position::from_str("8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1").expect("valid FEN"),
        ),
        (
            "position4",
            Position::from_str("r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1")
                .expect("valid FEN"),
        ),
    ]
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[test]
fn pgn_roundtrip_famous_games() {
    let pgn_files = [
        "tests/pgn/opera_game.pgn",
        "tests/pgn/evergreen_game.pgn",
        "tests/pgn/kasparov_topalov.pgn",
        "tests/pgn/fischer_byrne.pgn",
        "tests/pgn/immortal_game.pgn",
        "tests/pgn/scholars_mate.pgn",
    ];

    for file in &pgn_files {
        let pgn =
            std::fs::read_to_string(file).unwrap_or_else(|e| panic!("Failed to read {file}: {e}"));
        let moves = parse_pgn_moves(&pgn);

        let mut game = Game::new();

        for san in &moves {
            let m = game
                .position()
                .parse_san(san)
                .unwrap_or_else(|e| panic!("Failed to parse '{san}' in {file}: {e:?}"));

            let formatted = game.position().to_san(&m);
            assert_eq!(&formatted, san, "Round-trip failed for '{san}' in {file}");

            game.play(m)
                .unwrap_or_else(|e| panic!("Failed to apply '{san}' in {file}: {e:?}"));
        }

        if file.ends_with("scholars_mate.pgn") {
            assert_eq!(
                game.status(),
                Status::Checkmate {
                    winner: Color::White
                }
            );
        }
    }
}

#[test]
fn exhaustive_san_roundtrip() {
    for (name, fen) in &starting_positions() {
        let moves = all_legal_moves(fen);
        for m in &moves {
            let san = fen.to_san(m);
            let m2 = fen
                .parse_san(&san)
                .unwrap_or_else(|e| panic!("Failed to parse '{san}' from {name}: {e:?}"));
            assert_eq!(m, &m2, "Round-trip failed for '{san}' from {name}");
        }
    }
}

#[test]
fn invalid_moves_rejected() {
    let mut rng = Rng::new(0xDEAD_BEEF_CAFE_F00D);

    for (_, fen) in &starting_positions() {
        let legal = all_legal_moves(fen);

        for _ in 0..500 {
            let m = random_move(&mut rng);

            if legal.contains(&m) {
                continue;
            }

            let result = apply_move(fen, m);
            assert!(
                result.is_err(),
                "Invalid move {:?} was accepted from {}",
                m,
                fen.to_fen()
            );
        }
    }
}

#[test]
fn invalid_san_strings_rejected() {
    let fen =
        Position::from_str("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1").unwrap();

    let invalid = [
        "Xe4", "Ze4", "Ke9", "Qe9", "Ne9", "Be9", "Re9", "xe4", "e9", "e0", "e", "Q", "N", "B",
        "R", "K", "O-O-O-O", "0-0-0-0", "e4e5", "e4-e5", "e4xe5", "e4x", "x", "", "  ", "e4+",
        "e4#", "e4!!", "e4??", "e4!?", "Nf3x", "Nf3xx", "Nf3x9", "Nf3x0",
    ];

    for san in &invalid {
        assert!(
            fen.parse_san(san).is_err(),
            "Invalid SAN '{san}' was accepted"
        );
    }
}

#[test]
fn fuzz_roundtrip_with_invalid_moves() {
    let mut rng = Rng::new(0x1234_5678_9ABC_DEF0);

    let pgn_files = [
        "tests/pgn/opera_game.pgn",
        "tests/pgn/evergreen_game.pgn",
        "tests/pgn/kasparov_topalov.pgn",
        "tests/pgn/fischer_byrne.pgn",
        "tests/pgn/immortal_game.pgn",
        "tests/pgn/scholars_mate.pgn",
    ];

    for file in &pgn_files {
        let pgn =
            std::fs::read_to_string(file).unwrap_or_else(|e| panic!("Failed to read {file}: {e}"));
        let moves = parse_pgn_moves(&pgn);

        let mut fen =
            Position::from_str("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1")
                .expect("startpos FEN");

        for san in &moves {
            let m = fen
                .parse_san(san)
                .unwrap_or_else(|e| panic!("Failed to parse '{san}' in {file}: {e:?}"));

            let formatted = fen.to_san(&m);
            assert_eq!(&formatted, san, "Round-trip failed for '{san}' in {file}");

            fen = apply_move(&fen, m)
                .unwrap_or_else(|e| panic!("Failed to apply '{san}' in {file}: {e:?}"));

            // Interspersed invalid move attempts
            for _ in 0..5 {
                let bad = random_move(&mut rng);
                let legal = all_legal_moves(&fen);
                if !legal.contains(&bad) {
                    assert!(
                        apply_move(&fen, bad).is_err(),
                        "Invalid move {:?} was accepted at {}",
                        bad,
                        fen.to_fen()
                    );
                }
            }
        }
    }
}

#[test]
fn en_passant_roundtrip() {
    let fen =
        Position::from_str("rnbqkbnr/pppp1ppp/8/4P3/8/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1").unwrap();

    let m = fen.parse_san("d5").unwrap();
    let fen2 = apply_move(&fen, m).unwrap();

    let m2 = fen2.parse_san("exd6").unwrap();
    let san2 = fen2.to_san(&m2);
    assert_eq!(san2, "exd6");
}

#[test]
fn promotion_roundtrip() {
    let fen = Position::from_str("8/P7/8/8/8/8/8/k6K w - - 0 1").unwrap();

    let m = fen.parse_san("a8=Q+").unwrap();
    let san = fen.to_san(&m);
    assert_eq!(san, "a8=Q+");
}

#[test]
fn mate_suffix() {
    // Scholar's mate: after 1.e4 e5 2.Bc4 Nc6 3.Qh5 Nf6, Qxf7 is mate
    let fen =
        Position::from_str("r1bqkb1r/pppp1ppp/2n2n2/4p2Q/2B1P3/8/PPPP1PPP/RNB1K1NR w KQkq - 4 4")
            .unwrap();

    let m = fen.parse_san("Qxf7#").unwrap();
    let san = fen.to_san(&m);
    assert_eq!(san, "Qxf7#");
}

#[test]
fn piece_capture_roundtrip() {
    // White bishop on e2 captures black knight on f3
    let fen =
        Position::from_str("rnbqkbnr/pppppppp/8/8/8/5n2/PPPPBPPP/RNBQK1NR w KQkq - 0 1").unwrap();

    let m = fen.parse_san("Bxf3").unwrap();
    let san = fen.to_san(&m);
    assert_eq!(san, "Bxf3");
}
