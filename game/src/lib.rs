mod attacks;

use attacks::*;

use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

const SQUARE_CNT: usize = 64;
const MAX_HALFMOVE_CNT: u8 = 75;

// Piece type extraction from 4-bit piece encoding
const fn piece_type(piece_bits: u8) -> u8 {
    piece_bits & 0b0111
}

const fn piece_color(piece_bits: u8) -> u8 {
    piece_bits >> 3
}

// Piece type IDs (for piece_type() extraction and move encoding)

const PAWN_TYPE: u8 = 1;
const ROOK_TYPE: u8 = 2;
const BISHOP_TYPE: u8 = 3;
const KNIGHT_TYPE: u8 = 4;
const KING_TYPE: u8 = 5;
const QUEEN_TYPE: u8 = 7;

// Pieces (color bit 3 + type bits 2-0)

const EMPTY: u8 = 0b0000;

const WHITE_PAWN: u8 = 0b0001;
const WHITE_ROOK: u8 = 0b0010;
const WHITE_BISHOP: u8 = 0b0011;
const WHITE_KNIGHT: u8 = 0b0100;
const WHITE_KING: u8 = 0b0101;
const WHITE_QUEEN: u8 = 0b0111;

const BLACK_PAWN: u8 = 0b1001;
const BLACK_ROOK: u8 = 0b1010;
const BLACK_BISHOP: u8 = 0b1011;
const BLACK_KNIGHT: u8 = 0b1100;
const BLACK_KING: u8 = 0b1101;
const BLACK_QUEEN: u8 = 0b1111;

// Castling (4-bit mask)

const WHITE_KINGSIDE_CASTLE: u8 = 0b0001;
const WHITE_QUEENSIDE_CASTLE: u8 = 0b0010;
const BLACK_KINGSIDE_CASTLE: u8 = 0b0100;
const BLACK_QUEENSIDE_CASTLE: u8 = 0b1000;
const NO_CASTLE: u8 = 0b0000;

// Active player

const BLACK_MOVE: u8 = 1;
const WHITE_MOVE: u8 = 0;

// En passant

const NO_EN_PASSANT: u8 = 0;
const HAS_EN_PASSANT: u8 = 1;

// Files

const FILE_A: u8 = 0b001;
const FILE_B: u8 = 0b010;
const FILE_C: u8 = 0b011;
const FILE_D: u8 = 0b100;
const FILE_E: u8 = 0b101;
const FILE_F: u8 = 0b111;

// Error

pub enum Error {
    InvalidFenString(String),
    InvalidMove,
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Error::InvalidFenString(fen_string) => format!("invalid fen string: {fen_string}"),
            Error::InvalidMove => format!("invalid move"),
        }
        .fmt(f)
    }
}

/// Fen representation of a chess board state.
///
/// Consists of six space-separated fields:
/// 1. Piece placement. Slash separated list of ranks, tarting with rank 8. Pieces are represented
///    as p=pawn, r=rook, q=queen, k=king, n=knight, b=bishop. Capital letters represent white pieces, and
///    lowercase represent black pieces. (i.e.
/// 2. Active color. b (1) for black, w (0) for white.
/// 3. Castling rights. K for white kingside, Q for white queenside, k for black kingside, q for
///    black queenside. Hyphen if neither side can castle.
/// 4. En Passant target square. The square "behind" a pawn that just moved two squares. Hyphen if
///    no valid en passant square exists.
/// 5. Halfmoves. Number of moves since the last pawn advance or piece capture. Used to enforce 50
///    move rule (after 50 moves, either side can declare a draw, after 75 moves it is automatically
///    a draw.)
/// 6. Fullmoves. Starts at 1 and increments every black move.
///
/// # Representation
///
/// Fen[0..=31] = board squares.
///     Each square takes up 4 bits (1 for piece color, 3 for piece type, 0000 = empty)
/// Fen[32..=34] =
///     Byte 32:  [castling:4][has_en_passant:1][en_passant_file:3]
///     Byte 33:  [active_player:1][halfmove_cnt:7]
///     Byte 34:  [fullmove_cnt:8]
#[derive(Debug)]
pub struct Fen([u8; 35]);

