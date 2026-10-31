//! Chess rules, SAN and FEN parsing, move generation, engine. No I/O.
#![expect(
    missing_docs,
    clippy::missing_errors_doc,
    reason = "doc comments are tracked in TODO.md"
)]
#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "squares, bitboard indices, and counters are small; perft checks the arithmetic"
)]

mod attacks;
pub mod engine;
mod player;

use attacks::{
    BISHOP_RAYS, KING_ATTACKS, KNIGHT_ATTACKS, PAWN_ATTACKS_WHITE, PAWN_PUSHES_WHITE, QUEEN_RAYS,
    ROOK_RAYS, bitboard_iter, flip_rank_bb, flip_rank_sq, rank_file_to_sq, sliding_attacks,
    sq_file, sq_rank,
};
pub use player::{Match, Player, RandomPlayer};

use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

const SQUARE_CNT: usize = 64;
/// The halfmove clock is stored in 7 bits.
const MAX_HALFMOVE_CNT: u8 = 127;
/// 50 full moves without a pawn move or capture = 100 plies.
const FIFTY_MOVE_RULE_PLIES: u8 = 100;

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

// Error

/// Errors from parsing FEN or SAN and playing moves.
#[derive(Debug)]
pub enum Error {
    /// Indicates FEN string failed to parse or is an illegal position. `String` is the input string.
    InvalidFenString(String),
    /// Returned when move is not legal in the `Position`, or if the input SAN does not match exactly one legal move.
    InvalidMove,
    /// Returned by `Game::play` when the game has already ended.
    GameOver,
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Error::InvalidFenString(fen_string) => format!("invalid fen string: {fen_string}"),
            Error::InvalidMove => "invalid move".to_string(),
            Error::GameOver => "game is over".to_string(),
        }
        .fmt(f)
    }
}

// Public piece types

/// A side in the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Color {
    /// White, who moves first.
    White,
    /// Black, who moves second.
    Black,
}

impl Color {
    /// Returns the other side.
    #[must_use]
    pub const fn opponent(self) -> Self {
        match self {
            Color::White => Color::Black,
            Color::Black => Color::White,
        }
    }

    const fn from_bit(bit: u8) -> Self {
        if bit == WHITE_MOVE {
            Color::White
        } else {
            Color::Black
        }
    }
}

/// Represents each chess piece kind, independent of its color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PieceKind {
    /// A pawn. Moves forward and captures diagonally. Promotes once it reaches the last rank.
    Pawn,
    /// A knight. Moves in an L shape.
    Knight,
    /// A bishop. Moves diagonally, any distance.
    Bishop,
    /// A rook. Moves orthogonally, any distance.
    Rook,
    /// A queen. Moves diagonally and orthogonally, any distance.
    Queen,
    /// A king. Moves one square in any direction, or castles.
    King,
}

/// A piece on the chess board.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Piece {
    /// The piece color.
    pub color: Color,
    /// The piece kind.
    pub kind: PieceKind,
}

impl Piece {
    fn from_bits(bits: u8) -> Option<Self> {
        let kind = match piece_type(bits) {
            PAWN_TYPE => PieceKind::Pawn,
            KNIGHT_TYPE => PieceKind::Knight,
            BISHOP_TYPE => PieceKind::Bishop,
            ROOK_TYPE => PieceKind::Rook,
            QUEEN_TYPE => PieceKind::Queen,
            KING_TYPE => PieceKind::King,
            _ => return None,
        };
        Some(Self {
            color: Color::from_bit(piece_color(bits)),
            kind,
        })
    }

    /// FEN letter for this piece: uppercase for white, lowercase for black.
    pub const fn symbol(self) -> char {
        let ch = match self.kind {
            PieceKind::Pawn => 'p',
            PieceKind::Knight => 'n',
            PieceKind::Bishop => 'b',
            PieceKind::Rook => 'r',
            PieceKind::Queen => 'q',
            PieceKind::King => 'k',
        };
        match self.color {
            Color::White => ch.to_ascii_uppercase(),
            Color::Black => ch,
        }
    }
}

// Game status

/// Whether the game is still in progress, and if not, how it ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// The game is ongoing.
    Ongoing,
    /// Game ended by checkmate.
    Checkmate {
        /// The side that delivered mate.
        winner: Color,
    },
    /// Game ended by draw.
    Draw(DrawReason),
}

/// Why a game ended in a draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawReason {
    /// The side to move has no legal moves and is not in check.
    Stalemate,
    /// Neither side has enough material to checkmate.
    InsufficientMaterial,
    /// Applied automatically after 100 plies without a pawn move or capture.
    FiftyMoveRule,
    /// Applied automatically on the third occurrence of the same position. Only detected by
    /// [`Game`].
    ThreefoldRepetition,
}

/// A chess position: piece placement, side to move, castling rights, en passant square, and
/// the halfmove and fullmove counters. Parse one from FEN with `str::parse`; write one with
/// [`Position::to_fen`].
#[derive(Debug, Clone, Copy)]
pub struct Position(
    /// Packed into 36 bytes.
    ///
    /// Bytes 0..=31: the board, two squares per byte. Square index is `rank * 8 + file`
    /// (a1 = 0, h8 = 63). Even squares use the low nibble, odd squares the high nibble.
    /// A nibble is `[color:1][type:3]`, as in the `WHITE_*` and `BLACK_*` constants; 0 is empty.
    ///
    /// Byte 32: `[castling:4][has_en_passant:1][en_passant_file:3]`. Castling uses the `*_CASTLE` masks.
    /// Byte 33: `[active_player:1][halfmove_cnt:7]`. The clock saturates at 127.
    /// Bytes 34..=35: `fullmove_cnt` as a little-endian `u16`.
    [u8; 36],
);

impl Position {
    /// The standard starting position.
    pub fn new() -> Self {
        Self::default()
    }

    /// Plays `m` if it is legal in this position.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidMove`] if `m` is not legal in this position.
    pub fn apply_move(&mut self, m: Move) -> Result<(), Error> {
        if !self.legal_moves_from(Square(m.src())).contains(&m) {
            return Err(Error::InvalidMove);
        }
        self.apply_move_unchecked(m);
        Ok(())
    }

    /// The piece on `sq`, if any.
    pub fn piece_at(&self, sq: Square) -> Option<Piece> {
        Piece::from_bits(self.square_bits(sq))
    }

    /// The active player.
    pub fn side_to_move(&self) -> Color {
        Color::from_bit(self.active_player_bit())
    }

    /// All legal moves for the side to move.
    pub fn legal_moves(&self) -> Vec<Move> {
        // Enough for almost any position, so the list rarely has to grow.
        let mut moves = Vec::with_capacity(64);
        self.generate_legal_moves(!0, &mut moves);
        moves
    }

