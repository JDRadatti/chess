use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Widget,
};

use super::LABEL;
use crate::app::{App, BoardGeometry};
use game::{Move, Piece, PieceKind, Position, Square};

pub const RANK_LABEL_WIDTH: u16 = 2;
const MAX_SCALE: u16 = 6;

const LIGHT_SQUARE: Color = Color::Rgb(200, 160, 110);
const DARK_SQUARE: Color = Color::Rgb(150, 110, 70);
const LAST_MOVE_LIGHT: Color = Color::Rgb(215, 195, 95);
const LAST_MOVE_DARK: Color = Color::Rgb(175, 150, 60);
const CHECK_SQUARE: Color = Color::Rgb(210, 70, 60);
const CURSOR_SQUARE: Color = Color::Rgb(80, 170, 210);
const SELECTED_SQUARE: Color = Color::Rgb(110, 160, 90);
const TARGET_SQUARE: Color = Color::Rgb(150, 190, 120);
/// Background behind captured pieces, so both colors stand out as they do on the board.
const TRAY: Color = DARK_SQUARE;

/// White or black, bold, like pieces on the board.
fn piece_style(piece: Piece) -> Style {
    let fg = match piece.color {
        game::Color::White => Color::White,
        game::Color::Black => Color::Black,
    };
    Style::new().fg(fg).add_modifier(Modifier::BOLD)
}

/// Board size, as the height of one square in rows. Squares are `2 * height + 1` columns wide
/// because terminal cells are roughly twice as tall as they are wide.
#[derive(Clone, Copy)]
pub struct Scale(u16);

impl Scale {
    pub fn square_width(self) -> u16 {
        2 * self.0 + 1
    }

    pub fn square_height(self) -> u16 {
        self.0
    }

    pub fn board_width(self) -> u16 {
        RANK_LABEL_WIDTH + 8 * self.square_width()
    }

    /// 8 ranks plus the file label row.
    pub fn board_height(self) -> u16 {
        8 * self.square_height() + 1
    }

    /// One step down, but never below the smallest size.
    pub fn smaller(self) -> Self {
        Scale(self.0.saturating_sub(1).max(1))
    }

    /// Largest scale whose board fits inside `area`.
    pub fn fitting(area: Rect) -> Option<Self> {
        (1..=MAX_SCALE)
            .rev()
            .map(Scale)
            .find(|s| s.board_width() <= area.width && s.board_height() <= area.height)
    }
}

/// Character for `piece`. With `solid`, white and black share the filled symbols and are told
/// apart by color; otherwise white pieces use the outlined symbols.
pub fn glyph(piece: Piece, unicode: bool, solid: bool) -> char {
    if !unicode {
        return piece.symbol();
    }
    let outlined = !solid && piece.color == game::Color::White;
    match (piece.kind, outlined) {
        (PieceKind::King, true) => '♔',
        (PieceKind::Queen, true) => '♕',
        (PieceKind::Rook, true) => '♖',
        (PieceKind::Bishop, true) => '♗',
        (PieceKind::Knight, true) => '♘',
        (PieceKind::Pawn, true) => '♙',
        (PieceKind::King, false) => '♚',
        (PieceKind::Queen, false) => '♛',
        (PieceKind::Rook, false) => '♜',
        (PieceKind::Bishop, false) => '♝',
        (PieceKind::Knight, false) => '♞',
        (PieceKind::Pawn, false) => '♟',
    }
}

pub struct BoardWidget<'a> {
    position: &'a Position,
    geometry: BoardGeometry,
    cursor: Square,
    selected: Option<Square>,
    /// Destination squares of the selected piece.
    targets: Vec<u8>,
    last_move: Option<Move>,
    /// King of the side to move, when it is in check.
    checked_king: Option<Square>,
    unicode: bool,
    /// Pieces are drawn separately as images.
    images: bool,
}

/// Where to draw a piece image and the square color behind it.
pub struct Sprite {
    pub area: Rect,
    pub piece: Piece,
    pub background: [u8; 3],
}

