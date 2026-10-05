#![expect(
    missing_docs,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    reason = "doc comments are tracked in TODO.md"
)]

/// Application.
pub mod app;

/// Terminal events handler.
pub mod event;

/// Keyboard-driven chess player.
pub mod human;

/// Piece artwork for terminals that can show images.
pub mod pieces;

/// Widget renderer.
pub mod ui;

/// Terminal user interface.
pub mod tui;

/// Application updater.
pub mod update;

#[cfg(test)]
mod tests;
use anyhow::Result;
use app::App;
use event::{Event, EventHandler};
use pieces::PieceImages;
use ratatui::{Terminal, backend::CrosstermBackend};
use ratatui_image::picker::Picker;
use tui::Tui;
use update::{update, update_mouse};

fn main() -> Result<()> {
    // Create an application.
    let mut app = App::new();

    // Ask the terminal about image support before anything else reads its replies.
    let picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());
    if let Ok(images) = PieceImages::new(picker) {
        app.set_piece_images(images);
    }

    // Initialize the terminal user interface.
    let backend = CrosstermBackend::new(std::io::stderr());
    let terminal = Terminal::new(backend)?;
    // The computer's move and search progress show up on the next tick, so tick often.
    let events = EventHandler::new(100);
    let mut tui = Tui::new(terminal, events);
    tui.enter()?;

    // Start the main loop.
    while !app.should_quit {
        // Render the user interface.
        tui.draw(&mut app)?;
        // Handle events.
        match tui.events.next()? {
            Event::Tick => {}
            Event::Key(key_event) => update(&mut app, key_event),
            Event::Mouse(mouse_event) => update_mouse(&mut app, mouse_event),
            Event::Resize(_, _) => app.request_full_redraw(),
        }
        // Tick on every event so the human's move is applied right after the key press;
        // the computer thinks in the background and its reply lands on a later event.
        app.tick();
    }

    // Exit the user interface.
    tui.exit()?;
    Ok(())
}