    /// Is the side to move in check?
    pub fn is_check(&self) -> bool {
        let active_player = self.active_player_bit();
        self.is_attacked_by(self.king_square(active_player), active_player ^ 1)
    }

    /// Result of the position on its own. Threefold repetition needs move history, so it is
    /// only reported by [`Game::status`].
    pub fn status(&self) -> Status {
        if !self.has_legal_move() {
            return if self.is_check() {
                Status::Checkmate {
                    winner: self.side_to_move().opponent(),
                }
            } else {
                Status::Draw(DrawReason::Stalemate)
            };
        }
        if self.is_insufficient_material() {
            return Status::Draw(DrawReason::InsufficientMaterial);
        }
        if self.halfmove_cnt() >= FIFTY_MOVE_RULE_PLIES {
            return Status::Draw(DrawReason::FiftyMoveRule);
        }
        Status::Ongoing
    }

    /// Returns true if the active player has any legal moves.
    fn has_legal_move(&self) -> bool {
        !self.legal_moves().is_empty()
    }

    /// Neither side can possibly checkmate: K vs K, K+minor vs K, or only bishops that all
    /// stand on the same square color.
    fn is_insufficient_material(&self) -> bool {
        let mut knights = 0;
        let mut bishop_square_colors = [false; 2];
        for sq in 0..SQUARE_CNT as u8 {
            match piece_type(self.square_bits(Square(sq))) {
                EMPTY | KING_TYPE => {}
                KNIGHT_TYPE => knights += 1,
                BISHOP_TYPE => {
                    bishop_square_colors[((sq_rank(sq) + sq_file(sq)) % 2) as usize] = true;
                }
                _ => return false,
            }
        }
        let has_bishops = bishop_square_colors[0] || bishop_square_colors[1];
        let bishops_mixed = bishop_square_colors[0] && bishop_square_colors[1];
        match knights {
            0 => !bishops_mixed,
            1 => !has_bishops,
            _ => false,
        }
    }

    /// Identity of the position for repetition: placement, side to move, castling rights, and
    /// the en passant file only when an en passant capture is actually legal.
    fn repetition_key(&self) -> [u8; 34] {
        let mut key = [0u8; 34];
        key[..32].copy_from_slice(&self.0[..32]);
        let ep_bits = match self.en_passant_target(self.active_player_bit()) {
            Some(ep_sq)
                if self.legal_moves().iter().any(|m| {
                    m.dst() == ep_sq && piece_type(self.square_bits(Square(m.src()))) == PAWN_TYPE
                }) =>
            {
                self.en_passant_bits()
            }
            _ => 0,
        };
        key[32] = (self.castle_bits() << 4) | ep_bits;
        key[33] = self.active_player_bit();
        key
    }

    /// This position as a FEN string.
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

    /// Parses SAN such as `Nf3`, `exd5`, or `O-O` into a legal [`Move`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidMove`] if `san` does not parse, matches no legal move or more
    /// than one, or has a wrong `+` or `#` suffix.
    pub fn parse_san(&self, san: &str) -> Result<Move, Error> {
        let san = san.trim();
        if san.is_empty() {
            return Err(Error::InvalidMove);
        }

        // Separate optional check/mate suffix (at most one)
        let (core, suffix) = if let Some(rest) = san.strip_suffix('#') {
            (rest, Some('#'))
        } else if let Some(rest) = san.strip_suffix('+') {
            (rest, Some('+'))
        } else {
            (san, None)
        };

        let m = self.parse_san_core(core)?;
        self.validate_san_suffix(m, suffix)?;
        Ok(m)
    }