impl Fen {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn try_move(&mut self, m: Move) -> Result<(), Error> {
        let src = Square(m.src());
        let dst = Square(m.dst());
        let piece = m.dst() & 0b0111;
        for candidate_move in self.all_valid_moves_from_square(src) {
            if m == candidate_move {
                // Execute move
                self.set_square(src, EMPTY);
                self.set_square(dst, piece);
                // what if a castle 
                // what if this move removes a castle
                // what if this move is a promotion
                // what if this piece is an en passant (nned ot change differnet square )
                // need to update has_en_passant
                // need to update active player
                // need to update halfmove/fullmove_cnt
                return Ok(());
            }
        }
        Err(Error::InvalidMove)
    }

    fn all_valid_moves(&self) -> Vec<Move> {
        let mut moves = vec![];
        for i in 0..SQUARE_CNT {
            moves.extend(self.all_valid_moves_from_square(Square(i as u8)));
        }
        moves
    }

    fn set_square(&mut self, s: Square, p: u8) {
        let square_idx = (s.0 >> 1) as usize;
        assert!(square_idx <= 31, "invalid square");
        if s.0 % 2 == 0 {
            self.0[square_idx] = (self.0[square_idx] & 0b11110000) | p;
        } else {
            self.0[square_idx] = (self.0[square_idx] & 0b00001111) | p << 4;
        }
    }

    fn square_bits(&self, s: Square) -> u8 {
        // Even squares are the low bits (0b1111), odd squares are the high bits(0b11110000).
        let square_idx = (s.0 >> 1) as usize;
        assert!(square_idx <= 31, "invalid square");
        if s.0 % 2 == 0 {
            self.0[square_idx] & 0b1111
        } else {
            self.0[square_idx] >> 4
        }
    }

    fn en_passant_bits(&self) -> u8 {
        self.0[32] & 0b1111
    }

    fn castle_bits(&self) -> u8 {
        (self.0[32] & 0b11110000) >> 4
    }

    fn active_player_bit(&self) -> u8 {
        self.0[33] >> 7
    }

    fn halfmove_cnt(&self) -> u8 {
        self.0[33] & 0b01111111
    }

    fn fullmove_cnt(&self) -> u8 {
        self.0[34]
    }

    fn inc_halfmove_cnt(&mut self) {
        // NOTE: if halfmove_cnt == 127, self.0[33] = 0b111111111 OR 0b011111111.
        // In this cases, adding one will either panic or change active player and reset
        // halfmove_cnt to 0. Both cases are not good. This should never happen because game ends in
        // draw when halfmove_cnt == 75.
        // assert!((self.0[33] & 0b011111111) < 128, "halfmove overflow.");
        self.0[33] += 1
    }

    fn inc_fullmove_cnt(&mut self) {
        self.0[34] += 1
    }

    /// Bitboard of all occupied squares (both colors).
    fn occupancy(&self) -> u64 {
        let mut occ = 0u64;
        let mut sq = 0u8;
        while sq < 64 {
            if self.square_bits(Square(sq)) != 0 {
                occ |= 1u64 << sq;
            }
            sq += 1;
        }
        occ
    }

    /// Bitboard of all squares occupied by `color` (0=white, 1=black).
    fn occupancy_by_player(&self, color: u8) -> u64 {
        let mut occ = 0u64;
        let mut sq = 0u8;
        while sq < 64 {
            let bits = self.square_bits(Square(sq));
            if bits != 0 && (bits >> 3) == color {
                occ |= 1u64 << sq;
            }
            sq += 1;
        }
        occ
    }

