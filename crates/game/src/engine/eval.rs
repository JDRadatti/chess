//! Static evaluation: material plus piece-square tables.
//!
//! The tables are Tomasz Michniewski's "Simplified Evaluation Function". They are written from
//! White's side with a8 first, which matches our square numbering, so a white piece on `sq`
//! reads `TABLE[sq]` and a black piece reads the rank-mirrored `TABLE[sq ^ 56]`.

use crate::{
    BISHOP_TYPE, EMPTY, KING_TYPE, KNIGHT_TYPE, PAWN_TYPE, Position, QUEEN_TYPE, ROOK_TYPE,
    SQUARE_CNT, Square, WHITE_MOVE, piece_color, piece_type,
};

/// Material value in centipawns. The king is never traded, so it has none.
pub(crate) const fn piece_value(piece_type: u8) -> i32 {
    match piece_type {
        PAWN_TYPE => 100,
        KNIGHT_TYPE => 320,
        BISHOP_TYPE => 330,
        ROOK_TYPE => 500,
        QUEEN_TYPE => 900,
        _ => 0,
    }
}

/// How much each piece counts toward the middlegame. All pieces on the board add up to
/// [`MAX_PHASE`]; pawns and kings don't count.
const fn phase_weight(piece_type: u8) -> i32 {
    match piece_type {
        KNIGHT_TYPE | BISHOP_TYPE => 1,
        ROOK_TYPE => 2,
        QUEEN_TYPE => 4,
        _ => 0,
    }
}

const MAX_PHASE: i32 = 24;

/// Score of `position` in centipawns, positive when the side to move is better.
pub(crate) fn evaluate(position: &Position) -> i32 {
    let mut score = 0;
    let mut king_middlegame = 0;
    let mut king_endgame = 0;
    let mut phase = 0;

    for sq in 0..SQUARE_CNT as u8 {
        let bits = position.square_bits(Square(sq));
        if bits == EMPTY {
            continue;
        }
        let (index, sign) = if piece_color(bits) == WHITE_MOVE {
            (sq as usize, 1)
        } else {
            ((sq ^ 0b11_1000) as usize, -1)
        };
        let kind = piece_type(bits);
        phase += phase_weight(kind);
        let square_bonus = match kind {
            PAWN_TYPE => PAWN_TABLE[index],
            KNIGHT_TYPE => KNIGHT_TABLE[index],
            BISHOP_TYPE => BISHOP_TABLE[index],
            ROOK_TYPE => ROOK_TABLE[index],
            QUEEN_TYPE => QUEEN_TABLE[index],
            KING_TYPE => {
                king_middlegame += sign * KING_MIDDLEGAME_TABLE[index];
                king_endgame += sign * KING_ENDGAME_TABLE[index];
                0
            }
            _ => 0,
        };
        score += sign * (piece_value(kind) + square_bonus);
    }

    // The king hides behind its pawns while queens are around and walks to the center once
    // the board empties out; blend the two tables by how much material is left.
    let phase = phase.min(MAX_PHASE);
    score += (king_middlegame * phase + king_endgame * (MAX_PHASE - phase)) / MAX_PHASE;

    if position.active_player_bit() == WHITE_MOVE {
        score
    } else {
        -score
    }
}

#[rustfmt::skip]
const PAWN_TABLE: [i32; 64] = [
     0,  0,  0,  0,  0,  0,  0,  0,
    50, 50, 50, 50, 50, 50, 50, 50,
    10, 10, 20, 30, 30, 20, 10, 10,
     5,  5, 10, 25, 25, 10,  5,  5,
     0,  0,  0, 20, 20,  0,  0,  0,
     5, -5,-10,  0,  0,-10, -5,  5,
     5, 10, 10,-20,-20, 10, 10,  5,
     0,  0,  0,  0,  0,  0,  0,  0,
];

#[rustfmt::skip]
const KNIGHT_TABLE: [i32; 64] = [
    -50,-40,-30,-30,-30,-30,-40,-50,
    -40,-20,  0,  0,  0,  0,-20,-40,
    -30,  0, 10, 15, 15, 10,  0,-30,
    -30,  5, 15, 20, 20, 15,  5,-30,
    -30,  0, 15, 20, 20, 15,  0,-30,
    -30,  5, 10, 15, 15, 10,  5,-30,
    -40,-20,  0,  5,  5,  0,-20,-40,
    -50,-40,-30,-30,-30,-30,-40,-50,
];

#[rustfmt::skip]
const BISHOP_TABLE: [i32; 64] = [
    -20,-10,-10,-10,-10,-10,-10,-20,
    -10,  0,  0,  0,  0,  0,  0,-10,
    -10,  0,  5, 10, 10,  5,  0,-10,
    -10,  5,  5, 10, 10,  5,  5,-10,
    -10,  0, 10, 10, 10, 10,  0,-10,
    -10, 10, 10, 10, 10, 10, 10,-10,
    -10,  5,  0,  0,  0,  0,  5,-10,
    -20,-10,-10,-10,-10,-10,-10,-20,
];

