mod board;
mod panel;
mod popup;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Color,
    text::Line,
};
use ratatui_image::Image;

use crate::app::{App, BoardGeometry, Hitboxes, Mode};
use board::{BoardWidget, CapturesWidget, Scale};

const INFO_WIDTH: u16 = 34;
/// Enough rows for the status, a few moves, and the controls help.
const MIN_INFO_HEIGHT: u16 = 24;
const LABEL: Color = Color::DarkGray;

pub fn render(app: &mut App, frame: &mut Frame) {
    let [board_column, info_column] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(INFO_WIDTH)])
            .areas(frame.area());

    // One row above and below the board for captured pieces.
    let room_for_board = Rect {
        height: board_column.height.saturating_sub(2),
        ..board_column
    };
    let Some(largest) = Scale::fitting(room_for_board) else {
        frame.render_widget(Line::from("Terminal too small"), board_column);
        app.hitboxes = Hitboxes::default();
        return;
    };
    let scale = largest.smaller();

    let [top_captures, board_area, bottom_captures] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(scale.board_height()),
        Constraint::Length(1),
    ])
    .areas(board_column.centered(
        Constraint::Length(scale.board_width()),
        Constraint::Length(scale.board_height() + 2),
    ));
    let info_area = info_column.centered_vertically(Constraint::Length(
        (scale.board_height() + 2).max(MIN_INFO_HEIGHT),
    ));

    let geometry = BoardGeometry {
        x: board_area.x,
        y: board_area.y,
        square_width: scale.square_width(),
        square_height: scale.square_height(),
        flipped: app.human_color == game::Color::Black,
    };

    let human = app.human_color;
    let unicode = app.unicode_pieces();
    let board = BoardWidget::new(app, geometry);
    let top = CapturesWidget::new(app, human.opponent());
    let bottom = CapturesWidget::new(app, human);
    let mut sprites = board.sprites();
    sprites.extend(top.sprites(top_captures));
    sprites.extend(bottom.sprites(bottom_captures));
    frame.render_widget(board, board_area);
    frame.render_widget(top, top_captures);
    frame.render_widget(bottom, bottom_captures);
    if let Some(images) = app.piece_images.as_mut() {
        for sprite in sprites {
            if let Some(image) = images.get(
                sprite.graphic,
                sprite.background,
                sprite.square,
                sprite.crop,
            ) {
                frame.render_widget(Image::new(image), sprite.area);
            }
        }
    }
    board::draw_coordinates(frame.buffer_mut(), geometry);
    frame.render_widget(panel::GamePanel::new(app), info_area);

    let mut buttons = vec![];
    match app.mode {
        Mode::Promotion { choice, .. } => {
            popup::promotion(frame, board_area, choice, human, unicode, &mut buttons);
        }
        Mode::NewGame { choice } => popup::new_game(frame, board_area, choice, &mut buttons),
        Mode::Playing if app.show_game_over_popup() => {
            popup::game_over(
                frame,
                board_area,
                app,
                &panel::status_text(app),
                &mut buttons,
            );
        }
        Mode::Playing => {}
    }

    app.hitboxes = Hitboxes {
        board: Some(geometry),
        buttons,
    };
}