    /// Is `sq` attacked by `color` (0=white, 1=black)?
    fn is_attacked_by(&self, sq: u8, color: u8) -> bool {
        let occ = self.occupancy();

        // Knight
        for src in bitboard_iter(KNIGHT_ATTACKS[sq as usize]) {
            let bits = self.square_bits(Square(src));
            if bits != 0 && (bits >> 3) == color && piece_type(bits) == KNIGHT_TYPE {
                return true;
            }
        }
        // King
        for src in bitboard_iter(KING_ATTACKS[sq as usize]) {
            let bits = self.square_bits(Square(src));
            if bits != 0 && (bits >> 3) == color && piece_type(bits) == KING_TYPE {
                return true;
            }
        }
        // Pawn — check squares diagonally adjacent from which a pawn of `color` attacks `sq`
        let pawn_dr: i8 = if color == 0 { 1 } else { -1 };
        let r = sq_rank(sq);
        let f = sq_file(sq);
        let pr = r + pawn_dr;
        if pr >= 0 && pr < 8 {
            let pf_left = f - 1;
            let pf_right = f + 1;
            if pf_left >= 0 {
                let src = rank_file_to_sq(pr, pf_left);
                let bits = self.square_bits(Square(src));
                if bits != 0 && (bits >> 3) == color && piece_type(bits) == PAWN_TYPE {
                    return true;
                }
            }
            if pf_right < 8 {
                let src = rank_file_to_sq(pr, pf_right);
                let bits = self.square_bits(Square(src));
                if bits != 0 && (bits >> 3) == color && piece_type(bits) == PAWN_TYPE {
                    return true;
                }
            }
        }
        // Bishop / Queen on diagonals
        for src in bitboard_iter(sliding_attacks(sq, &BISHOP_RAYS, occ)) {
            let bits = self.square_bits(Square(src));
            if bits != 0 && (bits >> 3) == color {
                let pt = piece_type(bits);
                if pt == BISHOP_TYPE || pt == QUEEN_TYPE {
                    return true;
                }
            }
        }
        // Rook / Queen on straights
        for src in bitboard_iter(sliding_attacks(sq, &ROOK_RAYS, occ)) {
            let bits = self.square_bits(Square(src));
            if bits != 0 && (bits >> 3) == color {
                let pt = piece_type(bits);
                if pt == ROOK_TYPE || pt == QUEEN_TYPE {
                    return true;
                }
            }
        }

        false
    }

    // NOTE: will probably need to optimize this. 
    // Max number of moves is 218
    fn all_valid_moves_from_square(&self, s: Square) -> Vec<Move> {
        let mut moves = Vec::with_capacity(8);
        let active_player = self.active_player_bit();
        let square_bits = self.square_bits(s);

        if square_bits == 0 {
            return moves;
        }

        if active_player != (square_bits >> 3) {
            return moves;
        }

        let src = s.0;
        let pt = piece_type(square_bits);
        let occ = self.occupancy();
        let friendly = self.occupancy_by_player(active_player);
        let opponent = self.occupancy_by_player(active_player ^ 1);

        match pt {
            KNIGHT_TYPE => {
                let attacks = KNIGHT_ATTACKS[src as usize] & !friendly;
                for dst in bitboard_iter(attacks) {
                    moves.push(Move::new(0, src, dst));
                }
            }
            KING_TYPE => {
                let attacks = KING_ATTACKS[src as usize] & !friendly;
                for dst in bitboard_iter(attacks) {
                    moves.push(Move::new(0, src, dst));
                }
                // Castling
                moves.extend(self.castling_moves(active_player));
            }
            PAWN_TYPE => {
                let (pushes, captures, _) = if active_player == 0 {
                    (
                        PAWN_PUSHES_WHITE[src as usize],
                        PAWN_ATTACKS_WHITE[src as usize],
                        6u8, // white pawns on rank 6 can double-push
                    )
                } else {
                    (
                        flip_rank_bb(PAWN_PUSHES_WHITE[flip_rank_sq(src) as usize]),
                        flip_rank_bb(PAWN_ATTACKS_WHITE[flip_rank_sq(src) as usize]),
                        1u8, // black pawns on rank 1 can double-push
                    )
                };

                // Forward pushes (only if destination is empty)
                for dst in bitboard_iter(pushes & !occ) {
                    if is_promotion_rank(dst, active_player) {
                        // Generate promotion moves
                        for promo in [QUEEN_TYPE, ROOK_TYPE, BISHOP_TYPE, KNIGHT_TYPE] {
                            moves.push(Move::new(promo, src, dst));
                        }
                    } else {
                        moves.push(Move::new(0, src, dst));
                    }
                }

                // En passant capture
                if let Some(ep_sq) = self.en_passant_target(active_player) {
                    if PAWN_ATTACKS_WHITE[if active_player == 0 {
                        src
                    } else {
                        flip_rank_sq(src)
                    } as usize]
                        & (1u64 << ep_sq)
                        != 0
                    {
                        moves.push(Move::new(0, src, ep_sq));
                    }
                }

                // Diagonal captures (only onto opponent pieces)
                for dst in bitboard_iter(captures & opponent) {
                    if is_promotion_rank(dst, active_player) {
                        for promo in [QUEEN_TYPE, ROOK_TYPE, BISHOP_TYPE, KNIGHT_TYPE] {
                            moves.push(Move::new(promo, src, dst));
                        }
                    } else {
                        moves.push(Move::new(0, src, dst));
                    }
                }
            }
            BISHOP_TYPE => {
                let attacks = sliding_attacks(src, &BISHOP_RAYS, occ) & !friendly;
                for dst in bitboard_iter(attacks) {
                    moves.push(Move::new(0, src, dst));
                }
            }
            ROOK_TYPE => {
                let attacks = sliding_attacks(src, &ROOK_RAYS, occ) & !friendly;
                for dst in bitboard_iter(attacks) {
                    moves.push(Move::new(0, src, dst));
                }
            }
            QUEEN_TYPE => {
                let attacks = sliding_attacks(src, &QUEEN_RAYS, occ) & !friendly;
                for dst in bitboard_iter(attacks) {
                    moves.push(Move::new(0, src, dst));
                }
            }
            _ => {}
        }

        moves
    }

