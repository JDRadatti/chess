use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, BorderType, Paragraph, Widget},
};

use crate::app::App;
use game::{DrawReason, Piece, Position, Square, Status};

const RANK_LABEL_WIDTH: u16 = 2;
const MAX_SCALE: u16 = 6;
const INFO_WIDTH: u16 = 34;
/// Enough rows for the status, a few moves, and the controls help.
const MIN_INFO_HEIGHT: u16 = 22;

const LIGHT_SQUARE: Color = Color::Rgb(200, 160, 110);
const DARK_SQUARE: Color = Color::Rgb(150, 110, 70);
const CURSOR_SQUARE: Color = Color::Rgb(220, 200, 60);
const SELECTED_SQUARE: Color = Color::Rgb(110, 160, 90);
const TARGET_SQUARE: Color = Color::Rgb(100, 140, 200);
const LABEL: Color = Color::DarkGray;

pub fn render(app: &mut App, frame: &mut Frame) {
    let [board_column, info_column] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(INFO_WIDTH)])
            .areas(frame.area());

    let scale = Scale::fitting(board_column)
        .map(Scale::smaller)
        .unwrap_or(Scale(1));
    let board_area = board_column.centered(
        Constraint::Length(scale.board_width()),
        Constraint::Length(scale.board_height()),
    );
    let info_area = info_column
        .centered_vertically(Constraint::Length(scale.board_height().max(MIN_INFO_HEIGHT)));

    let position = app.game_match.game().position();
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
    frame.render_widget(
        BoardWidget {
            position,
            cursor: app.cursor,
            selected: app.selected,
            targets,
            flipped: app.human_color == game::Color::Black,
        },
        board_area,
    );

    frame.render_widget(
        Paragraph::new(info_lines(app)).block(
            Block::bordered()
                .title(" Game ")
                .border_type(BorderType::Rounded),
        ),
        info_area,
    );
}

fn info_lines(app: &App) -> Vec<Line<'static>> {
    let game = app.game_match.game();
    let mut lines = vec![Line::from(status_text(app))];
    if game.status() == Status::Ongoing && game.position().is_check() {
        lines.push(Line::styled(
            "Check!",
            Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
        ));
    }

    lines.push(Line::from(""));
    let pairs: Vec<String> = app
        .san_history
        .chunks(2)
        .enumerate()
        .map(|(i, pair)| format!("{:>3}. {:<8}{}", i + 1, pair[0], pair.get(1).map_or("", |s| s)))
        .collect();
    let recent = pairs.len().saturating_sub(12);
    lines.extend(pairs[recent..].iter().cloned().map(Line::from));

    lines.push(Line::from(""));
    lines.push(Line::styled("arrows/hjkl  move cursor", Style::new().fg(LABEL)));
    lines.push(Line::styled("enter/space  pick up / drop", Style::new().fg(LABEL)));
    lines.push(Line::styled("esc          cancel selection", Style::new().fg(LABEL)));
    lines.push(Line::styled("n            new game", Style::new().fg(LABEL)));
    lines.push(Line::styled("q            quit", Style::new().fg(LABEL)));
    lines
}

fn status_text(app: &App) -> String {
    match app.game_match.game().status() {
        Status::Ongoing if app.is_human_turn() => "Your move".to_string(),
        Status::Ongoing => "Computer is thinking...".to_string(),
        Status::Checkmate { winner } => format!("Checkmate, {} wins", color_name(winner)),
        Status::Draw(reason) => format!(
            "Draw by {}",
            match reason {
                DrawReason::Stalemate => "stalemate",
                DrawReason::InsufficientMaterial => "insufficient material",
                DrawReason::FiftyMoveRule => "fifty-move rule",
                DrawReason::ThreefoldRepetition => "threefold repetition",
            }
        ),
    }
}

fn color_name(color: game::Color) -> &'static str {
    match color {
        game::Color::White => "White",
        game::Color::Black => "Black",
    }
}

