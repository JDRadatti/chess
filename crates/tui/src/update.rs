use game::{PieceKind, engine::Level};
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

use crate::app::{
    App, Button, ColorChoice, Mode, NEW_GAME_OPTIONS, NewGameOption, PROMOTION_CHOICES,
};

pub fn update(app: &mut App, key_event: KeyEvent) {
    let code = key_event.code;
    if matches!(code, KeyCode::Char('c' | 'C')) && key_event.modifiers == KeyModifiers::CONTROL {
        app.quit();
        return;
    }

    match app.mode {
        Mode::NewGame { .. } => match code {
            KeyCode::Up | KeyCode::Char('k') => app.move_choice(-1),
            KeyCode::Down | KeyCode::Char('j') => app.move_choice(1),
            KeyCode::Char('w') => app.choose(NewGameOption::Color(ColorChoice::White)),
            KeyCode::Char('b') => app.choose(NewGameOption::Color(ColorChoice::Black)),
            KeyCode::Char('r') => app.choose(NewGameOption::Color(ColorChoice::Random)),
            KeyCode::Char(digit @ '1'..='4') => {
                let index = digit as usize - '1' as usize;
                app.choose(NewGameOption::Level(Level::ALL[index]));
            }
            KeyCode::Enter | KeyCode::Char(' ') => app.confirm_choice(),
            KeyCode::Esc => app.close_popup(),
            KeyCode::Char('q') => app.quit(),
            _ => {}
        },
        Mode::Promotion { .. } => match code {
            KeyCode::Up | KeyCode::Char('k') => app.move_choice(-1),
            KeyCode::Down | KeyCode::Char('j') => app.move_choice(1),
            KeyCode::Char('q') => app.promote(PieceKind::Queen),
            KeyCode::Char('r') => app.promote(PieceKind::Rook),
            KeyCode::Char('b') => app.promote(PieceKind::Bishop),
            KeyCode::Char('n') => app.promote(PieceKind::Knight),
            KeyCode::Enter | KeyCode::Char(' ') => app.confirm_choice(),
            KeyCode::Esc => app.close_popup(),
            _ => {}
        },
        Mode::Playing if app.show_game_over_popup() => match code {
            KeyCode::Char('n') => app.open_new_game(),
            KeyCode::Char('q') => app.quit(),
            KeyCode::Esc => app.close_popup(),
            _ => {}
        },
        Mode::Playing => match code {
            KeyCode::Esc => {
                if !app.cancel_selection() {
                    app.quit();
                }
            }
            KeyCode::Char('q') => app.quit(),
            KeyCode::Up | KeyCode::Char('k') => app.move_cursor(1, 0),
            KeyCode::Down | KeyCode::Char('j') => app.move_cursor(-1, 0),
            KeyCode::Left | KeyCode::Char('h') => app.move_cursor(0, -1),
            KeyCode::Right | KeyCode::Char('l') => app.move_cursor(0, 1),
            KeyCode::Enter | KeyCode::Char(' ') => app.select(),
            KeyCode::Char('g') => app.toggle_piece_style(),
            KeyCode::Char('n') => app.open_new_game(),
            _ => {}
        },
    }
}

/// Left clicks press popup buttons first, then board squares. Clicking off the board while
/// playing drops the selected piece back.
pub fn update_mouse(app: &mut App, mouse_event: MouseEvent) {
    if mouse_event.kind != MouseEventKind::Down(MouseButton::Left) {
        return;
    }
    let (x, y) = (mouse_event.column, mouse_event.row);

    if let Some(button) = app.hitboxes.button_at(x, y) {
        match button {
            Button::Promotion(i) => app.promote(PROMOTION_CHOICES[i]),
            Button::NewGameOption(i) => app.choose(NEW_GAME_OPTIONS[i]),
            Button::StartGame => app.confirm_choice(),
            Button::OpenNewGame => app.open_new_game(),
            Button::Quit => app.quit(),
        }
        return;
    }

    if app.mode != Mode::Playing || app.show_game_over_popup() {
        return;
    }
    match app.hitboxes.board.and_then(|b| b.square_at_point(x, y)) {
        Some(sq) => app.click_square(sq),
        None => {
            app.cancel_selection();
        }
    }
}