#[rustfmt::skip]
const ROOK_TABLE: [i32; 64] = [
     0,  0,  0,  0,  0,  0,  0,  0,
     5, 10, 10, 10, 10, 10, 10,  5,
    -5,  0,  0,  0,  0,  0,  0, -5,
    -5,  0,  0,  0,  0,  0,  0, -5,
    -5,  0,  0,  0,  0,  0,  0, -5,
    -5,  0,  0,  0,  0,  0,  0, -5,
    -5,  0,  0,  0,  0,  0,  0, -5,
     0,  0,  0,  5,  5,  0,  0,  0,
];

#[rustfmt::skip]
const QUEEN_TABLE: [i32; 64] = [
    -20,-10,-10, -5, -5,-10,-10,-20,
    -10,  0,  0,  0,  0,  0,  0,-10,
    -10,  0,  5,  5,  5,  5,  0,-10,
     -5,  0,  5,  5,  5,  5,  0, -5,
      0,  0,  5,  5,  5,  5,  0, -5,
    -10,  5,  5,  5,  5,  5,  0,-10,
    -10,  0,  5,  0,  0,  0,  0,-10,
    -20,-10,-10, -5, -5,-10,-10,-20,
];

#[rustfmt::skip]
const KING_MIDDLEGAME_TABLE: [i32; 64] = [
    -30,-40,-40,-50,-50,-40,-40,-30,
    -30,-40,-40,-50,-50,-40,-40,-30,
    -30,-40,-40,-50,-50,-40,-40,-30,
    -30,-40,-40,-50,-50,-40,-40,-30,
    -20,-30,-30,-40,-40,-30,-30,-20,
    -10,-20,-20,-20,-20,-20,-20,-10,
     20, 20,  0,  0,  0,  0, 20, 20,
     20, 30, 10,  0,  0, 10, 30, 20,
];

#[rustfmt::skip]
const KING_ENDGAME_TABLE: [i32; 64] = [
    -50,-40,-30,-20,-20,-30,-40,-50,
    -30,-20,-10,  0,  0,-10,-20,-30,
    -30,-10, 20, 30, 30, 20,-10,-30,
    -30,-10, 30, 40, 40, 30,-10,-30,
    -30,-10, 30, 40, 40, 30,-10,-30,
    -30,-10, 20, 30, 30, 20,-10,-30,
    -30,-30,  0,  0,  0,  0,-30,-30,
    -50,-30,-30,-30,-30,-30,-30,-50,
];

#[cfg(test)]
mod tests {
    use super::*;

    /// The same position with the board turned around and the colors swapped.
    fn mirrored(fen: &str) -> Position {
        let fields: Vec<&str> = fen.split(' ').collect();
        let swap_case = |s: &str| -> String {
            s.chars()
                .map(|c| {
                    if c.is_ascii_uppercase() {
                        c.to_ascii_lowercase()
                    } else {
                        c.to_ascii_uppercase()
                    }
                })
                .collect()
        };
        let placement: Vec<&str> = fields[0].split('/').rev().collect();
        let side = if fields[1] == "w" { "b" } else { "w" };
        let en_passant = match fields[3].as_bytes() {
            [file, rank] => format!("{}{}", *file as char, (b'1' + b'8' - rank) as char),
            _ => "-".to_string(),
        };
        format!(
            "{} {side} {} {en_passant} {} {}",
            swap_case(&placement.join("/")),
            swap_case(fields[2]),
            fields[4],
            fields[5]
        )
        .parse()
        .unwrap()
    }

    #[test]
    fn start_position_is_level() {
        assert_eq!(evaluate(&Position::new()), 0);
    }

    #[test]
    fn evaluation_is_symmetric() {
        for fen in [
            "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1",
            "r1bqkb1r/pppp1ppp/2n2n2/4p2Q/2B1P3/8/PPPP1PPP/RNB1K1NR w KQkq - 4 4",
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
            "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
            "4k3/8/8/8/8/8/8/4KQ2 b - - 0 1",
        ] {
            let position: Position = fen.parse().unwrap();
            assert_eq!(
                evaluate(&position),
                evaluate(&mirrored(fen)),
                "{fen} vs {}",
                mirrored(fen).to_fen()
            );
        }
    }

    #[test]
    fn extra_material_is_good_for_its_owner() {
        let white_up_a_queen: Position = "4k3/8/8/8/8/8/8/3QK3 w - - 0 1".parse().unwrap();
        assert!(evaluate(&white_up_a_queen) > 800);
        let black_to_move: Position = "4k3/8/8/8/8/8/8/3QK3 b - - 0 1".parse().unwrap();
        assert!(evaluate(&black_to_move) < -800);
    }

    #[test]
    fn developed_knight_beats_rim_knight() {
        let center: Position = "4k3/8/8/8/8/5N2/8/4K3 w - - 0 1".parse().unwrap();
        let rim: Position = "4k3/8/8/8/8/7N/8/4K3 w - - 0 1".parse().unwrap();
        assert!(evaluate(&center) > evaluate(&rim));
    }
}