impl<'a> BoardWidget<'a> {
    pub fn new(app: &'a App, geometry: BoardGeometry) -> Self {
        let game = app.game_match.game();
        let position = game.position();
        let targets = app
            .selected
            .map(|sq| {
                position
                    .legal_moves_from(sq)
                    .iter()
                    .map(|m| m.dst())
                    .collect()
            })
            .unwrap_or_default();
        let checked_king = position.is_check().then(|| {
            let to_move = position.side_to_move();
            (0..64).map(Square).find(|&sq| {
                position.piece_at(sq)
                    == Some(Piece {
                        color: to_move,
                        kind: PieceKind::King,
                    })
            })
        });
        Self {
            position,
            geometry,
            cursor: app.cursor,
            selected: app.selected,
            targets,
            last_move: game.moves().last().copied(),
            checked_king: checked_king.flatten(),
            unicode: app.unicode_pieces(),
            images: app.image_pieces(),
        }
    }

    /// Piece images to draw over the board, if pieces are drawn as images.
    pub fn sprites(&self) -> Vec<Sprite> {
        if !self.images {
            return vec![];
        }
        let g = self.geometry;
        let mut sprites = vec![];
        for row in 0..8 {
            for col in 0..8 {
                let sq = g.square_at_cell(row, col);
                let Some(piece) = self.position.piece_at(sq) else {
                    continue;
                };
                let Color::Rgb(red, green, blue) = self.background(sq) else {
                    continue;
                };
                sprites.push(Sprite {
                    area: Rect::new(
                        g.x + col * g.square_width,
                        g.y + row * g.square_height,
                        g.square_width,
                        g.square_height,
                    ),
                    piece,
                    background: [red, green, blue],
                });
            }
        }
        sprites
    }

    fn background(&self, sq: Square) -> Color {
        let light = (sq.0 / 8 + sq.0 % 8).is_multiple_of(2);
        let in_last_move = self
            .last_move
            .is_some_and(|m| m.src() == sq.0 || m.dst() == sq.0);
        if self.selected == Some(sq) {
            SELECTED_SQUARE
        } else if self.cursor == sq {
            CURSOR_SQUARE
        } else if self.checked_king == Some(sq) {
            CHECK_SQUARE
        } else if self.targets.contains(&sq.0) {
            TARGET_SQUARE
        } else if in_last_move {
            if light {
                LAST_MOVE_LIGHT
            } else {
                LAST_MOVE_DARK
            }
        } else if light {
            LIGHT_SQUARE
        } else {
            DARK_SQUARE
        }
    }
}

impl Widget for BoardWidget<'_> {
    /// `area` must be at least the board size for the geometry's scale.
    fn render(self, area: Rect, buf: &mut Buffer) {
        let g = self.geometry;
        let (w, h) = (g.square_width, g.square_height);
        let blank = " ".repeat(w as usize);

        for row in 0..8 {
            let top = g.y + row * h;
            let rank = g.square_at_cell(row, 0).0 / 8;
            buf.set_string(
                area.x,
                top + h / 2,
                format!("{} ", 8 - rank),
                Style::new().fg(LABEL),
            );

            for col in 0..8 {
                let sq = g.square_at_cell(row, col);
                let left = g.x + col * w;
                let piece = self.position.piece_at(sq);
                let symbol = match piece {
                    Some(_) if self.images => ' ',
                    Some(piece) => glyph(piece, self.unicode, true),
                    None if self.targets.contains(&sq.0) => '·',
                    None => ' ',
                };
                let style = piece
                    .map_or(
                        Style::new().fg(Color::Black).add_modifier(Modifier::BOLD),
                        piece_style,
                    )
                    .bg(self.background(sq));

                for dy in 0..h {
                    buf.set_string(left, top + dy, &blank, style);
                }
                buf.set_string(left + w / 2, top + h / 2, symbol.to_string(), style);
            }
        }

        for col in 0..8 {
            let file = g.square_at_cell(0, col).0 % 8;
            buf.set_string(
                g.x + col * w + w / 2,
                g.y + 8 * h,
                ((b'a' + file) as char).to_string(),
                Style::new().fg(LABEL),
            );
        }
    }
}