    fn en_passant_target(&self, active_player: u8) -> Option<u8> {
        let ep_bits = self.en_passant_bits();
        let has_ep = (ep_bits >> 3) & 1;
        if has_ep == 0 {
            return None;
        }
        let file = (ep_bits & 0b0111) as i8;
        let rank = if active_player == 0 { 2i8 } else { 5i8 }; // target square rank
        Some(rank_file_to_sq(rank, file))
    }

    fn castling_moves(&self, active_player: u8) -> Vec<Move> {
        let mut moves = Vec::with_capacity(2);
        let castle = self.castle_bits();

        if active_player == 0 {
            // White: king on e1 (square 60)
            let king_sq = 60u8;
            let occ = self.occupancy();
            // Kingside (K): king e1->g1, squares f1/g1 must be empty, king not in check
            if castle & WHITE_KINGSIDE_CASTLE != 0 {
                let between = (1u64 << 61) | (1u64 << 62); // f1, g1
                if occ & between == 0
                    && !self.is_attacked_by(king_sq, 1)
                    && !self.is_attacked_by(61, 1) // f1
                    && !self.is_attacked_by(62, 1)
                // g1
                {
                    moves.push(Move::new(0, king_sq, 62));
                }
            }
            // Queenside (Q): king e1->c1, squares b1/c1/d1 must be empty
            if castle & WHITE_QUEENSIDE_CASTLE != 0 {
                let between = (1u64 << 57) | (1u64 << 58) | (1u64 << 59); // b1, c1, d1
                if occ & between == 0
                    && !self.is_attacked_by(king_sq, 1)
                    && !self.is_attacked_by(59, 1) // d1
                    && !self.is_attacked_by(58, 1)
                // c1
                {
                    moves.push(Move::new(0, king_sq, 58));
                }
            }
        } else {
            // Black: king on e8 (square 4)
            let king_sq = 4u8;
            let occ = self.occupancy();
            // Kingside (k): king e8->g8
            if castle & BLACK_KINGSIDE_CASTLE != 0 {
                let between = (1u64 << 5) | (1u64 << 6); // f8, g8
                if occ & between == 0
                    && !self.is_attacked_by(king_sq, 0)
                    && !self.is_attacked_by(5, 0) // f8
                    && !self.is_attacked_by(6, 0)
                // g8
                {
                    moves.push(Move::new(0, king_sq, 6));
                }
            }
            // Queenside (q): king e8->c8
            if castle & BLACK_QUEENSIDE_CASTLE != 0 {
                let between = (1u64 << 1) | (1u64 << 2) | (1u64 << 3); // b8, c8, d8
                if occ & between == 0
                    && !self.is_attacked_by(king_sq, 0)
                    && !self.is_attacked_by(3, 0)
                    && !self.is_attacked_by(2, 0)
                {
                    moves.push(Move::new(0, king_sq, 2));
                }
            }
        }

        moves
    }
}

