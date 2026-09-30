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

#[derive(Debug)]
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
#[derive(Debug, Clone, Copy)]
pub struct Fen([u8; 35]);

impl Fen {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn try_move(&mut self, m: Move) -> Result<(), Error> {
        let src = m.src();
        let dst = m.dst();

        for candidate in self.all_valid_moves_from_square(Square(src)) {
            if m == candidate {
                self.apply_move_unchecked(m);
                return Ok(());
            }
        }
        Err(Error::InvalidMove)
    }

    pub fn to_fen(&self) -> String {
        let mut result = String::new();

        // Piece placement
        for rank in 0..8 {
            let mut empty_count = 0;
            for file in 0..8 {
                let sq = rank * 8 + file;
                let bits = self.square_bits(Square(sq));
                if bits == 0 {
                    empty_count += 1;
                } else {
                    if empty_count > 0 {
                        result.push_str(&empty_count.to_string());
                        empty_count = 0;
                    }
                    let ch = match bits {
                        WHITE_PAWN => 'P',
                        WHITE_ROOK => 'R',
                        WHITE_KNIGHT => 'N',
                        WHITE_BISHOP => 'B',
                        WHITE_QUEEN => 'Q',
                        WHITE_KING => 'K',
                        BLACK_PAWN => 'p',
                        BLACK_ROOK => 'r',
                        BLACK_KNIGHT => 'n',
                        BLACK_BISHOP => 'b',
                        BLACK_QUEEN => 'q',
                        BLACK_KING => 'k',
                        _ => '?',
                    };
                    result.push(ch);
                }
            }
            if empty_count > 0 {
                result.push_str(&empty_count.to_string());
            }
            if rank < 7 {
                result.push('/');
            }
        }
        result.push(' ');

        // Active color
        if self.active_player_bit() == WHITE_MOVE {
            result.push('w');
        } else {
            result.push('b');
        }
        result.push(' ');

        // Castling
        let castle = self.castle_bits();
        if castle == 0 {
            result.push('-');
        } else {
            if castle & WHITE_KINGSIDE_CASTLE != 0 {
                result.push('K');
            }
            if castle & WHITE_QUEENSIDE_CASTLE != 0 {
                result.push('Q');
            }
            if castle & BLACK_KINGSIDE_CASTLE != 0 {
                result.push('k');
            }
            if castle & BLACK_QUEENSIDE_CASTLE != 0 {
                result.push('q');
            }
        }
        result.push(' ');

        // En passant
        let ep_bits = self.en_passant_bits();
        let has_ep = (ep_bits >> 3) & 1;
        if has_ep == 0 {
            result.push('-');
        } else {
            let file = ep_bits & 0b0111;
            let file_ch = (b'a' + file) as char;
            let rank = if self.active_player_bit() == WHITE_MOVE {
                '6'
            } else {
                '3'
            };
            result.push(file_ch);
            result.push(rank);
        }
        result.push(' ');

        // Halfmove and fullmove
        result.push_str(&self.halfmove_cnt().to_string());
        result.push(' ');
        result.push_str(&self.fullmove_cnt().to_string());

        result
    }

