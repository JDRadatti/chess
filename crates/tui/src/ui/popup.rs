use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, BorderType, Clear, Paragraph},
};

use super::{LABEL, board::glyph, panel::status_style};
use crate::app::{App, Button, ColorChoice, NEW_GAME_OPTIONS, NewGameOption, PROMOTION_CHOICES};
use game::{Piece, PieceKind, engine::Level};

/// A clickable line: its index among the popup's lines and what it does.
type ButtonLine = (usize, Button);

pub(super) fn promotion(
    frame: &mut Frame,
    anchor: Rect,
    choice: usize,
    color: game::Color,
    unicode: bool,
    buttons: &mut Vec<(Rect, Button)>,
) {
    let mut lines = vec![];
    let mut clickable = vec![];
    for (i, kind) in PROMOTION_CHOICES.into_iter().enumerate() {
        let (key, name) = match kind {
            PieceKind::Queen => ('q', "Queen"),
            PieceKind::Rook => ('r', "Rook"),
            PieceKind::Bishop => ('b', "Bishop"),
            _ => ('n', "Knight"),
        };
        let symbol = glyph(Piece { color, kind }, unicode);
        clickable.push((lines.len(), Button::Promotion(i)));
        lines.push(option_line(
            format!(" {key}  {symbol}  {name}"),
            i == choice,
        ));
    }
    lines.push(Line::from(""));
    lines.push(Line::styled(" esc  cancel", Style::new().fg(LABEL)));
    draw(
        frame,
        anchor,
        " Promote to ",
        lines,
        Alignment::Left,
        &clickable,
        buttons,
    );
}

/// Color and opponent pickers. `color` and `level` are marked as chosen; the row at `focus`
/// is under the keyboard cursor.
pub(super) fn new_game(
    frame: &mut Frame,
    anchor: Rect,
    color: ColorChoice,
    level: Level,
    focus: usize,
    buttons: &mut Vec<(Rect, Button)>,
) {
    let mut lines = vec![];
    let mut clickable = vec![];
    for (i, option) in NEW_GAME_OPTIONS.into_iter().enumerate() {
        let (key, name, chosen) = match option {
            NewGameOption::Color(c) => {
                let (key, name) = match c {
                    ColorChoice::White => ('w', "White"),
                    ColorChoice::Black => ('b', "Black"),
                    ColorChoice::Random => ('r', "Random"),
                };
                (key, name.to_string(), c == color)
            }
            NewGameOption::Level(l) => {
                let (key, hint) = match l {
                    Level::Beginner => ('1', "makes mistakes"),
                    Level::Easy => ('2', "sees 1 move ahead"),
                    Level::Medium => ('3', "thinks 0.5 s"),
                    Level::Hard => ('4', "thinks 2 s"),
                };
                (key, format!("{:<9}{hint}", l.name()), l == level)
            }
        };
        match (i, option) {
            (0, _) => lines.extend([Line::from(" Play as"), Line::from("")]),
            (_, NewGameOption::Level(Level::Beginner)) => {
                lines.extend([Line::from(""), Line::from(" Opponent"), Line::from("")]);
            }
            _ => {}
        }
        let marker = if chosen { '●' } else { ' ' };
        clickable.push((lines.len(), Button::NewGameOption(i)));
        lines.push(option_line(format!(" {marker} {key}  {name}"), i == focus));
    }
    lines.push(Line::from(""));
    clickable.push((lines.len(), Button::StartGame));
    lines.push(Line::styled(
        " enter  start",
        Style::new().add_modifier(Modifier::BOLD),
    ));
    lines.push(Line::styled(" esc    cancel", Style::new().fg(LABEL)));
    draw(
        frame,
        anchor,
        " New game ",
        lines,
        Alignment::Left,
        &clickable,
        buttons,
    );
}

pub(super) fn game_over(
    frame: &mut Frame,
    anchor: Rect,
    app: &App,
    status: &str,
    buttons: &mut Vec<(Rect, Button)>,
) {
    let lines = vec![
        Line::styled(status.to_string(), status_style(app)),
        Line::from(""),
        Line::from("[n] New game"),
        Line::from("[q] Quit"),
        Line::styled("[esc] View board", Style::new().fg(LABEL)),
    ];
    let clickable = [(2, Button::OpenNewGame), (3, Button::Quit)];
    draw(
        frame,
        anchor,
        " Game over ",
        lines,
        Alignment::Center,
        &clickable,
        buttons,
    );
}

fn option_line(text: String, highlighted: bool) -> Line<'static> {
    if highlighted {
        Line::styled(text, Style::new().add_modifier(Modifier::REVERSED))
    } else {
        Line::from(text)
    }
}

/// Draws a bordered popup sized to `lines`, centered on `anchor` and kept inside the frame.
fn draw(
    frame: &mut Frame,
    anchor: Rect,
    title: &str,
    lines: Vec<Line<'static>>,
    alignment: Alignment,
    clickable: &[ButtonLine],
    buttons: &mut Vec<(Rect, Button)>,
) {
    let content_width = lines
        .iter()
        .map(Line::width)
        .chain([title.len()])
        .max()
        .unwrap_or(0) as u16;
    let bounds = frame.area();
    let width = (content_width + 4).min(bounds.width);
    let height = (lines.len() as u16 + 2).min(bounds.height);
    let x = (anchor.x + anchor.width / 2)
        .saturating_sub(width / 2)
        .clamp(bounds.x, bounds.right() - width);
    let y = (anchor.y + anchor.height / 2)
        .saturating_sub(height / 2)
        .clamp(bounds.y, bounds.bottom() - height);
    let area = Rect::new(x, y, width, height);

    for &(line, button) in clickable {
        let row = area.y + 1 + line as u16;
        if row < area.bottom() - 1 {
            buttons.push((Rect::new(area.x + 1, row, area.width - 2, 1), button));
        }
    }

    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).alignment(alignment).block(
            Block::bordered()
                .title(title.to_string())
                .border_type(BorderType::Rounded),
        ),
        area,
    );
}