fn is_promotion_rank(sq: u8, color: u8) -> bool {
    let rank = sq_rank(sq);
    (color == 0 && rank == 0) || (color == 1 && rank == 7)
}

/// Starting FEN: rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1
impl Default for Fen {
    fn default() -> Self {
        Self([
            // Rank 8: rnbqkbnr
            BLACK_KNIGHT << 4 | BLACK_ROOK,
            BLACK_QUEEN << 4 | BLACK_BISHOP,
            BLACK_BISHOP << 4 | BLACK_KING,
            BLACK_ROOK << 4 | BLACK_KNIGHT,
            // Rank 7: pppppppp
            BLACK_PAWN << 4 | BLACK_PAWN,
            BLACK_PAWN << 4 | BLACK_PAWN,
            BLACK_PAWN << 4 | BLACK_PAWN,
            BLACK_PAWN << 4 | BLACK_PAWN,
            // Rank 6: 8
            EMPTY,
            EMPTY,
            EMPTY,
            EMPTY,
            // Rank 5: 8
            EMPTY,
            EMPTY,
            EMPTY,
            EMPTY,
            // Rank 4: 8
            EMPTY,
            EMPTY,
            EMPTY,
            EMPTY,
            // Rank 3: 8
            EMPTY,
            EMPTY,
            EMPTY,
            EMPTY,
            // Rank 2: PPPPPPPP
            WHITE_PAWN << 4 | WHITE_PAWN,
            WHITE_PAWN << 4 | WHITE_PAWN,
            WHITE_PAWN << 4 | WHITE_PAWN,
            WHITE_PAWN << 4 | WHITE_PAWN,
            // Rank 1: RNBQKBNR
            WHITE_KNIGHT << 4 | WHITE_ROOK,
            WHITE_QUEEN << 4 | WHITE_BISHOP,
            WHITE_BISHOP << 4 | WHITE_KING,
            WHITE_ROOK << 4 | WHITE_KNIGHT,
            // Implicit has_en_passant and en_passant_file (0b0000)
            (WHITE_KINGSIDE_CASTLE
                | WHITE_QUEENSIDE_CASTLE
                | BLACK_KINGSIDE_CASTLE
                | BLACK_QUEENSIDE_CASTLE)
                << 4,
            // Implicit halfmove_clock
            WHITE_MOVE << 7,
            1, // fullmove_cnt
        ])
    }
}

//
/// # Pieces
///
/// K, Q, B, N, R (no letter for pawn)
///
/// # Squares
///
/// a1..a8, b1..b8, c1..c8, ..., h1..h8
///
/// # Piece Move without capture
///
/// Piece | destination square
/// - e4 - pawn moves to e4 square
/// - Bc4 - bishop moves to c4
///
/// # Captures
///
/// Piece | x | destination square
///
/// # Pawn Captures
///
/// File pawn is leaving | x | destination square
///
/// # Checks + Checkmates
///
/// append + for check
/// append # for checkmate
///
/// # Castling
///
/// 0-0 for King side castle
/// 0-0-0 for Queen side castle
///
/// # Pawn promotion
///
/// Dest square | = | promoted piece
/// - e8=Q
///
/// # Disambiguation
///
/// Iff two pieces can move to the same square LEGALLY, add the distinct rank/file after the piece.
/// - mainly used for knights and rooks
/// - i.e. a knight on g1 and a knight on e5 can both move to f3, but you move the one on g: Ngf3
/// - i.e. a knight on g1 and a knight on g5 can both move to f3, but you move the one on 1: N1f3
/// - a pinned piece does not count as it is illegal to move.
///
/// # Requirements
///
/// - must exist the correct piece on the start square.
/// - if x, must exist a piece on the dest square.
/// - if not x, must not exist a piece on the dest square.
/// - pawn capture: dest file must be +/-1 from start file.
/// - must be a valid move based on piece moved.
/// - must not create a check or checkmate for opponent.
/// - must correctly identify checks + checkmates.
///
/// # Representation
///
/// Move[0..=3] = Promotion piece. (0000 for no promotion)
/// Fen[4..=10] = src square.
/// Fen[11..=16] = dst square.
///
/// Total = 16 bits.
///
/// Technically, only 1,968 possible moves, which could take up 11 bits.
///
#[derive(Clone, Copy, PartialEq)]
pub struct Move(u16);