    pub fn parse_san(&self, san: &str) -> Result<Move, Error> {
        let san = san.trim();

        // Strip check/mate suffix
        let san = san.trim_end_matches('+').trim_end_matches('#');

        // Castling
        if san == "O-O" || san == "0-0" {
            let king_sq = if self.active_player_bit() == WHITE_MOVE {
                60
            } else {
                4
            };
            let king_dst = if self.active_player_bit() == WHITE_MOVE {
                62
            } else {
                6
            };
            return self.find_move(KING_TYPE, king_dst, None, None, 0, false);
        }
        if san == "O-O-O" || san == "0-0-0" {
            let king_sq = if self.active_player_bit() == WHITE_MOVE {
                60
            } else {
                4
            };
            let king_dst = if self.active_player_bit() == WHITE_MOVE {
                58
            } else {
                2
            };
            return self.find_move(KING_TYPE, king_dst, None, None, 0, false);
        }

        let bytes = san.as_bytes();
        let mut idx = 0;

        // Piece letter (optional for pawns)
        let pt = match bytes.get(idx) {
            Some(b'K') => {
                idx += 1;
                KING_TYPE
            }
            Some(b'Q') => {
                idx += 1;
                QUEEN_TYPE
            }
            Some(b'R') => {
                idx += 1;
                ROOK_TYPE
            }
            Some(b'B') => {
                idx += 1;
                BISHOP_TYPE
            }
            Some(b'N') => {
                idx += 1;
                KNIGHT_TYPE
            }
            _ => PAWN_TYPE,
        };

        // Disambiguation and destination parsing
        let mut disambig_file: Option<i8> = None;
        let mut disambig_rank: Option<i8> = None;
        let mut is_capture = false;
        let dst: u8;

        if pt == PAWN_TYPE {
            // For pawns: [file]x[file][rank](=[piece])?
            // Check for disambiguation file (only if followed by 'x')
            if let Some(&ch) = bytes.get(idx) {
                if (b'a'..=b'h').contains(&ch) {
                    if let Some(&next_ch) = bytes.get(idx + 1) {
                        if next_ch == b'x' {
                            disambig_file = Some((ch - b'a') as i8);
                            idx += 1;
                        }
                    }
                }
            }

            // Check for capture
            if let Some(&ch) = bytes.get(idx) {
                if ch == b'x' {
                    is_capture = true;
                    idx += 1;
                }
            }

            // Destination square
            if bytes.len().saturating_sub(idx) < 2 {
                return Err(Error::InvalidMove);
            }
            let dst_file = (bytes[idx] - b'a') as i8;
            let dst_rank = (bytes[idx + 1] - b'1') as i8;
            dst = rank_file_to_sq(7 - dst_rank, dst_file);
            idx += 2;
        } else {
            // For pieces: [KQRBN][file]?[rank]?x?[file][rank](=[piece])?
            // The destination is always the last 2 characters before any promotion
            // Find the destination by looking for the last file+rank pattern

            // First, check for capture
            if let Some(&ch) = bytes.get(idx) {
                if ch == b'x' {
                    is_capture = true;
                    idx += 1;
                }
            }

            // For pieces, we need to find the destination square
            // The destination is the last file+rank before any promotion
            // Strategy: scan from the end to find the destination

            // Find the position of the destination square
            // The destination is [file][rank] and is followed by optional [=piece]
            let mut dst_idx = idx;
            let mut found_dst = false;

            // Look for the last occurrence of [a-h][1-8] which is the destination
            let mut i = idx;
            while i + 1 < bytes.len() {
                if (b'a'..=b'h').contains(&bytes[i]) && (b'1'..=b'8').contains(&bytes[i + 1]) {
                    // Check if this is followed by '=' or end of string
                    if i + 2 >= bytes.len() || bytes[i + 2] == b'=' {
                        dst_idx = i;
                        found_dst = true;
                    }
                }
                i += 1;
            }

            if !found_dst {
                return Err(Error::InvalidMove);
            }

            // Parse disambiguation (everything between piece letter and destination)
            let disambig_bytes = &bytes[idx..dst_idx];
            let mut di = 0;
            while di < disambig_bytes.len() {
                let ch = disambig_bytes[di];
                if (b'a'..=b'h').contains(&ch) {
                    disambig_file = Some((ch - b'a') as i8);
                } else if (b'1'..=b'8').contains(&ch) {
                    // Convert SAN rank (1-8) to code rank (0=rank8, 7=rank1)
                    disambig_rank = Some(7 - (ch - b'1') as i8);
                }
                di += 1;
            }

            // Parse destination
            let dst_file = (bytes[dst_idx] - b'a') as i8;
            let dst_rank = (bytes[dst_idx + 1] - b'1') as i8;
            dst = rank_file_to_sq(7 - dst_rank, dst_file);
            idx = dst_idx + 2;
        }

        // Promotion
        let mut promo = 0;
        if bytes.len() > idx && bytes[idx] == b'=' {
            idx += 1;
            if bytes.len() <= idx {
                return Err(Error::InvalidMove);
            }
            promo = match bytes[idx] {
                b'Q' => QUEEN_TYPE,
                b'R' => ROOK_TYPE,
                b'B' => BISHOP_TYPE,
                b'N' => KNIGHT_TYPE,
                _ => return Err(Error::InvalidMove),
            };
        }

        // Find the matching move
        self.find_move(pt, dst, disambig_file, disambig_rank, promo, is_capture)
    }

    fn find_move(
        &self,
        pt: u8,
        dst: u8,
        disambig_file: Option<i8>,
        disambig_rank: Option<i8>,
        promo: u8,
        is_capture: bool,
    ) -> Result<Move, Error> {
        let active_player = self.active_player_bit();
        let mut matches = vec![];

        for m in self.all_valid_moves() {
            if m.dst() != dst || m.promo() != promo {
                continue;
            }
            let src = m.src();
            let src_bits = self.square_bits(Square(src));
            if piece_type(src_bits) != pt {
                continue;
            }
            if let Some(df) = disambig_file {
                if sq_file(src) != df {
                    continue;
                }
            }
            if let Some(dr) = disambig_rank {
                if sq_rank(src) != dr {
                    continue;
                }
            }
            // For pawns, check capture notation
            if pt == PAWN_TYPE {
                let dst_bits = self.square_bits(Square(dst));
                let actual_capture =
                    dst_bits != 0 || self.en_passant_target(active_player) == Some(dst);
                if is_capture != actual_capture {
                    continue;
                }
            }
            matches.push(m);
        }

        if matches.len() == 1 {
            Ok(matches[0])
        } else {
            Err(Error::InvalidMove)
        }
    }

    pub fn to_san(&self, m: &Move) -> String {
        let mut result = String::new();

        let src = m.src();
        let dst = m.dst();
        let piece = self.square_bits(Square(src));
        let pt = piece_type(piece);
        let active_player = self.active_player_bit();

        // Castling
        if pt == KING_TYPE {
            if (src == 60 && dst == 62) || (src == 4 && dst == 6) {
                result.push_str("O-O");
            } else if (src == 60 && dst == 58) || (src == 4 && dst == 2) {
                result.push_str("O-O-O");
            } else {
                result.push('K');
            }
        } else if pt == PAWN_TYPE {
            // Pawn move
            let dst_bits = self.square_bits(Square(dst));
            let is_capture =
                dst_bits != 0 || self.en_passant_target(active_player) == Some(dst);
            if is_capture {
                let src_file = sq_file(src);
                result.push((b'a' + src_file as u8) as char);
                result.push('x');
            }
            let dst_file = sq_file(dst);
            let dst_rank = sq_rank(dst);
            result.push((b'a' + dst_file as u8) as char);
            result.push((b'1' + (7 - dst_rank) as u8) as char);
            if m.promo() != 0 {
                result.push('=');
                result.push(match m.promo() {
                    QUEEN_TYPE => 'Q',
                    ROOK_TYPE => 'R',
                    BISHOP_TYPE => 'B',
                    KNIGHT_TYPE => 'N',
                    _ => '?',
                });
            }
        } else {
            // Piece move
            result.push(match pt {
                KING_TYPE => 'K',
                QUEEN_TYPE => 'Q',
                ROOK_TYPE => 'R',
                BISHOP_TYPE => 'B',
                KNIGHT_TYPE => 'N',
                _ => '?',
            });

            // Disambiguation
            let mut same_type_moves = vec![];
            for other_m in self.all_valid_moves() {
                if other_m.dst() == dst && other_m.src() != src {
                    let other_bits = self.square_bits(Square(other_m.src()));
                    if piece_type(other_bits) == pt {
                        same_type_moves.push(other_m);
                    }
                }
            }

            if !same_type_moves.is_empty() {
                let src_file = sq_file(src);
                let src_rank = sq_rank(src);
                let file_unique = same_type_moves.iter().all(|m| sq_file(m.src()) != src_file);
                let rank_unique = same_type_moves.iter().all(|m| sq_rank(m.src()) != src_rank);

                if file_unique {
                    result.push((b'a' + src_file as u8) as char);
                } else if rank_unique {
                    result.push((b'1' + (7 - src_rank) as u8) as char);
                } else {
                    result.push((b'a' + src_file as u8) as char);
                    result.push((b'1' + (7 - src_rank) as u8) as char);
                }
            }

            // Capture
            let dst_bits = self.square_bits(Square(dst));
            if dst_bits != 0 {
                result.push('x');
            }

            // Destination
            let dst_file = sq_file(dst);
            let dst_rank = sq_rank(dst);
            result.push((b'a' + dst_file as u8) as char);
            result.push((b'1' + (7 - dst_rank) as u8) as char);
        }

        // Check/mate suffix
        let mut test_fen = *self;
        test_fen.apply_move_unchecked(*m);
        let opponent = active_player ^ 1;
        let king_sq = test_fen.king_square(opponent);
        if test_fen.is_attacked_by(king_sq, active_player) {
            // Check if it's checkmate
            let has_legal_moves = (0..64).any(|sq| {
                !test_fen.all_valid_moves_from_square(Square(sq)).is_empty()
            });
            if has_legal_moves {
                result.push('+');
            } else {
                result.push('#');
            }
        }

        result
    }