struct BoardWidget<'a> {
    position: &'a Position,
    cursor: Square,
    selected: Option<Square>,
    /// Destination squares of the selected piece.
    targets: Vec<u8>,
    /// Draw from black's side (rank 1 at the top).
    flipped: bool,
}

/// Board size, as the height of one square in rows. Squares are `2 * height + 1` columns wide
/// because terminal cells are roughly twice as tall as they are wide.
#[derive(Clone, Copy)]
struct Scale(u16);

impl Scale {
    fn square_width(self) -> u16 {
        2 * self.0 + 1
    }

    fn square_height(self) -> u16 {
        self.0
    }

    fn board_width(self) -> u16 {
        RANK_LABEL_WIDTH + 8 * self.square_width()
    }

    /// 8 ranks plus the file label row.
    fn board_height(self) -> u16 {
        8 * self.square_height() + 1
    }

    /// One step down, but never below the smallest size.
    fn smaller(self) -> Self {
        Scale(self.0.saturating_sub(1).max(1))
    }

    /// Largest scale whose board fits inside `area`.
    fn fitting(area: Rect) -> Option<Self> {
        (1..=MAX_SCALE)
            .rev()
            .map(Scale)
            .find(|s| s.board_width() <= area.width && s.board_height() <= area.height)
    }
}

impl BoardWidget<'_> {
    /// Board square shown at screen row/column `(row, col)`, both 0..8 from the top left.
    fn square_at(&self, row: u16, col: u16) -> Square {
        let (row, col) = if self.flipped {
            (7 - row, 7 - col)
        } else {
            (row, col)
        };
        Square((row * 8 + col) as u8)
    }

    fn background(&self, sq: Square) -> Color {
        if self.selected == Some(sq) {
            SELECTED_SQUARE
        } else if self.cursor == sq {
            CURSOR_SQUARE
        } else if self.targets.contains(&sq.0) {
            TARGET_SQUARE
        } else if (sq.0 / 8 + sq.0 % 8).is_multiple_of(2) {
            LIGHT_SQUARE
        } else {
            DARK_SQUARE
        }
    }
}

impl Widget for BoardWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Writing outside the buffer panics, so bail out if the board does not fit.
        let Some(scale) = Scale::fitting(area) else {
            Line::from("Terminal too small").render(area, buf);
            return;
        };
        let (w, h) = (scale.square_width(), scale.square_height());
        let blank = " ".repeat(w as usize);

        for row in 0..8 {
            let top = area.y + row * h;
            let rank = self.square_at(row, 0).0 / 8;
            buf.set_string(
                area.x,
                top + h / 2,
                format!("{} ", 8 - rank),
                Style::new().fg(LABEL),
            );

            for col in 0..8 {
                let sq = self.square_at(row, col);
                let left = area.x + RANK_LABEL_WIDTH + col * w;
                let piece = self.position.piece_at(sq);
                let symbol = match piece {
                    Some(piece) => piece.symbol(),
                    None if self.targets.contains(&sq.0) => '·',
                    None => ' ',
                };
                let fg = match piece.map(|p: Piece| p.color) {
                    Some(game::Color::White) => Color::White,
                    _ => Color::Black,
                };
                let style = Style::new()
                    .fg(fg)
                    .bg(self.background(sq))
                    .add_modifier(Modifier::BOLD);

                for dy in 0..h {
                    buf.set_string(left, top + dy, &blank, style);
                }
                buf.set_string(left + w / 2, top + h / 2, symbol.to_string(), style);
            }
        }

        for col in 0..8 {
            let file = self.square_at(0, col).0 % 8;
            buf.set_string(
                area.x + RANK_LABEL_WIDTH + col * w + w / 2,
                area.y + 8 * h,
                ((b'a' + file) as char).to_string(),
                Style::new().fg(LABEL),
            );
        }
    }
}