impl Move {
    /// Create a new move.
    /// `promo_type`: piece type for promotion (0 = none), values match piece type encoding.
    /// `src`: source square index (0-63).
    /// `dst`: destination square index (0-63).
    const fn new(promo_type: u8, src: u8, dst: u8) -> Self {
        Self((promo_type as u16) | (src as u16) << 4 | (dst as u16) << 10)
    }

    const fn src(self) -> u8 {
        ((self.0 >> 4) & 0b00111111) as u8
    }

    const fn dst(self) -> u8 {
        ((self.0 >> 10) & 0b00111111) as u8
    }

    const fn promo(self) -> u8 {
        (self.0 & 0b00001111) as u8
    }
}

fn parse_square(file_ch: u8, rank_ch: u8) -> Result<u8, &'static str> {
    if file_ch < b'a' || file_ch > b'h' {
        return Err("file must be a-h");
    }
    if rank_ch < b'1' || rank_ch > b'8' {
        return Err("rank must be 1-8");
    }
    let file = file_ch - b'a';
    let rank = rank_ch - b'1';
    Ok((7 - rank) * 8 + file)
}

fn parse_promo(ch: u8) -> Result<u8, &'static str> {
    match ch {
        b'q' => Ok(QUEEN_TYPE),
        b'r' => Ok(ROOK_TYPE),
        b'b' => Ok(BISHOP_TYPE),
        b'n' => Ok(KNIGHT_TYPE),
        _ => Err("promotion piece must be q, r, b, or n"),
    }
}

impl FromStr for Move {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = s.as_bytes();

        // "src-dst" = 5 chars, "src-dst-promo" = 7 chars
        if bytes.len() != 5 && bytes.len() != 7 {
            return Err("expected format: src-dst (e.g. e2e4) or src-dst-promo (e.g. e7e8q)");
        }
        if bytes[2] != b'-' {
            return Err("expected '-' between src and dst");
        }
        if bytes.len() == 7 && bytes[5] != b'-' {
            return Err("expected '-' before promotion piece");
        }

        let src = parse_square(bytes[0], bytes[1])?;
        let dst = parse_square(bytes[3], bytes[4])?;

        let promo = if bytes.len() == 7 {
            parse_promo(bytes[6])?
        } else {
            0
        };

        Ok(Move::new(promo, src, dst))
    }
}

/// Represents a square on an 8x8 board.
///
/// (a8, 0), b8, c8, d8, e8, f8, g8, (h8, 7)
/// ...
/// (a1, 56), b1, c1, d1, e1, f1, g1, (h1, 63)
///
/// Square must be less than 64.
/// Square >> 1 = Index into Fen.
#[derive(Clone, Copy)]
struct Square(u8);

//
// TESTING
//