/// Pieces `captor` has taken, on a tray styled like the board, plus its material lead.
pub struct CapturesWidget {
    captured: Vec<Piece>,
    lead: i32,
    unicode: bool,
    images: bool,
}

/// Each captured piece gets a slot this wide, which is roughly square in a terminal.
const SLOT_WIDTH: u16 = 2;

impl CapturesWidget {
    pub fn new(app: &App, captor: game::Color) -> Self {
        let position = app.game_match.game().position();
        let victim = captor.opponent();
        let mut captured = vec![];
        for (kind, start) in STARTING_PIECES {
            let remaining = pieces_of(position, victim)
                .filter(|p| p.kind == kind)
                .count();
            let piece = Piece {
                color: victim,
                kind,
            };
            captured.extend(std::iter::repeat_n(piece, start.saturating_sub(remaining)));
        }
        Self {
            captured,
            lead: material(position, captor) - material(position, victim),
            unicode: app.unicode_pieces(),
            images: app.image_pieces(),
        }
    }

    /// Slot of the `i`th captured piece, clipped to `area`.
    fn slot(area: Rect, i: usize) -> Option<Rect> {
        let x = area.x + RANK_LABEL_WIDTH + SLOT_WIDTH * i as u16;
        (x + SLOT_WIDTH <= area.right()).then(|| Rect::new(x, area.y, SLOT_WIDTH, 1))
    }

    /// Piece images to draw over the tray, if pieces are drawn as images.
    pub fn sprites(&self, area: Rect) -> Vec<Sprite> {
        if !self.images {
            return vec![];
        }
        let Color::Rgb(red, green, blue) = TRAY else {
            unreachable!("tray color is RGB");
        };
        self.captured
            .iter()
            .enumerate()
            .filter_map(|(i, &piece)| {
                Some(Sprite {
                    area: Self::slot(area, i)?,
                    piece,
                    background: [red, green, blue],
                })
            })
            .collect()
    }
}

/// Starting count of each kind, in ascending value.
const STARTING_PIECES: [(PieceKind, usize); 5] = [
    (PieceKind::Pawn, 8),
    (PieceKind::Knight, 2),
    (PieceKind::Bishop, 2),
    (PieceKind::Rook, 2),
    (PieceKind::Queen, 1),
];

fn value(kind: PieceKind) -> i32 {
    match kind {
        PieceKind::Pawn => 1,
        PieceKind::Knight | PieceKind::Bishop => 3,
        PieceKind::Rook => 5,
        PieceKind::Queen => 9,
        PieceKind::King => 0,
    }
}

fn pieces_of(position: &Position, color: game::Color) -> impl Iterator<Item = Piece> + '_ {
    (0..64)
        .filter_map(move |sq| position.piece_at(Square(sq)))
        .filter(move |p| p.color == color)
}

fn material(position: &Position, color: game::Color) -> i32 {
    pieces_of(position, color).map(|p| value(p.kind)).sum()
}

impl Widget for CapturesWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut end = area.x + RANK_LABEL_WIDTH;
        for (i, &piece) in self.captured.iter().enumerate() {
            let Some(slot) = Self::slot(area, i) else {
                break;
            };
            let symbol = if self.images {
                ' '
            } else {
                glyph(piece, self.unicode, true)
            };
            buf.set_string(
                slot.x,
                slot.y,
                format!("{symbol:<width$}", width = SLOT_WIDTH as usize),
                piece_style(piece).bg(TRAY),
            );
            end = slot.right();
        }

        if self.lead > 0 {
            let lead = Rect {
                x: end,
                width: area.right().saturating_sub(end),
                ..area
            };
            Line::from(Span::styled(
                format!(" +{}", self.lead),
                Style::new().add_modifier(Modifier::BOLD),
            ))
            .render(lead, buf);
        }
    }
}
