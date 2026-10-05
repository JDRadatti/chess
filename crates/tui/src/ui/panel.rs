use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Paragraph, Widget},
};

use super::LABEL;
use crate::app::App;
use game::{DrawReason, Status, engine::EngineInfo};

const CONTROLS: [&str; 7] = [
    "arrows/hjkl  move cursor",
    "enter/space  pick up / drop",
    "mouse        click to move",
    "esc          cancel",
    "g            piece style",
    "n            new game",
    "q            quit",
];

/// Status line, check warning, engine progress, move list, and controls help.
pub(super) struct GamePanel<'a> {
    app: &'a App,
}

impl<'a> GamePanel<'a> {
    pub(super) fn new(app: &'a App) -> Self {
        Self { app }
    }
}

impl Widget for GamePanel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let game = self.app.game_match.game();
        let mut lines = vec![Line::styled(status_text(self.app), status_style(self.app))];
        if game.status() == Status::Ongoing && game.position().is_check() {
            lines.push(Line::styled(
                "Check!",
                Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
            ));
        }
        if let Some(engine) = &self.app.engine_info {
            lines.extend(engine_lines(self.app, engine));
        }
        lines.push(Line::from(""));

        // Show as many of the latest moves as fit between the status and the controls.
        let reserved = lines.len() + 1 + CONTROLS.len() + 2;
        let room = (area.height as usize).saturating_sub(reserved);
        let pairs: Vec<&[String]> = self.app.san_history.chunks(2).collect();
        let skip = pairs.len().saturating_sub(room);
        for (i, pair) in pairs.iter().enumerate().skip(skip) {
            let mut spans = vec![
                Span::styled(format!("{:>3}. ", i + 1), Style::new().fg(LABEL)),
                Span::styled(format!("{:<8}", pair[0]), san_style(&pair[0])),
            ];
            if let Some(reply) = pair.get(1) {
                spans.push(Span::styled(reply.clone(), san_style(reply)));
            }
            lines.push(Line::from(spans));
        }

        let controls_top = (area.height as usize).saturating_sub(2 + CONTROLS.len());
        let padding = controls_top.saturating_sub(lines.len()).max(1);
        lines.extend(std::iter::repeat_n(Line::from(""), padding));
        lines.extend(
            CONTROLS
                .iter()
                .map(|c| Line::styled(*c, Style::new().fg(LABEL))),
        );

        Paragraph::new(lines)
            .block(
                Block::bordered()
                    .title(" Game ")
                    .border_type(BorderType::Rounded),
            )
            .render(area, buf);
    }
}

pub(super) fn status_text(app: &App) -> String {
    match app.game_match.game().status() {
        Status::Ongoing if app.is_human_turn() => "Your move".to_string(),
        Status::Ongoing => "Computer is thinking...".to_string(),
        Status::Checkmate { winner } if winner == app.human_color => {
            "Checkmate, you win!".to_string()
        }
        Status::Checkmate { .. } => "Checkmate, computer wins".to_string(),
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

/// The opponent's level and, once it has searched, how deep it looked and its evaluation
/// (positive favors White).
fn engine_lines(app: &App, engine: &EngineInfo) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(vec![
        Span::styled("Computer  ", Style::new().fg(LABEL)),
        Span::raw(app.level.name()),
    ])];
    if let Some(info) = engine.latest() {
        lines.push(Line::styled(
            format!("depth {} · eval {}", info.depth, info.score),
            Style::new().fg(LABEL),
        ));
    }
    lines
}

/// Green for a win, red for a loss, yellow for a draw.
pub(super) fn status_style(app: &App) -> Style {
    let color = match app.game_match.game().status() {
        Status::Ongoing => return Style::new(),
        Status::Checkmate { winner } if winner == app.human_color => Color::Green,
        Status::Checkmate { .. } => Color::Red,
        Status::Draw(_) => Color::Yellow,
    };
    Style::new().fg(color).add_modifier(Modifier::BOLD)
}

fn san_style(san: &str) -> Style {
    if san.ends_with('#') {
        Style::new().fg(Color::Red).add_modifier(Modifier::BOLD)
    } else if san.ends_with('+') {
        Style::new().fg(Color::Yellow)
    } else {
        Style::new()
    }
}