    fn apply_move_unchecked(&mut self, m: Move) {
        let src = m.src();
        let dst = m.dst();

        let piece = self.square_bits(Square(src));
        let pt = piece_type(piece);
        let color = piece_color(piece);
        let active_player = self.active_player_bit();
        let castle = self.castle_bits();
        let ep_target = self.en_passant_target(active_player);

        // Determine move characteristics
        let is_capture =
            self.square_bits(Square(dst)) != 0 || (pt == PAWN_TYPE && ep_target == Some(dst));
        let is_pawn_move = pt == PAWN_TYPE;
        let is_double_push = is_pawn_move && (src as i8 - dst as i8).abs() == 16;

        // Execute the move
        self.set_square(Square(src), EMPTY);

        // Handle promotion: keep color bit, replace type with promo type
        if m.promo() != 0 {
            self.set_square(Square(dst), (piece & 0b1000) | m.promo());
        } else {
            self.set_square(Square(dst), piece);
        }

        // Handle en passant capture: remove the captured pawn (behind dst)
        if pt == PAWN_TYPE && ep_target == Some(dst) {
            let captured_sq = if color == 0 { dst + 8 } else { dst - 8 };
            self.set_square(Square(captured_sq), EMPTY);
        }

        // Handle castling: move the rook
        if pt == KING_TYPE {
            match (src, dst) {
                (60, 62) => {
                    // White kingside: rook h1 -> f1
                    self.set_square(Square(61), WHITE_ROOK);
                    self.set_square(Square(63), EMPTY);
                }
                (60, 58) => {
                    // White queenside: rook a1 -> d1
                    self.set_square(Square(59), WHITE_ROOK);
                    self.set_square(Square(56), EMPTY);
                }
                (4, 6) => {
                    // Black kingside: rook h8 -> f8
                    self.set_square(Square(5), BLACK_ROOK);
                    self.set_square(Square(7), EMPTY);
                }
                (4, 2) => {
                    // Black queenside: rook a8 -> d8
                    self.set_square(Square(3), BLACK_ROOK);
                    self.set_square(Square(0), EMPTY);
                }
                _ => {}
            }
        }

        // Update castling rights
        let mut new_castle = castle;
        // King moves lose both rights for that color
        if pt == KING_TYPE {
            if color == 0 {
                new_castle &= !(WHITE_KINGSIDE_CASTLE | WHITE_QUEENSIDE_CASTLE);
            } else {
                new_castle &= !(BLACK_KINGSIDE_CASTLE | BLACK_QUEENSIDE_CASTLE);
            }
        }
        // Rook moves or is captured
        if src == 63 || dst == 63 {
            new_castle &= !WHITE_KINGSIDE_CASTLE;
        } // h1
        if src == 56 || dst == 56 {
            new_castle &= !WHITE_QUEENSIDE_CASTLE;
        } // a1
        if src == 7 || dst == 7 {
            new_castle &= !BLACK_KINGSIDE_CASTLE;
        } // h8
        if src == 0 || dst == 0 {
            new_castle &= !BLACK_QUEENSIDE_CASTLE;
        } // a8
        self.set_castle_bits(new_castle);

        // Update en passant target
        if is_double_push {
            let ep_sq = if color == 0 { src - 8 } else { src + 8 };
            self.set_en_passant(sq_file(ep_sq) as u8, true);
        } else {
            self.set_en_passant(0, false);
        }

        // Update active player
        self.set_active_player(active_player ^ 1);

        // Update counters
        if is_pawn_move || is_capture {
            self.set_halfmove_cnt(0);
        } else {
            self.inc_halfmove_cnt();
        }
        if active_player == 1 {
            // Black just moved
            self.inc_fullmove_cnt();
        }
    }

    fn all_valid_moves(&self) -> Vec<Move> {
        let mut moves = vec![];
        for i in 0..SQUARE_CNT {
            moves.extend(self.all_valid_moves_from_square(Square(i as u8)));
        }
        moves
    }