    /// A missing suffix is accepted; a present one must match the resulting position.
    fn validate_san_suffix(&self, m: Move, suffix: Option<char>) -> Result<(), Error> {
        if suffix.is_none() {
            return Ok(());
        }
        if suffix == self.check_suffix(m) {
            Ok(())
        } else {
            Err(Error::InvalidMove)
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "a single-pass parser; splitting it scatters the grammar"
    )]
    fn parse_san_core(&self, san: &str) -> Result<Move, Error> {
        // Castling
        if san == "O-O" || san == "0-0" {
            let king_dst = if self.active_player_bit() == WHITE_MOVE {
                62
            } else {
                6
            };
            return self.find_move(KING_TYPE, king_dst, None, None, 0, false);
        }
        if san == "O-O-O" || san == "0-0-0" {
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
            Some(&ch) if ch.is_ascii_uppercase() => {
                return Err(Error::InvalidMove);
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
            if let Some(&ch) = bytes.get(idx)
                && (b'a'..=b'h').contains(&ch)
                && let Some(&next_ch) = bytes.get(idx + 1)
                && next_ch == b'x'
            {
                disambig_file = Some((ch - b'a') as i8);
                idx += 1;
            }

            // Check for capture
            if let Some(&ch) = bytes.get(idx)
                && ch == b'x'
            {
                is_capture = true;
                idx += 1;
            }

            // Destination square
            if bytes.len().saturating_sub(idx) < 2 {
                return Err(Error::InvalidMove);
            }
            if !(b'a'..=b'h').contains(&bytes[idx]) || !(b'1'..=b'8').contains(&bytes[idx + 1]) {
                return Err(Error::InvalidMove);
            }
            let dst_file = (bytes[idx] - b'a') as i8;
            let dst_rank = (bytes[idx + 1] - b'1') as i8;
            dst = rank_file_to_sq(7 - dst_rank, dst_file);
            idx += 2;

            // Pawn captures must include the origin file letter (e.g. exd5, not xd5)
            if is_capture && disambig_file.is_none() {
                return Err(Error::InvalidMove);
            }
        } else {
            // For pieces: [KQRBN][file]?[rank]?x?[file][rank](=[piece])?
            // First, check for capture
            if let Some(&ch) = bytes.get(idx)
                && ch == b'x'
            {
                is_capture = true;
                idx += 1;
            }

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
                } else if ch == b'x' {
                    is_capture = true;
                } else {
                    return Err(Error::InvalidMove);
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
            idx += 1;
        }

        // Reject trailing junk (e.g. "e4e5", "e4!!")
        if idx != bytes.len() {
            return Err(Error::InvalidMove);
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

        for m in self.legal_moves() {
            if m.dst() != dst || m.promo() != promo {
                continue;
            }
            let src = m.src();
            let src_bits = self.square_bits(Square(src));
            if piece_type(src_bits) != pt {
                continue;
            }
            if let Some(df) = disambig_file
                && sq_file(src) != df
            {
                continue;
            }
            if let Some(dr) = disambig_rank
                && sq_rank(src) != dr
            {
                continue;
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

    /// Converts a [`Move`] to a SAN string. If the move is illegal in this position, the output
    /// will be incorrect.
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
                let dst_bits = self.square_bits(Square(dst));
                if dst_bits != 0 {
                    result.push('x');
                }
                let dst_file = sq_file(dst);
                let dst_rank = sq_rank(dst);
                result.push((b'a' + dst_file as u8) as char);
                result.push((b'1' + (7 - dst_rank) as u8) as char);
            }
        } else if pt == PAWN_TYPE {
            // Pawn move
            let dst_bits = self.square_bits(Square(dst));
            let is_capture = dst_bits != 0 || self.en_passant_target(active_player) == Some(dst);
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
            for other_m in self.legal_moves() {
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

        if let Some(suffix) = self.check_suffix(*m) {
            result.push(suffix);
        }

        result
    }

    /// `+` if `m` gives check, `#` if it gives checkmate.
    fn check_suffix(&self, m: Move) -> Option<char> {
        let mut after = *self;
        after.apply_move_unchecked(m);
        if !after.is_check() {
            None
        } else if after.has_legal_move() {
            Some('+')
        } else {
            Some('#')
        }
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

    fn king_square(&self, color: u8) -> u8 {
        let king = KING_TYPE | (color << 3);
        for (i, &pair) in self.0[..32].iter().enumerate() {
            if pair & 0b1111 == king {
                return 2 * i as u8;
            }
            if pair >> 4 == king {
                return 2 * i as u8 + 1;
            }
        }
        panic!("no king found for color {color}");
    }

    fn set_square(&mut self, s: Square, p: u8) {
        let square_idx = (s.0 >> 1) as usize;
        assert!(square_idx <= 31, "invalid square");
        if s.0.is_multiple_of(2) {
            self.0[square_idx] = (self.0[square_idx] & 0b1111_0000) | p;
        } else {
            self.0[square_idx] = (self.0[square_idx] & 0b0000_1111) | p << 4;
        }
    }

    fn square_bits(&self, s: Square) -> u8 {
        // Even squares are the low bits (0b1111), odd squares are the high bits(0b1111_0000).
        let square_idx = (s.0 >> 1) as usize;
        assert!(square_idx <= 31, "invalid square");
        if s.0.is_multiple_of(2) {
            self.0[square_idx] & 0b1111
        } else {
            self.0[square_idx] >> 4
        }
    }

    fn en_passant_bits(&self) -> u8 {
        self.0[32] & 0b1111
    }

    fn castle_bits(&self) -> u8 {
        (self.0[32] & 0b1111_0000) >> 4
    }

    fn active_player_bit(&self) -> u8 {
        self.0[33] >> 7
    }

    fn halfmove_cnt(&self) -> u8 {
        self.0[33] & 0b0111_1111
    }

    fn fullmove_cnt(&self) -> u16 {
        u16::from_le_bytes([self.0[34], self.0[35]])
    }

    fn inc_halfmove_cnt(&mut self) {
        // Saturate so the clock can never carry into the active player bit.
        let cnt = self.halfmove_cnt();
        if cnt < MAX_HALFMOVE_CNT {
            self.set_halfmove_cnt(cnt + 1);
        }
    }

    fn inc_fullmove_cnt(&mut self) {
        self.set_fullmove_cnt(self.fullmove_cnt().saturating_add(1));
    }

    fn set_castle_bits(&mut self, bits: u8) {
        self.0[32] = (self.0[32] & 0b0000_1111) | (bits << 4);
    }

    fn set_en_passant(&mut self, file: u8, has_ep: bool) {
        let ep_bits = if has_ep { 0b1000 | file } else { 0 };
        self.0[32] = (self.0[32] & 0b1111_0000) | ep_bits;
    }

    fn set_active_player(&mut self, player: u8) {
        self.0[33] = (self.0[33] & 0b0111_1111) | (player << 7);
    }

    fn set_halfmove_cnt(&mut self, cnt: u8) {
        self.0[33] = (self.0[33] & 0b1000_0000) | (cnt & 0b0111_1111);
    }

    fn set_fullmove_cnt(&mut self, cnt: u16) {
        [self.0[34], self.0[35]] = cnt.to_le_bytes();
    }

    /// Bitboard of all occupied squares (both colors).
    fn occupancy(&self) -> u64 {
        let [white, black] = self.occupancy_by_color();
        white | black
    }

    /// Bitboards of the squares occupied by white and by black, from one pass over the board.
    fn occupancy_by_color(&self) -> [u64; 2] {
        let mut occ = [0u64; 2];
        for (i, &pair) in self.0[..32].iter().enumerate() {
            // Each byte holds two squares: the even one in the low nibble.
            for (sq, bits) in [(2 * i, pair & 0b1111), (2 * i + 1, pair >> 4)] {
                if bits != EMPTY {
                    occ[usize::from(bits >> 3)] |= 1u64 << sq;
                }
            }
        }
        occ
    }

    /// Is `sq` attacked by `color` (0=white, 1=black)?
    fn is_attacked_by(&self, sq: u8, color: u8) -> bool {
        self.is_attacked_by_with(sq, color, self.occupancy())
    }

    /// [`Self::is_attacked_by`] with the occupancy bitboard already known.
    fn is_attacked_by_with(&self, sq: u8, color: u8, occ: u64) -> bool {
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
        if (0..8).contains(&pr) {
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

    /// Legal moves for the piece on `s`. Empty if the square is empty or holds an opponent's piece.
    pub fn legal_moves_from(&self, s: Square) -> Vec<Move> {
        let mut moves = Vec::new();
        self.generate_legal_moves(1u64 << s.0, &mut moves);
        moves
    }

    /// Appends the legal moves of the side to move's pieces standing on `from` (a bitboard) to
    /// `moves`. Reusing one list across calls avoids allocating for every position.
    pub(crate) fn generate_legal_moves(&self, from: u64, moves: &mut Vec<Move>) {
        let active_player = self.active_player_bit();
        let by_color = self.occupancy_by_color();
        let friendly = by_color[usize::from(active_player)];
        let opponent = by_color[usize::from(active_player ^ 1)];
        let occ = friendly | opponent;

        let first = moves.len();
        for src in bitboard_iter(friendly & from) {
            self.pseudo_legal_moves(src, occ, friendly, opponent, moves);
        }

        // Only keep moves that don't leave the king in check, preserving their order.
        let king_sq = self.king_square(active_player);
        let ep_target = self.en_passant_target(active_player);
        let in_check = self.is_attacked_by_with(king_sq, active_player ^ 1, occ);
        // When not in check, only the king itself, a piece on a line through the king, or an
        // en passant capture (which also empties the captured pawn's square) can expose it.
        let king_lines = QUEEN_RAYS[king_sq as usize];
        let mut kept = first;
        for i in first..moves.len() {
            let m = moves[i];
            let src = m.src();
            let en_passant = ep_target == Some(m.dst())
                && piece_type(self.square_bits(Square(src))) == PAWN_TYPE;
            let cannot_expose_king =
                !in_check && src != king_sq && king_lines & (1u64 << src) == 0 && !en_passant;
            if cannot_expose_king || !self.leaves_king_attacked(m, king_sq, occ, ep_target) {
                moves[kept] = m;
                kept += 1;
            }
        }
        moves.truncate(kept);
    }

    /// Moves for the piece on `src` that follow its movement rules, ignoring whether they
    /// leave the king in check.
    fn pseudo_legal_moves(
        &self,
        src: u8,
        occ: u64,
        friendly: u64,
        opponent: u64,
        moves: &mut Vec<Move>,
    ) {
        let active_player = self.active_player_bit();
        let pt = piece_type(self.square_bits(Square(src)));

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
                self.castling_moves(active_player, occ, moves);
            }
            PAWN_TYPE => {
                let (pushes, captures) = if active_player == 0 {
                    (
                        PAWN_PUSHES_WHITE[src as usize],
                        PAWN_ATTACKS_WHITE[src as usize],
                    )
                } else {
                    (
                        flip_rank_bb(PAWN_PUSHES_WHITE[flip_rank_sq(src) as usize]),
                        flip_rank_bb(PAWN_ATTACKS_WHITE[flip_rank_sq(src) as usize]),
                    )
                };

                // Forward pushes (only if destination is empty)
                for dst in bitboard_iter(pushes & !occ) {
                    // Double push requires the intermediate square empty
                    if (src as i8 - dst as i8).abs() == 16 {
                        let mid = u16::midpoint(u16::from(src), u16::from(dst)) as u8;
                        if occ & (1u64 << mid) != 0 {
                            continue;
                        }
                    }
                    if is_promotion_rank(dst, active_player) {
                        // Generate promotion moves
                        for promo in [QUEEN_TYPE, ROOK_TYPE, BISHOP_TYPE, KNIGHT_TYPE] {
                            moves.push(Move::new(promo, src, dst));
                        }
                    } else {
                        moves.push(Move::new(0, src, dst));
                    }
                }

                // En passant capture — only if the pawn is on the correct rank
                // White captures from rank 3 (ep target on rank 2)
                // Black captures from rank 4 (ep target on rank 5)
                if let Some(ep_sq) = self.en_passant_target(active_player) {
                    let (correct_src_rank, correct_ep_rank) =
                        if active_player == 0 { (3, 2) } else { (4, 5) };
                    if sq_rank(src) == correct_src_rank
                        && sq_rank(ep_sq) == correct_ep_rank
                        && captures & (1u64 << ep_sq) != 0
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
    }

    /// Would the pseudo-legal move `m` leave the mover's king, now on `king_sq`, in check?
    ///
    /// `occ` is the occupancy before the move; the occupancy after it is derived from that
    /// rather than rescanning the board.
    fn leaves_king_attacked(&self, m: Move, king_sq: u8, occ: u64, ep_target: Option<u8>) -> bool {
        let (src, dst) = (m.src(), m.dst());
        let mut after = *self;
        after.apply_move_unchecked(m);

        let mover = piece_type(self.square_bits(Square(src)));
        let mut occ_after = (occ & !(1u64 << src)) | (1u64 << dst);
        if mover == PAWN_TYPE && ep_target == Some(dst) {
            // The pawn taken en passant stands beside the mover, not on `dst`.
            let captured = if self.active_player_bit() == 0 {
                dst + 8
            } else {
                dst - 8
            };
            occ_after &= !(1u64 << captured);
        }
        if mover == KING_TYPE && src.abs_diff(dst) == 2 {
            // Castling moves the rook too.
            occ_after = after.occupancy();
        }

        let king = if src == king_sq { dst } else { king_sq };
        after.is_attacked_by_with(king, self.active_player_bit() ^ 1, occ_after)
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

    fn castling_moves(&self, active_player: u8, occ: u64, moves: &mut Vec<Move>) {
        let castle = self.castle_bits();

        if active_player == 0 {
            // White: king on e1 (square 60)
            let king_sq = 60u8;
            // Kingside (K): king e1->g1, squares f1/g1 must be empty, king not in check
            if castle & WHITE_KINGSIDE_CASTLE != 0 {
                let between = (1u64 << 61) | (1u64 << 62); // f1, g1
                if occ & between == 0
                    && !self.is_attacked_by_with(king_sq, 1, occ)
                    && !self.is_attacked_by_with(61, 1, occ) // f1
                    && !self.is_attacked_by_with(62, 1, occ)
                // g1
                {
                    moves.push(Move::new(0, king_sq, 62));
                }
            }
            // Queenside (Q): king e1->c1, squares b1/c1/d1 must be empty
            if castle & WHITE_QUEENSIDE_CASTLE != 0 {
                let between = (1u64 << 57) | (1u64 << 58) | (1u64 << 59); // b1, c1, d1
                if occ & between == 0
                    && !self.is_attacked_by_with(king_sq, 1, occ)
                    && !self.is_attacked_by_with(59, 1, occ) // d1
                    && !self.is_attacked_by_with(58, 1, occ)
                // c1
                {
                    moves.push(Move::new(0, king_sq, 58));
                }
            }
        } else {
            // Black: king on e8 (square 4)
            let king_sq = 4u8;
            // Kingside (k): king e8->g8
            if castle & BLACK_KINGSIDE_CASTLE != 0 {
                let between = (1u64 << 5) | (1u64 << 6); // f8, g8
                if occ & between == 0
                    && !self.is_attacked_by_with(king_sq, 0, occ)
                    && !self.is_attacked_by_with(5, 0, occ) // f8
                    && !self.is_attacked_by_with(6, 0, occ)
                // g8
                {
                    moves.push(Move::new(0, king_sq, 6));
                }
            }
            // Queenside (q): king e8->c8
            if castle & BLACK_QUEENSIDE_CASTLE != 0 {
                let between = (1u64 << 1) | (1u64 << 2) | (1u64 << 3); // b8, c8, d8
                if occ & between == 0
                    && !self.is_attacked_by_with(king_sq, 0, occ)
                    && !self.is_attacked_by_with(3, 0, occ)
                    && !self.is_attacked_by_with(2, 0, occ)
                {
                    moves.push(Move::new(0, king_sq, 2));
                }
            }
        }
    }
}

fn is_promotion_rank(sq: u8, color: u8) -> bool {
    let rank = sq_rank(sq);
    (color == 0 && rank == 0) || (color == 1 && rank == 7)
}

/// Starting FEN: rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w `KQkq` - 0 1
impl Default for Position {
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
            // fullmove_cnt = 1 (little endian)
            1,
            0,
        ])
    }
}

impl FromStr for Position {
    type Err = Error;

    #[expect(
        clippy::too_many_lines,
        reason = "a single-pass parser; splitting it scatters the grammar"
    )]
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
                if let Some(empty) = ch.to_digit(10) {
                    file += empty as u8;
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
            let Some(rank_num) = rank_ch.to_digit(10) else {
                return Err(Error::InvalidFenString(s.to_string()));
            };
            let rank_num = rank_num as u8;
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
            .parse::<u16>()
            .map_err(|_| Error::InvalidFenString(s.to_string()))?;
        if fullmove == 0 {
            return Err(Error::InvalidFenString(s.to_string()));
        }

        // Build the position
        let mut fen = Position([0u8; 36]);

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
/// Move[4..=9] = src square.
/// Move[10..=15] = dst square.
///
/// Total = 16 bits.
///
/// Technically, only 1,968 possible moves, which could take up 11 bits.
///
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Move(u16);

impl Move {
    /// Create a new move.
    /// `promo_type`: piece type for promotion (0 = none), values match piece type encoding.
    /// `src`: source square index (0-63).
    /// `dst`: destination square index (0-63).
    const fn new(promo_type: u8, src: u8, dst: u8) -> Self {
        Self((promo_type as u16) | (src as u16) << 4 | (dst as u16) << 10)
    }

    /// Source square index (0-63).
    pub const fn src(self) -> u8 {
        ((self.0 >> 4) & 0b0011_1111) as u8
    }

    /// Destination square index (0-63).
    pub const fn dst(self) -> u8 {
        ((self.0 >> 10) & 0b0011_1111) as u8
    }

    /// Promotion piece type (0 = no promotion).
    pub const fn promo(self) -> u8 {
        (self.0 & 0b0000_1111) as u8
    }

    /// The piece a pawn promotes to, if this is a promotion.
    pub fn promotion(self) -> Option<PieceKind> {
        Piece::from_bits(self.promo()).map(|p| p.kind)
    }
}

fn parse_square(file_ch: u8, rank_ch: u8) -> Result<u8, &'static str> {
    if !(b'a'..=b'h').contains(&file_ch) {
        return Err("file must be a-h");
    }
    if !(b'1'..=b'8').contains(&rank_ch) {
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

/// Count the number of leaf nodes in the move tree at the given depth.
///
/// Perft(0) = 1, Perft(1) = number of legal moves, Perft(2) = number of
/// legal move pairs, etc.
pub fn perft(position: &Position, depth: u32) -> u64 {
    if depth == 0 {
        return 1;
    }
    let moves = position.legal_moves();
    // Every legal move is one leaf; no need to play them out.
    if depth == 1 {
        return moves.len() as u64;
    }
    moves
        .into_iter()
        .map(|m| {
            let mut next = *position;
            next.apply_move_unchecked(m);
            perft(&next, depth - 1)
        })
        .sum()
}

/// A square on the chess board, indexed 0-63 (a8=0, h1=63).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Square(pub u8);

/// A game in progress: the current position plus the history needed for repetition.
#[derive(Debug, Clone)]
pub struct Game {
    /// The current position.
    position: Position,
    /// Moves played since the starting position.
    moves: Vec<Move>,
    /// Repetition key of every position reached, including the starting one.
    history: Vec<[u8; 34]>,
}

impl Game {
    /// New game from the default starting position.
    pub fn new() -> Self {
        Self::from_position(Position::default())
    }

    /// New game from the given position.
    pub fn from_position(position: Position) -> Self {
        Self {
            position,
            moves: vec![],
            history: vec![position.repetition_key()],
        }
    }

    /// The current position.
    pub fn position(&self) -> &Position {
        &self.position
    }

    /// Moves played since the starting position.
    pub fn moves(&self) -> &[Move] {
        &self.moves
    }

    /// Repetition keys of every position reached, oldest first; the last is the current one.
    pub(crate) fn history(&self) -> &[[u8; 34]] {
        &self.history
    }

    /// Plays `m` if the game is still ongoing and the move is legal.
    ///
    /// # Errors
    ///
    /// Returns [`Error::GameOver`] if the game already ended and [`Error::InvalidMove`] if `m` is
    /// illegal.
    pub fn play(&mut self, m: Move) -> Result<(), Error> {
        if self.status() != Status::Ongoing {
            return Err(Error::GameOver);
        }
        self.position.apply_move(m)?;
        self.moves.push(m);
        self.history.push(self.position.repetition_key());
        Ok(())
    }

    /// Like [`Position::status`], but also detects threefold repetition from the game's
    /// history.
    pub fn status(&self) -> Status {
        let status = self.position.status();
        if status != Status::Ongoing {
            return status;
        }
        let current = self.position.repetition_key();
        if self.history.iter().filter(|key| **key == current).count() >= 3 {
            return Status::Draw(DrawReason::ThreefoldRepetition);
        }
        Status::Ongoing
    }
}

impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_size() {
        assert_eq!(std::mem::size_of::<Position>(), 36);
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
        let fen = Position::default();
        let moves = fen.legal_moves();
        assert_eq!(
            moves.len(),
            20,
            "starting position should have 20 legal moves"
        );
    }

    #[test]
    fn starting_position_pawn_moves() {
        let fen = Position::default();
        // White to move: 16 pawn pushes + 4 knight moves = 20
        let pawn_moves: Vec<_> = (0..64)
            .flat_map(|sq| fen.legal_moves_from(Square(sq)))
            .filter(|m| {
                let bits = fen.square_bits(Square(m.src()));
                piece_type(bits) == PAWN_TYPE
            })
            .collect();
        assert_eq!(pawn_moves.len(), 16, "should have 16 pawn pushes");
    }

    #[test]
    fn starting_position_knight_moves() {
        let fen = Position::default();
        let knight_moves: Vec<_> = (0..64)
            .flat_map(|sq| fen.legal_moves_from(Square(sq)))
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

    fn empty_fen() -> Position {
        let mut fen = Position::new();
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
        fen.apply_move(m).unwrap();

        assert_eq!(fen.square_bits(Square(4)), WHITE_QUEEN); // e8
        assert_eq!(fen.square_bits(Square(12)), EMPTY); // e7
        assert_eq!(fen.active_player_bit(), BLACK_MOVE);
    }

    #[test]
    fn move_promotion_kind() {
        assert_eq!(
            "e7-e8-q".parse::<Move>().unwrap().promotion(),
            Some(PieceKind::Queen)
        );
        assert_eq!(
            "e7-e8-n".parse::<Move>().unwrap().promotion(),
            Some(PieceKind::Knight)
        );
        assert_eq!("e2-e4".parse::<Move>().unwrap().promotion(), None);
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
        fen.apply_move(m1).unwrap();

        // White plays e5xd6 (en passant)
        let m2: Move = "e5-d6".parse().unwrap();
        fen.apply_move(m2).unwrap();

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
        fen.apply_move(m).unwrap();

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
        fen.apply_move(m).unwrap();

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
        fen.apply_move(m).unwrap();

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
        fen.apply_move(m).unwrap();

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
        fen.apply_move(m).unwrap();

        assert_eq!(fen.halfmove_cnt(), 0);
    }

    #[test]
    fn halfmove_increment_on_quiet_move() {
        let mut fen = empty_fen();
        fen.set_square(Square(60), WHITE_KING); // e1
        fen.set_square(Square(4), BLACK_KING); // e8

        let m: Move = "e1-e2".parse().unwrap();
        fen.apply_move(m).unwrap();

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
        fen.apply_move(m1).unwrap();
        assert_eq!(fen.fullmove_cnt(), 1); // Not incremented yet

        // Black moves
        let m2: Move = "e8-e7".parse().unwrap();
        fen.apply_move(m2).unwrap();
        assert_eq!(fen.fullmove_cnt(), 2); // Incremented after black
    }

    #[test]
    fn en_passant_target_set_on_double_push() {
        let mut fen = empty_fen();
        fen.set_square(Square(52), WHITE_PAWN); // e2
        fen.set_square(Square(60), WHITE_KING); // e1
        fen.set_square(Square(4), BLACK_KING); // e8

        let m: Move = "e2-e4".parse().unwrap();
        fen.apply_move(m).unwrap();

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
        fen.apply_move(m1).unwrap();
        assert_eq!(fen.en_passant_target(BLACK_MOVE), Some(44)); // e3

        // Black plays d7-d6 (should clear ep target)
        let m2: Move = "d7-d6".parse().unwrap();
        fen.apply_move(m2).unwrap();
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

        let moves = fen.legal_moves_from(Square(52));
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

        let moves = fen.legal_moves_from(Square(60));
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
        fen.apply_move(m1).unwrap();

        // White's e5 pawn is pinned — en passant capture would expose king
        let moves = fen.legal_moves_from(Square(28));
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
        let fen: Position = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
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
        let fen: Position = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1"
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
        let fen: Position = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
            .parse()
            .unwrap();
        assert_eq!(
            fen.to_fen(),
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
        );
    }

    #[test]
    fn fen_roundtrip_midgame() {
        let fen: Position = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1"
            .parse()
            .unwrap();
        assert_eq!(
            fen.to_fen(),
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1"
        );
    }

    #[test]
    fn fen_roundtrip_no_castling() {
        let fen: Position = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w - - 0 1"
            .parse()
            .unwrap();
        assert_eq!(
            fen.to_fen(),
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w - - 0 1"
        );
    }

    #[test]
    fn fen_roundtrip_black_to_move() {
        let fen: Position = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR b KQkq - 0 1"
            .parse()
            .unwrap();
        assert_eq!(
            fen.to_fen(),
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR b KQkq - 0 1"
        );
    }

    #[test]
    fn fen_roundtrip_en_passant() {
        let fen: Position = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1"
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
                .parse::<Position>()
                .is_err()
        );
        // Invalid piece letter
        assert!(
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNX w KQkq - 0 1"
                .parse::<Position>()
                .is_err()
        );
        // Invalid active color
        assert!(
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR x KQkq - 0 1"
                .parse::<Position>()
                .is_err()
        );
        // Invalid castling
        assert!(
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkqX - 0 1"
                .parse::<Position>()
                .is_err()
        );
        // Invalid en passant
        assert!(
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq e9 0 1"
                .parse::<Position>()
                .is_err()
        );
        // Invalid halfmove
        assert!(
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - x 1"
                .parse::<Position>()
                .is_err()
        );
        // Invalid fullmove
        assert!(
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 0"
                .parse::<Position>()
                .is_err()
        );
        // Two white kings
        assert!(
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNK w KQkq - 0 1"
                .parse::<Position>()
                .is_err()
        );
        // Pawn on rank 8
        assert!(
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNP w KQkq - 0 1"
                .parse::<Position>()
                .is_err()
        );
    }

    // Phase 4 tests

    #[test]
    fn san_parse_quiet_move() {
        let fen: Position = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
            .parse()
            .unwrap();
        let m = fen.parse_san("e4").unwrap();
        assert_eq!(m.src(), 52); // e2
        assert_eq!(m.dst(), 36); // e4
    }

    #[test]
    fn san_parse_capture() {
        let fen: Position = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
            .parse()
            .unwrap();
        let m = fen.parse_san("Nf3").unwrap();
        assert_eq!(m.src(), 62); // g1
        assert_eq!(m.dst(), 45); // f3
    }

    #[test]
    fn san_parse_pawn_capture() {
        let fen: Position = "rnbqkbnr/pppppppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1"
            .parse()
            .unwrap();
        let m = fen.parse_san("dxe4").unwrap();
        assert_eq!(m.src(), 27); // d5
        assert_eq!(m.dst(), 36); // e4
    }

    #[test]
    fn san_parse_promotion() {
        let fen: Position = "8/P7/8/8/8/8/8/k6K w - - 0 1".parse().unwrap();
        let m = fen.parse_san("a8=Q+").unwrap();
        assert_eq!(m.src(), 8); // a7
        assert_eq!(m.dst(), 0); // a8
        assert_eq!(m.promo(), QUEEN_TYPE);
    }

    #[test]
    fn san_parse_castling_kingside() {
        let fen: Position = "r3k2r/pppppppp/8/8/8/8/PPPPPPPP/R3K2R w KQkq - 0 1"
            .parse()
            .unwrap();
        let m = fen.parse_san("O-O").unwrap();
        assert_eq!(m.src(), 60); // e1
        assert_eq!(m.dst(), 62); // g1
    }

    #[test]
    fn san_parse_castling_queenside() {
        let fen: Position = "r3k2r/pppppppp/8/8/8/8/PPPPPPPP/R3K2R w KQkq - 0 1"
            .parse()
            .unwrap();
        let m = fen.parse_san("O-O-O").unwrap();
        assert_eq!(m.src(), 60); // e1
        assert_eq!(m.dst(), 58); // c1
    }

    #[test]
    fn san_parse_check() {
        // Position where Qh5+ is check (e2/f2 pawns moved out of the way, f7 pawn removed)
        let fen: Position = "rnbqkbnr/pppp2pp/8/8/8/8/PPPP2PP/RNBQKBNR w KQkq - 0 1"
            .parse()
            .unwrap();
        let m = fen.parse_san("Qh5+").unwrap();
        assert_eq!(m.src(), 59); // d1
        assert_eq!(m.dst(), 31); // h5
    }

    #[test]
    fn san_parse_knight_disambiguation() {
        // Two knights can move to f3: g1 and e5
        let fen: Position = "8/8/8/4N3/8/8/5N2/k6K w - - 0 1".parse().unwrap();
        let m = fen.parse_san("Nef3").unwrap();
        assert_eq!(m.src(), 28); // e5
        assert_eq!(m.dst(), 45); // f3
    }

    #[test]
    fn san_parse_rook_disambiguation_by_rank() {
        // Two rooks can move to d3: d1 and d5
        let fen: Position = "7k/8/8/3R4/8/8/8/3R3K w - - 0 1".parse().unwrap();
        let m = fen.parse_san("R1d3").unwrap();
        assert_eq!(m.src(), 59); // d1
        assert_eq!(m.dst(), 43); // d3
    }

    #[test]
    fn san_format_quiet_move() {
        let fen: Position = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
            .parse()
            .unwrap();
        let m = fen.parse_san("e4").unwrap();
        assert_eq!(fen.to_san(&m), "e4");
    }

    #[test]
    fn san_format_capture() {
        let fen: Position = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
            .parse()
            .unwrap();
        let m = fen.parse_san("Nf3").unwrap();
        assert_eq!(fen.to_san(&m), "Nf3");
    }

    #[test]
    fn san_format_promotion() {
        // Queen on a8 attacks black king on a1 (same file) — this IS check
        let fen: Position = "8/P7/8/8/8/8/8/k6K w - - 0 1".parse().unwrap();
        let m = fen.parse_san("a8=Q+").unwrap();
        assert_eq!(fen.to_san(&m), "a8=Q+");
    }

    #[test]
    fn san_format_castling() {
        let fen: Position = "r3k2r/pppppppp/8/8/8/8/PPPPPPPP/R3K2R w KQkq - 0 1"
            .parse()
            .unwrap();
        let m = fen.parse_san("O-O").unwrap();
        assert_eq!(fen.to_san(&m), "O-O");
    }

    #[test]
    fn san_format_check() {
        let fen: Position = "rnbqkbnr/pppp2pp/8/8/8/8/PPPP2PP/RNBQKBNR w KQkq - 0 1"
            .parse()
            .unwrap();
        let m = fen.parse_san("Qh5+").unwrap();
        assert_eq!(fen.to_san(&m), "Qh5+");
    }

    #[test]
    fn san_format_knight_disambiguation() {
        // Knights on e5 and g1 can both move to f3
        let fen: Position = "7k/8/8/4N3/8/8/8/6NK w - - 0 1".parse().unwrap();
        let m = fen.parse_san("Nef3").unwrap();
        assert_eq!(fen.to_san(&m), "Nef3");
    }

    #[test]
    fn san_roundtrip_startpos() {
        let fen: Position = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
            .parse()
            .unwrap();
        let m = fen.parse_san("e4").unwrap();
        assert_eq!(fen.to_san(&m), "e4");
    }

    #[test]
    fn san_roundtrip_midgame() {
        let fen: Position = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1"
            .parse()
            .unwrap();
        let m = fen.parse_san("O-O").unwrap();
        assert_eq!(fen.to_san(&m), "O-O");
    }

    #[test]
    fn public_api_apply_move() {
        let fen: Position = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
            .parse()
            .unwrap();
        let m = fen.parse_san("e4").unwrap();
        let mut new_fen = fen;
        new_fen.apply_move(m).unwrap();
        assert_eq!(
            new_fen.to_fen(),
            "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1"
        );
    }

    #[test]
    fn public_api_apply_move_invalid() {
        let mut fen: Position = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
            .parse()
            .unwrap();
        // e2e5 is not a legal move
        let m = Move::new(0, 12, 28);
        assert!(fen.apply_move(m).is_err());
    }

    #[test]
    fn public_api_legal_moves_from() {
        let fen = Position::new();
        // Knight on b1 (sq 57) has 2 moves: a3 and c3
        assert_eq!(fen.legal_moves_from(Square(57)).len(), 2);
        // Empty square
        assert!(fen.legal_moves_from(Square(28)).is_empty());
    }

    #[test]
    fn public_api_piece_at_and_side_to_move() {
        let fen = Position::new();
        assert_eq!(
            fen.piece_at(Square(60)),
            Some(Piece {
                color: Color::White,
                kind: PieceKind::King
            })
        );
        assert_eq!(fen.piece_at(Square(3)).map(Piece::symbol), Some('q'));
        assert_eq!(fen.piece_at(Square(28)), None);
        assert_eq!(fen.side_to_move(), Color::White);

        let fen: Position = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1"
            .parse()
            .unwrap();
        assert_eq!(fen.side_to_move(), Color::Black);
    }

    #[test]
    fn status_ongoing() {
        assert_eq!(Position::new().status(), Status::Ongoing);
    }

    #[test]
    fn status_checkmate() {
        // Fool's mate
        let fen: Position = "rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3"
            .parse()
            .unwrap();
        assert!(fen.is_check());
        assert_eq!(
            fen.status(),
            Status::Checkmate {
                winner: Color::Black
            }
        );
    }

    #[test]
    fn status_stalemate() {
        let fen: Position = "7k/5Q2/6K1/8/8/8/8/8 b - - 0 1".parse().unwrap();
        assert!(!fen.is_check());
        assert_eq!(fen.status(), Status::Draw(DrawReason::Stalemate));
    }

    #[test]
    fn status_insufficient_material() {
        let draws = [
            "4k3/8/8/8/8/8/8/4K3 w - - 0 1",    // K vs K
            "4k3/8/8/8/8/8/8/2B1K3 w - - 0 1",  // K+B vs K
            "4k3/8/8/8/8/8/8/1N2K3 w - - 0 1",  // K+N vs K
            "2b1k3/8/8/8/8/8/8/5BK1 w - - 0 1", // bishops on the same square color
        ];
        for fen_str in draws {
            let fen: Position = fen_str.parse().unwrap();
            assert_eq!(
                fen.status(),
                Status::Draw(DrawReason::InsufficientMaterial),
                "{fen_str}"
            );
        }

        let not_draws = [
            "4k3/8/8/8/8/8/4P3/4K3 w - - 0 1",  // pawn
            "1b2k3/8/8/8/8/8/8/5BK1 w - - 0 1", // bishops on opposite square colors
            "4k3/8/8/8/8/8/8/1NB1K3 w - - 0 1", // K+B+N
            "4k3/8/8/8/8/8/8/NN2K3 w - - 0 1",  // two knights
        ];
        for fen_str in not_draws {
            let fen: Position = fen_str.parse().unwrap();
            assert_eq!(fen.status(), Status::Ongoing, "{fen_str}");
        }
    }

    #[test]
    fn status_fifty_move_rule() {
        let fen: Position = "4k3/8/8/8/8/8/4P3/4K3 w - - 99 80".parse().unwrap();
        assert_eq!(fen.status(), Status::Ongoing);

        let fen: Position = "4k3/8/8/8/8/8/4P3/4K3 w - - 100 80".parse().unwrap();
        assert_eq!(fen.status(), Status::Draw(DrawReason::FiftyMoveRule));
    }

    #[test]
    fn checkmate_takes_precedence_over_fifty_move_rule() {
        let fen: Position = "rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 100 3"
            .parse()
            .unwrap();
        assert_eq!(
            fen.status(),
            Status::Checkmate {
                winner: Color::Black
            }
        );
    }

    #[test]
    fn halfmove_clock_saturates() {
        let mut fen: Position = "4k3/8/8/8/8/8/8/R3K3 w - - 127 100".parse().unwrap();
        fen.apply_move("a1-a2".parse().unwrap()).unwrap();
        assert_eq!(fen.halfmove_cnt(), 127);
        assert_eq!(fen.side_to_move(), Color::Black);
        assert!(
            "4k3/8/8/8/8/8/8/R3K3 w - - 128 100"
                .parse::<Position>()
                .is_err()
        );
    }

    #[test]
    fn fullmove_counter_past_255() {
        let mut fen: Position = "4k3/8/8/8/8/8/8/R3K3 b - - 0 255".parse().unwrap();
        fen.apply_move("e8-e7".parse().unwrap()).unwrap();
        assert_eq!(fen.fullmove_cnt(), 256);
        assert_eq!(fen.to_fen(), "8/4k3/8/8/8/8/8/R3K3 w - - 1 256");
    }

    #[test]
    fn game_threefold_repetition() {
        let mut game = Game::new();
        let shuffle = ["Nf3", "Nf6", "Ng1", "Ng8"];
        // Start position occurs for the second time after one shuffle, third after two.
        for _ in 0..2 {
            assert_eq!(game.status(), Status::Ongoing);
            for san in shuffle {
                let m = game.position().parse_san(san).unwrap();
                game.play(m).unwrap();
            }
        }
        assert_eq!(game.moves().len(), 8);
        assert_eq!(game.status(), Status::Draw(DrawReason::ThreefoldRepetition));
        let m = game.position().parse_san("Nf3").unwrap();
        assert!(matches!(game.play(m), Err(Error::GameOver)));
    }

    #[test]
    fn game_repetition_ignores_unusable_en_passant() {
        // After 1.e4 the FEN has an e3 target, but no black pawn can capture, so the same
        // placement after each later Ng1 (no e3 target) counts as a repeat of it.
        let mut game = Game::new();
        for san in ["e4", "Nf6", "Nf3", "Ng8", "Ng1", "Nf6", "Nf3", "Ng8", "Ng1"] {
            let m = game.position().parse_san(san).unwrap();
            game.play(m).unwrap();
        }
        assert_eq!(game.status(), Status::Draw(DrawReason::ThreefoldRepetition));
    }

    #[test]
    fn game_rejects_illegal_move() {
        let mut game = Game::new();
        assert!(matches!(
            game.play("e2-e5".parse().unwrap()),
            Err(Error::InvalidMove)
        ));
        assert!(game.moves().is_empty());
    }

    #[test]
    fn after_e4_e5_nf3_black_has_29_moves() {
        // After 1.e4 e5 2.Nf3, the open e-file/diagonals give black 29 legal moves
        let fen: Position = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
            .parse()
            .unwrap();
        let m1 = fen.parse_san("e4").unwrap();
        let fen2 = {
            let mut f = fen;
            f.apply_move_unchecked(m1);
            f
        };
        let m2 = fen2.parse_san("e5").unwrap();
        let fen3 = {
            let mut f = fen2;
            f.apply_move_unchecked(m2);
            f
        };
        let m3 = fen3.parse_san("Nf3").unwrap();
        let fen4 = {
            let mut f = fen3;
            f.apply_move_unchecked(m3);
            f
        };

        assert_eq!(fen4.legal_moves().len(), 29);
    }

    #[test]
    fn san_parse_piece_capture() {
        // White bishop on e2 captures black knight on f3
        let fen: Position = "rnbqkbnr/pppppppp/8/8/8/5n2/PPPPBPPP/RNBQK1NR w KQkq - 0 1"
            .parse()
            .unwrap();
        let m = fen.parse_san("Bxf3").unwrap();
        assert_eq!(fen.to_san(&m), "Bxf3");
    }

    #[test]
    fn san_format_mate() {
        // Scholar's mate position: after 1.e4 e5 2.Bc4 Nc6 3.Qh5 Nf6
        let fen: Position = "r1bqkb1r/pppp1ppp/2n2n2/4p2Q/2B1P3/8/PPPP1PPP/RNB1K1NR w KQkq - 4 4"
            .parse()
            .unwrap();
        let m = fen.parse_san("Qxf7#").unwrap();
        assert_eq!(fen.to_san(&m), "Qxf7#");
    }

    #[test]
    fn exhaustive_san_roundtrip() {
        let positions = [
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
            "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
            "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
        ];

        for fen_str in &positions {
            let fen: Position = fen_str.parse().unwrap();
            for m in fen.legal_moves() {
                let san = fen.to_san(&m);
                let m2 = fen.parse_san(&san).unwrap_or_else(|e| {
                    panic!("Failed to parse SAN '{san}' from {fen_str}: {e:?}")
                });
                assert_eq!(m, m2, "Round-trip failed for '{san}' from {fen_str}");
            }
        }
    }

    // Perft tests

    #[test]
    fn perft_startpos_depth1() {
        let fen: Position = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
            .parse()
            .unwrap();
        assert_eq!(perft(&fen, 1), 20);
    }

    #[test]
    fn perft_startpos_depth2() {
        let fen: Position = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
            .parse()
            .unwrap();
        assert_eq!(perft(&fen, 2), 400);
    }

    #[test]
    fn perft_startpos_depth3() {
        let fen: Position = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
            .parse()
            .unwrap();
        assert_eq!(perft(&fen, 3), 8_902);
    }

    #[test]
    fn perft_startpos_depth4() {
        let fen: Position = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
            .parse()
            .unwrap();
        assert_eq!(perft(&fen, 4), 197_281);
    }

    #[test]
    fn perft_kiwipete_depth1() {
        let fen: Position = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1"
            .parse()
            .unwrap();
        assert_eq!(perft(&fen, 1), 48);
    }

    #[test]
    fn perft_kiwipete_depth2() {
        let fen: Position = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1"
            .parse()
            .unwrap();
        assert_eq!(perft(&fen, 2), 2_039);
    }

    #[test]
    fn perft_kiwipete_depth3() {
        let fen: Position = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1"
            .parse()
            .unwrap();
        assert_eq!(perft(&fen, 3), 97_862);
    }

    #[test]
    fn perft_position3_depth3() {
        let fen: Position = "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1".parse().unwrap();
        assert_eq!(perft(&fen, 3), 2_812);
    }

    #[test]
    fn perft_position3_depth4() {
        let fen: Position = "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1".parse().unwrap();
        assert_eq!(perft(&fen, 4), 43_238);
    }

    #[test]
    fn perft_position4_depth3() {
        let fen: Position = "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1"
            .parse()
            .unwrap();
        assert_eq!(perft(&fen, 3), 9_467);
    }
}
