use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::App;

pub fn update(app: &mut App, key_event: KeyEvent) {
    match key_event.code {
        KeyCode::Esc => {
            if !app.cancel_selection() {
                app.quit();
            }
        }
        KeyCode::Char('q') => app.quit(),
        KeyCode::Char('c') | KeyCode::Char('C') if key_event.modifiers == KeyModifiers::CONTROL => {
            app.quit()
        }
        KeyCode::Up | KeyCode::Char('k') => app.move_cursor(1, 0),
        KeyCode::Down | KeyCode::Char('j') => app.move_cursor(-1, 0),
        KeyCode::Left | KeyCode::Char('h') => app.move_cursor(0, -1),
        KeyCode::Right | KeyCode::Char('l') => app.move_cursor(0, 1),
        KeyCode::Enter | KeyCode::Char(' ') => app.select(),
        KeyCode::Char('n') => app.new_game(),
        _ => {}
    };
}