    fn king_square(&self, color: u8) -> u8 {
        for sq in 0..64 {
            let bits = self.square_bits(Square(sq));
            if bits != 0 && (bits >> 3) == color && piece_type(bits) == KING_TYPE {
                return sq;
            }
        }
        panic!("no king found for color {color}");
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

    fn set_castle_bits(&mut self, bits: u8) {
        self.0[32] = (self.0[32] & 0b00001111) | (bits << 4);
    }

    fn set_en_passant(&mut self, file: u8, has_ep: bool) {
        let ep_bits = if has_ep { 0b1000 | file } else { 0 };
        self.0[32] = (self.0[32] & 0b11110000) | ep_bits;
    }

    fn set_active_player(&mut self, player: u8) {
        self.0[33] = (self.0[33] & 0b01111111) | (player << 7);
    }

    fn set_halfmove_cnt(&mut self, cnt: u8) {
        self.0[33] = (self.0[33] & 0b10000000) | (cnt & 0b01111111);
    }

    fn set_fullmove_cnt(&mut self, cnt: u8) {
        self.0[34] = cnt;
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

        // Filter: only keep moves that don't leave the king in check
        moves.retain(|&m| {
            let mut test_fen = *self;
            test_fen.apply_move_unchecked(m);
            let king_sq = test_fen.king_square(active_player);
            !test_fen.is_attacked_by(king_sq, active_player ^ 1)
        });

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

impl FromStr for Fen {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let fields: Vec<&str> = s.split_whitespace().collect();
        if fields.len() != 6 {
            return Err(Error::InvalidFenString(s.to_string()));
        }

        // Parse piece placement
        let placement = fields[0];
        let mut board = [EMPTY; 64];
        let ranks: Vec<&str> = placement.split('/').collect();
        if ranks.len() != 8 {
            return Err(Error::InvalidFenString(s.to_string()));
        }

        for (i, rank_str) in ranks.iter().enumerate() {
            let rank = i as u8; // 0 = rank 8, 7 = rank 1
            let mut file = 0u8;
            for ch in rank_str.chars() {
                if ch.is_ascii_digit() {
                    let empty = ch.to_digit(10).unwrap() as u8;
                    file += empty;
                } else {
                    let piece = match ch {
                        'P' => WHITE_PAWN,
                        'R' => WHITE_ROOK,
                        'N' => WHITE_KNIGHT,
                        'B' => WHITE_BISHOP,
                        'Q' => WHITE_QUEEN,
                        'K' => WHITE_KING,
                        'p' => BLACK_PAWN,
                        'r' => BLACK_ROOK,
                        'n' => BLACK_KNIGHT,
                        'b' => BLACK_BISHOP,
                        'q' => BLACK_QUEEN,
                        'k' => BLACK_KING,
                        _ => return Err(Error::InvalidFenString(s.to_string())),
                    };
                    if file >= 8 {
                        return Err(Error::InvalidFenString(s.to_string()));
                    }
                    board[(rank * 8 + file) as usize] = piece;
                    file += 1;
                }
            }
            if file != 8 {
                return Err(Error::InvalidFenString(s.to_string()));
            }
        }

        // Parse active color
        let active_player = match fields[1] {
            "w" => WHITE_MOVE,
            "b" => BLACK_MOVE,
            _ => return Err(Error::InvalidFenString(s.to_string())),
        };

        // Parse castling
        let mut castle = NO_CASTLE;
        if fields[2] != "-" {
            for ch in fields[2].chars() {
                match ch {
                    'K' => castle |= WHITE_KINGSIDE_CASTLE,
                    'Q' => castle |= WHITE_QUEENSIDE_CASTLE,
                    'k' => castle |= BLACK_KINGSIDE_CASTLE,
                    'q' => castle |= BLACK_QUEENSIDE_CASTLE,
                    _ => return Err(Error::InvalidFenString(s.to_string())),
                }
            }
        }

        // Parse en passant
        let mut ep_file = 0u8;
        let mut has_ep = false;
        if fields[3] != "-" {
            let ep_chars: Vec<char> = fields[3].chars().collect();
            if ep_chars.len() != 2 {
                return Err(Error::InvalidFenString(s.to_string()));
            }
            let file_ch = ep_chars[0];
            let rank_ch = ep_chars[1];
            if !('a'..='h').contains(&file_ch) {
                return Err(Error::InvalidFenString(s.to_string()));
            }
            let rank_num = rank_ch.to_digit(10);
            if rank_num.is_none() {
                return Err(Error::InvalidFenString(s.to_string()));
            }
            let rank_num = rank_num.unwrap() as u8;
            // En passant rank: 6 if white to move (black just double-pushed),
            // 3 if black to move (white just double-pushed)
            if active_player == WHITE_MOVE && rank_num != 6 {
                return Err(Error::InvalidFenString(s.to_string()));
            }
            if active_player == BLACK_MOVE && rank_num != 3 {
                return Err(Error::InvalidFenString(s.to_string()));
            }
            ep_file = (file_ch as u8) - b'a';
            has_ep = true;
        }

        // Parse halfmove counter
        let halfmove = fields[4]
            .parse::<u8>()
            .map_err(|_| Error::InvalidFenString(s.to_string()))?;
        if halfmove > MAX_HALFMOVE_CNT {
            return Err(Error::InvalidFenString(s.to_string()));
        }

        // Parse fullmove counter
        let fullmove = fields[5]
            .parse::<u8>()
            .map_err(|_| Error::InvalidFenString(s.to_string()))?;
        if fullmove == 0 {
            return Err(Error::InvalidFenString(s.to_string()));
        }

        // Build the Fen
        let mut fen = Fen([0u8; 35]);

        // Set board squares
        for sq in 0..64 {
            fen.set_square(Square(sq), board[sq as usize]);
        }

        // Set castling
        fen.set_castle_bits(castle);

        // Set en passant
        fen.set_en_passant(ep_file, has_ep);

        // Set active player
        fen.set_active_player(active_player);

        // Set counters
        fen.set_halfmove_cnt(halfmove);
        fen.set_fullmove_cnt(fullmove);

        // Validate: exactly one king per side
        let mut white_kings = 0;
        let mut black_kings = 0;
        for sq in 0..64 {
            let bits = fen.square_bits(Square(sq));
            if bits == WHITE_KING {
                white_kings += 1;
            }
            if bits == BLACK_KING {
                black_kings += 1;
            }
        }
        if white_kings != 1 || black_kings != 1 {
            return Err(Error::InvalidFenString(s.to_string()));
        }

        // Validate: no pawns on rank 1 or 8
        for file in 0..8 {
            let sq_rank8 = file; // rank 0 = rank 8
            let sq_rank1 = 7 * 8 + file; // rank 7 = rank 1
            let bits8 = fen.square_bits(Square(sq_rank8));
            let bits1 = fen.square_bits(Square(sq_rank1));
            if piece_type(bits8) == PAWN_TYPE || piece_type(bits1) == PAWN_TYPE {
                return Err(Error::InvalidFenString(s.to_string()));
            }
        }

        // Validate: side not to move cannot be in check
        let opponent = active_player ^ 1;
        let king_sq = fen.king_square(opponent);
        if fen.is_attacked_by(king_sq, active_player) {
            return Err(Error::InvalidFenString(s.to_string()));
        }

        Ok(fen)
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
        assert_eq!(m.dst(), 4); // e8
        assert_eq!(m.promo(), QUEEN_TYPE);

        let m2: Move = "a7-a8-n".parse().unwrap();
        assert_eq!(m2.src(), 8); // a7
        assert_eq!(m2.dst(), 0); // a8
        assert_eq!(m2.promo(), KNIGHT_TYPE);
    }

    #[test]
    fn move_parse_corners() {
        let m: Move = "a8-h1".parse().unwrap();
        assert_eq!(m.src(), 0); // a8
        assert_eq!(m.dst(), 63); // h1

        let m2: Move = "h1-a8".parse().unwrap();
        assert_eq!(m2.src(), 63); // h1
        assert_eq!(m2.dst(), 0); // a8
    }

    #[test]
    fn move_parse_errors() {
        assert!("e2e4".parse::<Move>().is_err()); // missing dashes
        assert!("e2e4e".parse::<Move>().is_err()); // too long
        assert!("e2".parse::<Move>().is_err()); // too short
        assert!("i2-e4".parse::<Move>().is_err()); // invalid file
        assert!("e9-e4".parse::<Move>().is_err()); // invalid rank
        assert!("e7-e8-x".parse::<Move>().is_err()); // invalid promo
    }

    // Phase 1 tests

    fn empty_fen() -> Fen {
        let mut fen = Fen::new();
        for sq in 0..64 {
            fen.set_square(Square(sq), EMPTY);
        }
        fen.set_castle_bits(NO_CASTLE);
        fen.set_en_passant(0, false);
        fen.set_active_player(WHITE_MOVE);
        fen.set_halfmove_cnt(0);
        fen.set_fullmove_cnt(1);
        fen
    }

    #[test]
    fn promotion_queen() {
        let mut fen = empty_fen();
        fen.set_square(Square(12), WHITE_PAWN); // e7
        fen.set_square(Square(60), WHITE_KING); // e1
        fen.set_square(Square(0), BLACK_KING); // a8

        let m: Move = "e7-e8-q".parse().unwrap();
        fen.try_move(m).unwrap();

        assert_eq!(fen.square_bits(Square(4)), WHITE_QUEEN); // e8
        assert_eq!(fen.square_bits(Square(12)), EMPTY); // e7
        assert_eq!(fen.active_player_bit(), BLACK_MOVE);
    }

    #[test]
    fn en_passant_capture() {
        let mut fen = empty_fen();
        fen.set_square(Square(28), WHITE_PAWN); // e5
        fen.set_square(Square(11), BLACK_PAWN); // d7
        fen.set_square(Square(60), WHITE_KING); // e1
        fen.set_square(Square(4), BLACK_KING); // e8

        // Black plays d7-d5 (double push)
        fen.set_active_player(BLACK_MOVE);
        let m1: Move = "d7-d5".parse().unwrap();
        fen.try_move(m1).unwrap();

        // White plays e5xd6 (en passant)
        let m2: Move = "e5-d6".parse().unwrap();
        fen.try_move(m2).unwrap();

        assert_eq!(fen.square_bits(Square(19)), WHITE_PAWN); // d6
        assert_eq!(fen.square_bits(Square(27)), EMPTY); // d5 (captured pawn removed)
        assert_eq!(fen.square_bits(Square(28)), EMPTY); // e5
        assert_eq!(fen.active_player_bit(), BLACK_MOVE);
    }

    #[test]
    fn castling_kingside() {
        let mut fen = empty_fen();
        fen.set_square(Square(60), WHITE_KING); // e1
        fen.set_square(Square(63), WHITE_ROOK); // h1
        fen.set_square(Square(4), BLACK_KING); // e8
        fen.set_castle_bits(WHITE_KINGSIDE_CASTLE);

        let m: Move = "e1-g1".parse().unwrap();
        fen.try_move(m).unwrap();

        assert_eq!(fen.square_bits(Square(62)), WHITE_KING); // g1
        assert_eq!(fen.square_bits(Square(61)), WHITE_ROOK); // f1
        assert_eq!(fen.square_bits(Square(60)), EMPTY); // e1
        assert_eq!(fen.square_bits(Square(63)), EMPTY); // h1
        assert_eq!(fen.active_player_bit(), BLACK_MOVE);
    }

    #[test]
    fn castling_queenside() {
        let mut fen = empty_fen();
        fen.set_square(Square(60), WHITE_KING); // e1
        fen.set_square(Square(56), WHITE_ROOK); // a1
        fen.set_square(Square(4), BLACK_KING); // e8
        fen.set_castle_bits(WHITE_QUEENSIDE_CASTLE);

        let m: Move = "e1-c1".parse().unwrap();
        fen.try_move(m).unwrap();

        assert_eq!(fen.square_bits(Square(58)), WHITE_KING); // c1
        assert_eq!(fen.square_bits(Square(59)), WHITE_ROOK); // d1
        assert_eq!(fen.square_bits(Square(60)), EMPTY); // e1
        assert_eq!(fen.square_bits(Square(56)), EMPTY); // a1
        assert_eq!(fen.active_player_bit(), BLACK_MOVE);
    }

    #[test]
    fn castling_rights_lost_on_rook_capture() {
        let mut fen = empty_fen();
        fen.set_square(Square(60), WHITE_KING); // e1
        fen.set_square(Square(56), WHITE_ROOK); // a1
        fen.set_square(Square(0), BLACK_ROOK); // a8
        fen.set_square(Square(4), BLACK_KING); // e8
        fen.set_castle_bits(WHITE_QUEENSIDE_CASTLE);

        // Black plays a8xa1 (captures rook)
        fen.set_active_player(BLACK_MOVE);
        let m: Move = "a8-a1".parse().unwrap();
        fen.try_move(m).unwrap();

        assert_eq!(fen.castle_bits() & WHITE_QUEENSIDE_CASTLE, 0);
    }

    #[test]
    fn halfmove_reset_on_pawn_move() {
        let mut fen = empty_fen();
        fen.set_square(Square(52), WHITE_PAWN); // e2
        fen.set_square(Square(60), WHITE_KING); // e1
        fen.set_square(Square(4), BLACK_KING); // e8
        fen.set_halfmove_cnt(5);

        let m: Move = "e2-e4".parse().unwrap();
        fen.try_move(m).unwrap();

        assert_eq!(fen.halfmove_cnt(), 0);
    }

    #[test]
    fn halfmove_reset_on_capture() {
        let mut fen = empty_fen();
        fen.set_square(Square(36), WHITE_PAWN); // e4
        fen.set_square(Square(27), BLACK_PAWN); // d5
        fen.set_square(Square(60), WHITE_KING); // e1
        fen.set_square(Square(4), BLACK_KING); // e8
        fen.set_halfmove_cnt(5);

        let m: Move = "e4-d5".parse().unwrap();
        fen.try_move(m).unwrap();

        assert_eq!(fen.halfmove_cnt(), 0);
    }

    #[test]
    fn halfmove_increment_on_quiet_move() {
        let mut fen = empty_fen();
        fen.set_square(Square(60), WHITE_KING); // e1
        fen.set_square(Square(4), BLACK_KING); // e8

        let m: Move = "e1-e2".parse().unwrap();
        fen.try_move(m).unwrap();

        assert_eq!(fen.halfmove_cnt(), 1);
    }

    #[test]
    fn fullmove_increment_after_black() {
        let mut fen = empty_fen();
        fen.set_square(Square(60), WHITE_KING); // e1
        fen.set_square(Square(4), BLACK_KING); // e8
        fen.set_fullmove_cnt(1);

        // White moves
        let m1: Move = "e1-e2".parse().unwrap();
        fen.try_move(m1).unwrap();
        assert_eq!(fen.fullmove_cnt(), 1); // Not incremented yet

        // Black moves
        let m2: Move = "e8-e7".parse().unwrap();
        fen.try_move(m2).unwrap();
        assert_eq!(fen.fullmove_cnt(), 2); // Incremented after black
    }

    #[test]
    fn en_passant_target_set_on_double_push() {
        let mut fen = empty_fen();
        fen.set_square(Square(52), WHITE_PAWN); // e2
        fen.set_square(Square(60), WHITE_KING); // e1
        fen.set_square(Square(4), BLACK_KING); // e8

        let m: Move = "e2-e4".parse().unwrap();
        fen.try_move(m).unwrap();

        // En passant target should be e3 (sq 44)
        let ep_target = fen.en_passant_target(BLACK_MOVE);
        assert_eq!(ep_target, Some(44)); // e3
    }

    #[test]
    fn en_passant_target_cleared_on_other_moves() {
        let mut fen = empty_fen();
        fen.set_square(Square(52), WHITE_PAWN); // e2
        fen.set_square(Square(11), BLACK_PAWN); // d7
        fen.set_square(Square(60), WHITE_KING); // e1
        fen.set_square(Square(4), BLACK_KING); // e8

        // White plays e2-e4 (sets ep target to e3)
        let m1: Move = "e2-e4".parse().unwrap();
        fen.try_move(m1).unwrap();
        assert_eq!(fen.en_passant_target(BLACK_MOVE), Some(44)); // e3

        // Black plays d7-d6 (should clear ep target)
        let m2: Move = "d7-d6".parse().unwrap();
        fen.try_move(m2).unwrap();
        assert_eq!(fen.en_passant_target(WHITE_MOVE), None);
    }

    // Phase 2 tests

    #[test]
    fn pinned_piece_cannot_move_off_pin_line() {
        let mut fen = empty_fen();
        fen.set_square(Square(60), WHITE_KING); // e1
        fen.set_square(Square(52), WHITE_ROOK); // e2 — pinned
        fen.set_square(Square(4), BLACK_ROOK); // e8 — pinning
        fen.set_square(Square(0), BLACK_KING); // a8

        let moves = fen.all_valid_moves_from_square(Square(52));
        // Rook can only move along the e-file (e3-e7), not horizontally
        for m in &moves {
            assert_eq!(sq_file(m.dst()), 4, "pinned rook can only stay on e-file");
        }
        // Should have moves along the e-file
        assert!(!moves.is_empty());
    }

    #[test]
    fn king_cannot_move_into_check() {
        let mut fen = empty_fen();
        fen.set_square(Square(60), WHITE_KING); // e1
        fen.set_square(Square(48), BLACK_ROOK); // a2
        fen.set_square(Square(0), BLACK_KING); // a8

        let moves = fen.all_valid_moves_from_square(Square(60));
        // King cannot move to d2, e2, f2 — attacked by black rook on a2
        for m in &moves {
            assert_ne!(m.dst(), 51, "king cannot move to d2");
            assert_ne!(m.dst(), 52, "king cannot move to e2");
            assert_ne!(m.dst(), 53, "king cannot move to f2");
        }
        // King can move to d1 and f1
        assert!(moves.iter().any(|m| m.dst() == 59), "king can move to d1");
        assert!(moves.iter().any(|m| m.dst() == 61), "king can move to f1");
    }

    #[test]
    fn en_passant_pinned_pawn_cannot_capture() {
        let mut fen = empty_fen();
        fen.set_square(Square(60), WHITE_KING); // e1
        fen.set_square(Square(28), WHITE_PAWN); // e5 — pinned
        fen.set_square(Square(11), BLACK_PAWN); // d7
        fen.set_square(Square(4), BLACK_ROOK); // e8 — pinning
        fen.set_square(Square(0), BLACK_KING); // a8

        // Black plays d7-d5 (double push)
        fen.set_active_player(BLACK_MOVE);
        let m1: Move = "d7-d5".parse().unwrap();
        fen.try_move(m1).unwrap();

        // White's e5 pawn is pinned — en passant capture would expose king
        let moves = fen.all_valid_moves_from_square(Square(28));
        for m in &moves {
            assert_ne!(m.dst(), 19, "pinned pawn cannot capture en passant");
        }
        // Pawn can still move forward to e6 (sq 20)
        assert!(
            moves.iter().any(|m| m.dst() == 20),
            "pinned pawn can move forward"
        );
    }

    // Phase 3 tests

    #[test]
    fn parse_startpos() {
        let fen: Fen = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
            .parse()
            .unwrap();
        assert_eq!(fen.square_bits(Square(60)), WHITE_KING); // e1
        assert_eq!(fen.square_bits(Square(4)), BLACK_KING); // e8
        assert_eq!(fen.square_bits(Square(52)), WHITE_PAWN); // e2
        assert_eq!(fen.square_bits(Square(12)), BLACK_PAWN); // e7
        assert_eq!(fen.active_player_bit(), WHITE_MOVE);
        assert_eq!(
            fen.castle_bits(),
            WHITE_KINGSIDE_CASTLE
                | WHITE_QUEENSIDE_CASTLE
                | BLACK_KINGSIDE_CASTLE
                | BLACK_QUEENSIDE_CASTLE
        );
        assert_eq!(fen.halfmove_cnt(), 0);
        assert_eq!(fen.fullmove_cnt(), 1);
    }

    #[test]
    fn parse_midgame_fen() {
        let fen: Fen = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1"
            .parse()
            .unwrap();
        assert_eq!(fen.square_bits(Square(60)), WHITE_KING); // e1
        assert_eq!(fen.square_bits(Square(4)), BLACK_KING); // e8
        assert_eq!(fen.active_player_bit(), WHITE_MOVE);
        assert_eq!(fen.halfmove_cnt(), 0);
        assert_eq!(fen.fullmove_cnt(), 1);
    }

    #[test]
    fn fen_roundtrip_startpos() {
        let fen: Fen = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
            .parse()
            .unwrap();
        assert_eq!(
            fen.to_fen(),
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
        );
    }

    #[test]
    fn fen_roundtrip_midgame() {
        let fen: Fen = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1"
            .parse()
            .unwrap();
        assert_eq!(
            fen.to_fen(),
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1"
        );
    }

    #[test]
    fn fen_roundtrip_no_castling() {
        let fen: Fen = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w - - 0 1"
            .parse()
            .unwrap();
        assert_eq!(
            fen.to_fen(),
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w - - 0 1"
        );
    }

    #[test]
    fn fen_roundtrip_black_to_move() {
        let fen: Fen = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR b KQkq - 0 1"
            .parse()
            .unwrap();
        assert_eq!(
            fen.to_fen(),
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR b KQkq - 0 1"
        );
    }

    #[test]
    fn fen_roundtrip_en_passant() {
        let fen: Fen = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1"
            .parse()
            .unwrap();
        assert_eq!(
            fen.to_fen(),
            "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1"
        );
    }

    #[test]
    fn fen_reject_invalid() {
        // Missing fields
        assert!(
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0"
                .parse::<Fen>()
                .is_err()
        );
        // Invalid piece letter
        assert!(
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNX w KQkq - 0 1"
                .parse::<Fen>()
                .is_err()
        );
        // Invalid active color
        assert!(
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR x KQkq - 0 1"
                .parse::<Fen>()
                .is_err()
        );
        // Invalid castling
        assert!(
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkqX - 0 1"
                .parse::<Fen>()
                .is_err()
        );
        // Invalid en passant
        assert!(
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq e9 0 1"
                .parse::<Fen>()
                .is_err()
        );
        // Invalid halfmove
        assert!(
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - x 1"
                .parse::<Fen>()
                .is_err()
        );
        // Invalid fullmove
        assert!(
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 0"
                .parse::<Fen>()
                .is_err()
        );
        // Two white kings
        assert!(
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNK w KQkq - 0 1"
                .parse::<Fen>()
                .is_err()
        );
        // Pawn on rank 8
        assert!(
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNP w KQkq - 0 1"
                .parse::<Fen>()
                .is_err()
        );
    }

    // Phase 4 tests

    #[test]
    fn san_parse_quiet_move() {
        let fen: Fen = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
            .parse()
            .unwrap();
        let m = fen.parse_san("e4").unwrap();
        assert_eq!(m.src(), 52); // e2
        assert_eq!(m.dst(), 36); // e4
    }

    #[test]
    fn san_parse_capture() {
        let fen: Fen = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
            .parse()
            .unwrap();
        let m = fen.parse_san("Nf3").unwrap();
        assert_eq!(m.src(), 62); // g1
        assert_eq!(m.dst(), 45); // f3
    }

    #[test]
    fn san_parse_pawn_capture() {
        let fen: Fen = "rnbqkbnr/pppppppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1"
            .parse()
            .unwrap();
        let m = fen.parse_san("dxe4").unwrap();
        assert_eq!(m.src(), 27); // d5
        assert_eq!(m.dst(), 36); // e4
    }

    #[test]
    fn san_parse_promotion() {
        let fen: Fen = "8/P7/8/8/8/8/8/k6K w - - 0 1".parse().unwrap();
        let m = fen.parse_san("a8=Q").unwrap();
        assert_eq!(m.src(), 8); // a7
        assert_eq!(m.dst(), 0); // a8
        assert_eq!(m.promo(), QUEEN_TYPE);
    }

    #[test]
    fn san_parse_castling_kingside() {
        let fen: Fen = "r3k2r/pppppppp/8/8/8/8/PPPPPPPP/R3K2R w KQkq - 0 1"
            .parse()
            .unwrap();
        let m = fen.parse_san("O-O").unwrap();
        assert_eq!(m.src(), 60); // e1
        assert_eq!(m.dst(), 62); // g1
    }

    #[test]
    fn san_parse_castling_queenside() {
        let fen: Fen = "r3k2r/pppppppp/8/8/8/8/PPPPPPPP/R3K2R w KQkq - 0 1"
            .parse()
            .unwrap();
        let m = fen.parse_san("O-O-O").unwrap();
        assert_eq!(m.src(), 60); // e1
        assert_eq!(m.dst(), 58); // c1
    }

    #[test]
    fn san_parse_check() {
        // Position where Qh5+ is check (e2/f2 pawns moved out of the way, f7 pawn removed)
        let fen: Fen = "rnbqkbnr/pppp2pp/8/8/8/8/PPPP2PP/RNBQKBNR w KQkq - 0 1"
            .parse()
            .unwrap();
        let m = fen.parse_san("Qh5+").unwrap();
        assert_eq!(m.src(), 59); // d1
        assert_eq!(m.dst(), 31); // h5
    }

    #[test]
    fn san_parse_knight_disambiguation() {
        // Two knights can move to f3: g1 and e5
        let fen: Fen = "8/8/8/4N3/8/8/5N2/k6K w - - 0 1".parse().unwrap();
        let m = fen.parse_san("Nef3").unwrap();
        assert_eq!(m.src(), 28); // e5
        assert_eq!(m.dst(), 45); // f3
    }

    #[test]
    fn san_parse_rook_disambiguation_by_rank() {
        // Two rooks can move to d3: d1 and d5
        let fen: Fen = "7k/8/8/3R4/8/8/8/3R3K w - - 0 1".parse().unwrap();
        let m = fen.parse_san("R1d3").unwrap();
        assert_eq!(m.src(), 59); // d1
        assert_eq!(m.dst(), 43); // d3
    }

    #[test]
    fn san_format_quiet_move() {
        let fen: Fen = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
            .parse()
            .unwrap();
        let m = fen.parse_san("e4").unwrap();
        assert_eq!(fen.to_san(&m), "e4");
    }

    #[test]
    fn san_format_capture() {
        let fen: Fen = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
            .parse()
            .unwrap();
        let m = fen.parse_san("Nf3").unwrap();
        assert_eq!(fen.to_san(&m), "Nf3");
    }

    #[test]
    fn san_format_promotion() {
        // Queen on a8 attacks black king on a1 (same file) — this IS check
        let fen: Fen = "8/P7/8/8/8/8/8/k6K w - - 0 1".parse().unwrap();
        let m = fen.parse_san("a8=Q").unwrap();
        assert_eq!(fen.to_san(&m), "a8=Q+");
    }

    #[test]
    fn san_format_castling() {
        let fen: Fen = "r3k2r/pppppppp/8/8/8/8/PPPPPPPP/R3K2R w KQkq - 0 1"
            .parse()
            .unwrap();
        let m = fen.parse_san("O-O").unwrap();
        assert_eq!(fen.to_san(&m), "O-O");
    }

    #[test]
    fn san_format_check() {
        let fen: Fen = "rnbqkbnr/pppp2pp/8/8/8/8/PPPP2PP/RNBQKBNR w KQkq - 0 1"
            .parse()
            .unwrap();
        let m = fen.parse_san("Qh5+").unwrap();
        assert_eq!(fen.to_san(&m), "Qh5+");
    }

    #[test]
    fn san_format_knight_disambiguation() {
        // Knights on e5 and g1 can both move to f3
        let fen: Fen = "7k/8/8/4N3/8/8/8/6NK w - - 0 1".parse().unwrap();
        let m = fen.parse_san("Nef3").unwrap();
        assert_eq!(fen.to_san(&m), "Nef3");
    }

    #[test]
    fn san_roundtrip_startpos() {
        let fen: Fen = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
            .parse()
            .unwrap();
        let m = fen.parse_san("e4").unwrap();
        assert_eq!(fen.to_san(&m), "e4");
    }

    #[test]
    fn san_roundtrip_midgame() {
        let fen: Fen = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1"
            .parse()
            .unwrap();
        let m = fen.parse_san("O-O").unwrap();
        assert_eq!(fen.to_san(&m), "O-O");
    }
}
