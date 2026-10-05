use ratatui::{
    buffer::{Buffer, CellDiffOption},
    layout::{Rect, Size},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Widget,
};

use crate::{
    app::{App, BoardGeometry},
    pieces::Graphic,
};
use game::{Move, Piece, PieceKind, Position, Square};

const MAX_SCALE: u16 = 6;

const LIGHT_SQUARE: Color = Color::Rgb(235, 236, 208);
const DARK_SQUARE: Color = Color::Rgb(115, 149, 82);
const LAST_MOVE_LIGHT: Color = Color::Rgb(245, 246, 130);
const LAST_MOVE_DARK: Color = Color::Rgb(185, 202, 67);
const CHECK_SQUARE: Color = Color::Rgb(210, 70, 60);
const CURSOR_SQUARE: Color = Color::Rgb(80, 170, 210);
const SELECTED_SQUARE: Color = Color::Rgb(230, 160, 70);
/// Background behind captured pieces, so both colors stand out as they do on the board.
const TRAY: Color = DARK_SQUARE;
/// How much darker a capturable square is than usual.
const CAPTURE_SHADE: f32 = 0.8;
/// How much darker a move dot is than its square.
const DOT_SHADE: f32 = 0.6;

/// Text pieces are black so they read on light and dark squares alike; white pieces use
/// outlined symbols or capital letters instead.
const PIECE_TEXT: Style = Style::new().fg(Color::Black).add_modifier(Modifier::BOLD);

fn shade(color: Color, factor: f32) -> Color {
    match color {
        Color::Rgb(r, g, b) => Color::Rgb(
            (f32::from(r) * factor) as u8,
            (f32::from(g) * factor) as u8,
            (f32::from(b) * factor) as u8,
        ),
        other => other,
    }
}

/// Channels of a board color; every board color is RGB.
fn rgb(color: Color) -> [u8; 3] {
    match color {
        Color::Rgb(r, g, b) => [r, g, b],
        other => unreachable!("board colors are RGB, got {other:?}"),
    }
}

fn is_light(sq: Square) -> bool {
    (sq.0 / 8 + sq.0 % 8).is_multiple_of(2)
}

/// Board size, as the height of one square in rows. Squares are `2 * height + 1` columns wide
/// because terminal cells are roughly twice as tall as they are wide.
#[derive(Clone, Copy)]
pub(super) struct Scale(u16);

impl Scale {
    pub(super) fn square_width(self) -> u16 {
        2 * self.0 + 1
    }

    pub(super) fn square_height(self) -> u16 {
        self.0
    }

    pub(super) fn board_width(self) -> u16 {
        8 * self.square_width()
    }

    pub(super) fn board_height(self) -> u16 {
        8 * self.square_height()
    }

    /// One step down, but never below the smallest size.
    pub(super) fn smaller(self) -> Self {
        Scale(self.0.saturating_sub(1).max(1))
    }

    /// Largest scale whose board fits inside `area`.
    pub(super) fn fitting(area: Rect) -> Option<Self> {
        (1..=MAX_SCALE)
            .rev()
            .map(Scale)
            .find(|s| s.board_width() <= area.width && s.board_height() <= area.height)
    }
}