fn print_memory_representation<T: std::fmt::Debug>(t: T) {
    print!("type={} value={t:?}: ", std::any::type_name::<T>());
    let start = &t as *const _ as *const u8;
    for i in 0..std::mem::size_of::<T>() {
        print!("{:02x} ", unsafe { *start.offset(i as isize) });
    }
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_size() {
        let default = Fen::new();
        dbg!("{:?}", print_memory_representation(default));
        assert_eq!(std::mem::size_of::<Fen>(), 35);
        assert_eq!(std::mem::size_of::<Move>(), 2);
    }

    #[test]
    fn knight_attack_e4() {
        // e4 = rank 4, file 4 = sq 36
        let attacks = KNIGHT_ATTACKS[36];
        // Knight on e4 attacks: d6(19), f6(21), c5(26), g5(30), c3(42), g3(46), d2(51), f2(53)
        let expected_squares = [19, 21, 26, 30, 42, 46, 51, 53];
        for sq in &expected_squares {
            assert!(attacks & (1u64 << sq) != 0, "missing attack on sq {sq}");
        }
        assert_eq!(attacks.count_ones(), 8);
    }

    #[test]
    fn knight_attack_corner() {
        // a8 = sq 0, knight attacks: c7(10), b6(17)
        let attacks = KNIGHT_ATTACKS[0];
        assert!(attacks & (1u64 << 10) != 0); // c7
        assert!(attacks & (1u64 << 17) != 0); // b6
        assert_eq!(attacks.count_ones(), 2);
    }

    #[test]
    fn king_attack_e4() {
        // King on e4 = sq 36
        let attacks = KING_ATTACKS[36];
        assert_eq!(attacks.count_ones(), 8);
        // d3=43, e3=44, f3=45, d4=35, f4=37, d5=27, e5=28, f5=29
        for sq in [27, 28, 29, 35, 37, 43, 44, 45] {
            assert!(attacks & (1u64 << sq) != 0, "missing attack on sq {sq}");
        }
    }

    #[test]
    fn pawn_pushes_white_e2() {
        // e2 = rank 6, file 4 => sq 52
        let pushes = PAWN_PUSHES_WHITE[52];
        // Should push to e3(44) and e4(36)
        assert!(pushes & (1u64 << 44) != 0, "missing single push to e3");
        assert!(pushes & (1u64 << 36) != 0, "missing double push to e4");
        assert_eq!(pushes.count_ones(), 2);
    }

    #[test]
    fn pawn_pushes_white_e3() {
        // e3 = rank 5, file 4 => sq 44
        let pushes = PAWN_PUSHES_WHITE[44];
        // Only single push to e4(36)
        assert!(pushes & (1u64 << 36) != 0);
        assert_eq!(pushes.count_ones(), 1);
    }

    #[test]
    fn pawn_attacks_white() {
        // e4 = sq 36
        let attacks = PAWN_ATTACKS_WHITE[36];
        // d5(27), f5(29)
        assert!(attacks & (1u64 << 27) != 0);
        assert!(attacks & (1u64 << 29) != 0);
        assert_eq!(attacks.count_ones(), 2);
    }

    #[test]
    fn pawn_attacks_white_a_file() {
        // a4 = sq 32, no left capture
        let attacks = PAWN_ATTACKS_WHITE[32];
        assert_eq!(attacks.count_ones(), 1); // only b5
    }

    #[test]
    fn bishop_rays_e4() {
        let rays = BISHOP_RAYS[36];
        // Diagonals from e4: d5,c6,b7,a8 | f5,g6,h7 | d3,c2,b1 | f3,g2,h1
        assert!(rays & (1u64 << 27) != 0); // d5
        assert!(rays & (1u64 << 18) != 0); // c6
        assert!(rays & (1u64 << 9) != 0); // b7
        assert!(rays & (1u64 << 0) != 0); // a8
        assert!(rays & (1u64 << 29) != 0); // f5
        assert!(rays & (1u64 << 22) != 0); // g6
        assert!(rays & (1u64 << 15) != 0); // h7
        assert!(rays & (1u64 << 43) != 0); // d3
        assert!(rays & (1u64 << 50) != 0); // c2
        assert!(rays & (1u64 << 57) != 0); // b1
        assert!(rays & (1u64 << 45) != 0); // f3
        assert!(rays & (1u64 << 54) != 0); // g2
        assert!(rays & (1u64 << 63) != 0); // h1
    }

    #[test]
    fn rook_rays_e4() {
        let rays = ROOK_RAYS[36];
        // e-file: e5(28), e6(20), e7(12), e8(4), e3(44), e2(52), e1(60)
        // 4th rank: d4(35), c4(34), b4(33), a4(32), f4(37), g4(38), h4(39)
        assert_eq!(rays.count_ones(), 14);
        assert!(rays & (1u64 << 28) != 0); // e5
        assert!(rays & (1u64 << 60) != 0); // e1
        assert!(rays & (1u64 << 35) != 0); // d4
        assert!(rays & (1u64 << 39) != 0); // h4
    }

    #[test]
    fn queen_rays_e4() {
        let rays = QUEEN_RAYS[36];
        assert_eq!(rays.count_ones(), 27); // 13 bishop + 14 rook
    }

    #[test]
    fn flip_rank() {
        assert_eq!(flip_rank_sq(0), 56); // a8 <-> a1
        assert_eq!(flip_rank_sq(7), 63); // h8 <-> h1
        assert_eq!(flip_rank_sq(56), 0); // a1 <-> a8
        assert_eq!(flip_rank_sq(36), 28); // e4 <-> e5
    }

    #[test]
    fn starting_position_20_moves() {
        let fen = Fen::default();
        let moves = fen.all_valid_moves();
        assert_eq!(
            moves.len(),
            20,
            "starting position should have 20 legal moves"
        );
    }

    #[test]
    fn starting_position_pawn_moves() {
        let fen = Fen::default();
        // White to move: 16 pawn pushes + 4 knight moves = 20
        let pawn_moves: Vec<_> = (0..64)
            .flat_map(|sq| fen.all_valid_moves_from_square(Square(sq)))
            .filter(|m| {
                let bits = fen.square_bits(Square(m.src()));
                piece_type(bits) == PAWN_TYPE
            })
            .collect();
        assert_eq!(pawn_moves.len(), 16, "should have 16 pawn pushes");
    }

    #[test]
    fn starting_position_knight_moves() {
        let fen = Fen::default();
        let knight_moves: Vec<_> = (0..64)
            .flat_map(|sq| fen.all_valid_moves_from_square(Square(sq)))
            .filter(|m| {
                let bits = fen.square_bits(Square(m.src()));
                piece_type(bits) == KNIGHT_TYPE
            })
            .collect();
        assert_eq!(
            knight_moves.len(),
            4,
            "should have 4 knight moves (2 per knight)"
        );
    }

    #[test]
    fn move_encoding() {
        let m = Move::new(0, 36, 28);
        assert_eq!(m.src(), 36);
        assert_eq!(m.dst(), 28);
        assert_eq!(m.promo(), 0);

        let m2 = Move::new(QUEEN_TYPE, 52, 36);
        assert_eq!(m2.src(), 52);
        assert_eq!(m2.dst(), 36);
        assert_eq!(m2.promo(), QUEEN_TYPE);
    }

    #[test]
    fn move_parse_simple() {
        let m: Move = "e2-e4".parse().unwrap();
        assert_eq!(m.src(), 52); // e2
        assert_eq!(m.dst(), 36); // e4
        assert_eq!(m.promo(), 0);
    }

    #[test]
    fn move_parse_with_promo() {
        let m: Move = "e7-e8-q".parse().unwrap();
        assert_eq!(m.src(), 12); // e7
        assert_eq!(m.dst(), 4);  // e8
        assert_eq!(m.promo(), QUEEN_TYPE);

        let m2: Move = "a7-a8-n".parse().unwrap();
        assert_eq!(m2.src(), 8);  // a7
        assert_eq!(m2.dst(), 0);  // a8
        assert_eq!(m2.promo(), KNIGHT_TYPE);
    }

    #[test]
    fn move_parse_corners() {
        let m: Move = "a8-h1".parse().unwrap();
        assert_eq!(m.src(), 0);  // a8
        assert_eq!(m.dst(), 63); // h1

        let m2: Move = "h1-a8".parse().unwrap();
        assert_eq!(m2.src(), 63); // h1
        assert_eq!(m2.dst(), 0);  // a8
    }

    #[test]
    fn move_parse_errors() {
        assert!("e2e4".parse::<Move>().is_err());       // missing dashes
        assert!("e2e4e".parse::<Move>().is_err());       // too long
        assert!("e2".parse::<Move>().is_err());           // too short
        assert!("i2-e4".parse::<Move>().is_err());        // invalid file
        assert!("e9-e4".parse::<Move>().is_err());        // invalid rank
        assert!("e7-e8-x".parse::<Move>().is_err());      // invalid promo
    }

}