/// Character for `piece`: outlined symbols for white, filled for black, or FEN letters.
pub(super) fn glyph(piece: Piece, unicode: bool) -> char {
    if !unicode {
        return piece.symbol();
    }
    let outlined = piece.color == game::Color::White;
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

pub(super) struct BoardWidget<'a> {
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
pub(super) struct Sprite {
    /// Where the image goes; may be narrower than the square to leave room for a coordinate.
    pub area: Rect,
    /// Size of the whole square, which the graphic is centered in.
    pub square: Size,
    /// Part of the square to draw, in cells relative to its top-left corner; `area` is where
    /// that part goes on screen.
    pub crop: Rect,
    pub graphic: Graphic,
    pub background: [u8; 3],
}

/// Part of the square at screen `(row, col)` that an image may cover. Squares carrying a
/// coordinate leave their last column to plain text, because some terminals draw an image over
/// a cell even after text replaces its placeholder.
fn image_region(g: BoardGeometry, row: u16, col: u16) -> Rect {
    let has_coordinate = col == 7 || row == 7;
    Rect::new(
        0,
        0,
        g.square_width - u16::from(has_coordinate),
        g.square_height,
    )
}

impl<'a> BoardWidget<'a> {
    pub(super) fn new(app: &'a App, geometry: BoardGeometry) -> Self {
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
    pub(super) fn sprites(&self) -> Vec<Sprite> {
        if !self.images {
            return vec![];
        }
        let g = self.geometry;
        let mut sprites = vec![];
        for row in 0..8 {
            for col in 0..8 {
                let sq = g.square_at_cell(row, col);
                let background = self.background(sq);
                let graphic = match self.position.piece_at(sq) {
                    Some(piece) => Graphic::Piece(piece),
                    None if self.targets.contains(&sq.0) => Graphic::Dot {
                        color: rgb(shade(background, DOT_SHADE)),
                    },
                    None => continue,
                };
                let (left, top) = (g.x + col * g.square_width, g.y + row * g.square_height);
                let crop = image_region(g, row, col);
                sprites.push(Sprite {
                    area: Rect {
                        x: left,
                        y: top,
                        ..crop
                    },
                    square: Size::new(g.square_width, g.square_height),
                    crop,
                    graphic,
                    background: rgb(background),
                });
            }
        }
        sprites
    }

    /// Plain square color, tinted if the square was part of the last move.
    fn base(&self, sq: Square) -> Color {
        let in_last_move = self
            .last_move
            .is_some_and(|m| m.src() == sq.0 || m.dst() == sq.0);
        match (in_last_move, is_light(sq)) {
            (true, true) => LAST_MOVE_LIGHT,
            (true, false) => LAST_MOVE_DARK,
            (false, true) => LIGHT_SQUARE,
            (false, false) => DARK_SQUARE,
        }
    }

    fn background(&self, sq: Square) -> Color {
        let capturable = self.targets.contains(&sq.0) && self.position.piece_at(sq).is_some();
        if self.selected == Some(sq) {
            SELECTED_SQUARE
        } else if self.cursor == sq {
            CURSOR_SQUARE
        } else if self.checked_king == Some(sq) {
            CHECK_SQUARE
        } else if capturable {
            shade(self.base(sq), CAPTURE_SHADE)
        } else {
            self.base(sq)
        }
    }
}

impl Widget for BoardWidget<'_> {
    /// `area` must be at least the board size for the geometry's scale.
    fn render(self, _area: Rect, buf: &mut Buffer) {
        let g = self.geometry;
        let (w, h) = (g.square_width, g.square_height);
        let blank = " ".repeat(w as usize);

        for row in 0..8 {
            for col in 0..8 {
                let sq = g.square_at_cell(row, col);
                let (left, top) = (g.x + col * w, g.y + row * h);
                let bg = self.background(sq);
                let (symbol, style) = match self.position.piece_at(sq) {
                    _ if self.images => (' ', Style::new()),
                    Some(piece) => (glyph(piece, self.unicode), PIECE_TEXT),
                    None if self.targets.contains(&sq.0) => {
                        ('●', Style::new().fg(shade(bg, DOT_SHADE)))
                    }
                    None => (' ', Style::new()),
                };
                let style = style.bg(bg);

                for dy in 0..h {
                    buf.set_string(left, top + dy, &blank, style);
                }
                buf.set_string(left + w / 2, top + h / 2, symbol.to_string(), style);
            }
        }
    }
}

/// Rank numbers in the top-right corner of the right-hand file and file letters in the
/// bottom-right corner of the bottom rank, colored like the opposite square.
///
/// Must run after piece images are drawn; the images on these squares leave the coordinate
/// column uncovered.
pub(super) fn draw_coordinates(buf: &mut Buffer, g: BoardGeometry) {
    let (w, h) = (g.square_width, g.square_height);
    for row in 0..8 {
        let sq = g.square_at_cell(row, 7);
        let x = g.x + 8 * w - 1;
        put_coordinate(buf, x, g.y + row * h, (b'8' - sq.0 / 8) as char, sq);
    }
    for col in 0..8 {
        let sq = g.square_at_cell(7, col);
        let (x, y) = (g.x + col * w + w - 1, g.y + 8 * h - 1);
        put_coordinate(buf, x, y, (b'a' + sq.0 % 8) as char, sq);
    }
}

fn put_coordinate(buf: &mut Buffer, x: u16, y: u16, label: char, sq: Square) {
    let fg = if is_light(sq) {
        DARK_SQUARE
    } else {
        LIGHT_SQUARE
    };
    if let Some(cell) = buf.cell_mut((x, y)) {
        cell.set_char(label)
            .set_style(Style::new().fg(fg).add_modifier(Modifier::BOLD))
            .set_diff_option(CellDiffOption::None);
    }
}

/// Pieces `captor` has taken, on a tray styled like the board, plus its material lead.
pub(super) struct CapturesWidget {
    captured: Vec<Piece>,
    lead: i32,
    unicode: bool,
    images: bool,
}

/// Each captured piece gets a slot this wide, which is roughly square in a terminal.
const SLOT_WIDTH: u16 = 2;

impl CapturesWidget {
    pub(super) fn new(app: &App, captor: game::Color) -> Self {
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
        let x = area.x + SLOT_WIDTH * i as u16;
        (x + SLOT_WIDTH <= area.right()).then(|| Rect::new(x, area.y, SLOT_WIDTH, 1))
    }

    /// Piece images to draw over the tray, if pieces are drawn as images.
    pub(super) fn sprites(&self, area: Rect) -> Vec<Sprite> {
        if !self.images {
            return vec![];
        }
        self.captured
            .iter()
            .enumerate()
            .filter_map(|(i, &piece)| {
                let area = Self::slot(area, i)?;
                Some(Sprite {
                    area,
                    square: area.as_size(),
                    crop: Rect::new(0, 0, area.width, area.height),
                    graphic: Graphic::Piece(piece),
                    background: rgb(TRAY),
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
        let mut end = area.x;
        for (i, &piece) in self.captured.iter().enumerate() {
            let Some(slot) = Self::slot(area, i) else {
                break;
            };
            let symbol = if self.images {
                ' '
            } else {
                glyph(piece, self.unicode)
            };
            buf.set_string(
                slot.x,
                slot.y,
                format!("{symbol:<width$}", width = SLOT_WIDTH as usize),
                PIECE_TEXT.bg(TRAY),
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
