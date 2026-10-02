use game::{Color, Game, Move, Position, Square};
use ratatui::{
    Terminal,
    backend::TestBackend,
    buffer::Buffer,
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind},
    style,
};

use ratatui_image::picker::{Picker, ProtocolType};

use crate::{
    app::{App, Button, ColorChoice, Mode, PieceStyle},
    pieces::PieceImages,
    ui,
    update::{update, update_mouse},
};

const CHECK_SQUARE: style::Color = style::Color::Rgb(210, 70, 60);

/// Computer that never moves, so tests control the position.
struct Idle;

impl game::Player for Idle {
    fn request_move(&mut self, _position: &Position) {}

    fn poll_move(&mut self) -> Option<Move> {
        None
    }
}

fn app_at(fen: &str, human: Color) -> App {
    let position: Position = fen.parse().unwrap();
    App::with_opponent(human, Game::from_position(position), Box::new(Idle))
}

fn draw(app: &mut App) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|frame| ui::render(app, frame)).unwrap();
    terminal.backend().buffer().clone()
}

fn text(buf: &Buffer) -> String {
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Terminal cell at the middle of `sq` in the last frame.
fn center_of(app: &App, sq: Square) -> (u16, u16) {
    let g = app.hitboxes.board.unwrap();
    let (row, col) = (0..8)
        .flat_map(|row| (0..8).map(move |col| (row, col)))
        .find(|&(row, col)| g.square_at_cell(row, col) == sq)
        .unwrap();
    (
        g.x + col * g.square_width + g.square_width / 2,
        g.y + row * g.square_height + g.square_height / 2,
    )
}

fn click(app: &mut App, (x, y): (u16, u16)) {
    update_mouse(
        app,
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        },
    );
}

fn click_square(app: &mut App, sq: Square) {
    draw(app);
    click(app, center_of(app, sq));
}

fn key(app: &mut App, code: KeyCode) {
    update(app, KeyEvent::new(code, KeyModifiers::NONE));
}

fn click_button(app: &mut App, button: Button) {
    draw(app);
    let (rect, _) = app
        .hitboxes
        .buttons
        .iter()
        .find(|(_, b)| *b == button)
        .unwrap();
    let corner = (rect.x, rect.y);
    click(app, corner);
}

const E2: Square = Square(52);
const E4: Square = Square(36);
const E1: Square = Square(60);
const A7: Square = Square(8);
const A8: Square = Square(0);
const H1: Square = Square(63);

#[test]
fn startup_opens_color_picker() {
    let mut app = App::new();
    let screen = text(&draw(&mut app));
    assert!(screen.contains("New game"));
    assert!(screen.contains("Random"));

    app.tick();
    assert!(app.san_history.is_empty(), "match is paused while picking");

    key(&mut app, KeyCode::Char('b'));
    assert_eq!(app.human_color, Color::Black);
    assert_eq!(app.mode, Mode::Playing);
    app.tick();
    assert_eq!(app.san_history.len(), 1, "computer opens as White");
}

#[test]
fn clicks_map_to_squares_in_both_orientations() {
    for human in [Color::White, Color::Black] {
        let mut app = App::with_opponent(human, Game::new(), Box::new(Idle));
        draw(&mut app);
        let g = app.hitboxes.board.unwrap();
        for sq in (0..64).map(Square) {
            let (x, y) = center_of(&app, sq);
            assert_eq!(g.square_at_point(x, y), Some(sq));
        }
        let a8 = center_of(&app, A8);
        let h1 = center_of(&app, H1);
        assert_eq!(
            a8 < h1,
            human == Color::White,
            "black sees the board flipped"
        );
    }
}

#[test]
fn click_to_move() {
    let mut app = App::with_opponent(Color::White, Game::new(), Box::new(Idle));
    click_square(&mut app, E2);
    assert_eq!(app.selected, Some(E2));
    click_square(&mut app, E4);
    app.tick();
    assert_eq!(app.san_history, ["e4"]);

    let buf = draw(&mut app);
    let (x, y) = center_of(&app, E4);
    assert_eq!(buf[(x, y)].symbol(), "♟");
    assert_eq!(buf[(x, y)].fg, style::Color::White);
}

#[test]
fn clicking_off_the_board_drops_the_piece() {
    let mut app = App::with_opponent(Color::White, Game::new(), Box::new(Idle));
    click_square(&mut app, E2);
    click(&mut app, (0, 0));
    assert_eq!(app.selected, None);
}

#[test]
fn king_in_check_is_highlighted() {
    let mut app = app_at("4k3/8/8/8/8/8/4r3/4K3 w - - 0 1", Color::White);
    app.cursor = A8;
    let buf = draw(&mut app);
    let (x, y) = center_of(&app, E1);
    assert_eq!(buf[(x, y)].bg, CHECK_SQUARE);
    assert!(text(&buf).contains("Check!"));
}

#[test]
fn game_over_popup() {
    // Fool's mate: White is checkmated.
    let fen = "rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3";
    let mut app = app_at(fen, Color::White);
    let screen = text(&draw(&mut app));
    assert!(screen.contains("Game over"));
    assert!(screen.contains("Checkmate, computer wins"));

    key(&mut app, KeyCode::Esc);
    let screen = text(&draw(&mut app));
    assert!(!screen.contains("Game over"));
    assert!(
        screen.contains("Checkmate, computer wins"),
        "status line remains"
    );
    assert!(!app.should_quit);

    let mut app = app_at(fen, Color::White);
    click_button(&mut app, Button::OpenNewGame);
    assert!(matches!(app.mode, Mode::NewGame { .. }));

    let mut app = app_at(fen, Color::White);
    click_button(&mut app, Button::Quit);
    assert!(app.should_quit);
}

#[test]
fn promotion_picker() {
    let fen = "8/P6k/8/8/8/8/8/K7 w - - 0 1";
    let mut app = app_at(fen, Color::White);
    click_square(&mut app, A7);
    click_square(&mut app, A8);
    assert!(matches!(app.mode, Mode::Promotion { .. }));
    assert!(text(&draw(&mut app)).contains("Promote to"));

    key(&mut app, KeyCode::Esc);
    assert_eq!(app.mode, Mode::Playing);
    app.tick();
    assert!(app.san_history.is_empty(), "cancelling plays nothing");

    click_square(&mut app, A8);
    click_button(&mut app, Button::Promotion(1));
    app.tick();
    assert_eq!(app.san_history, ["a8=R"]);

    let mut app = app_at(fen, Color::White);
    click_square(&mut app, A7);
    click_square(&mut app, A8);
    key(&mut app, KeyCode::Char('n'));
    app.tick();
    assert_eq!(app.san_history, ["a8=N"]);
}

#[test]
fn captured_pieces_and_material() {
    // Black is missing its queen.
    let mut app = app_at(
        "rnb1kbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        Color::White,
    );
    let buf = draw(&mut app);
    let g = app.hitboxes.board.unwrap();
    let below_board = g.y + 8 * g.square_height + 1;
    let row: String = (0..buf.area.width)
        .map(|x| buf[(x, below_board)].symbol())
        .collect();
    assert!(row.contains("♛  +9"), "{row}");

    let queen = &buf[(g.x, below_board)];
    assert_eq!(queen.symbol(), "♛", "tray starts under the a-file");
    assert_eq!(queen.fg, style::Color::Black);
    assert_ne!(queen.bg, style::Color::Reset, "tray has a background");

    key(&mut app, KeyCode::Char('g'));
    assert!(text(&draw(&mut app)).contains("q  +9"));
}

#[test]
fn image_pieces() {
    let mut app = App::with_opponent(Color::White, Game::new(), Box::new(Idle));
    let mut picker = Picker::halfblocks();
    picker.set_protocol_type(ProtocolType::Kitty);
    app.set_piece_images(PieceImages::new(picker).unwrap());
    assert_eq!(app.piece_style, PieceStyle::Images);

    key(&mut app, KeyCode::Char('g'));
    key(&mut app, KeyCode::Char('g'));
    key(&mut app, KeyCode::Char('g'));
    assert_eq!(
        app.piece_style,
        PieceStyle::Images,
        "g cycles back to images"
    );
    let buf = draw(&mut app);
    assert!(!text(&buf).contains('♟'), "pieces are drawn as images");
    let (x, y) = center_of(&app, E2);
    assert!(buf[(x, y)].symbol() != " ", "image covers the square");

    app.start_new_game(ColorChoice::White);
    assert!(app.image_pieces(), "style survives a new game");

    let mut picker = Picker::halfblocks();
    picker.set_protocol_type(ProtocolType::Kitty);
    let mut app = app_at(
        "rnb1kbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        Color::White,
    );
    app.set_piece_images(PieceImages::new(picker).unwrap());
    let buf = draw(&mut app);
    let g = app.hitboxes.board.unwrap();
    let tray = &buf[(g.x, g.y + 8 * g.square_height + 1)];
    assert!(
        !matches!(tray.symbol(), " " | "♛"),
        "captured queen is an image"
    );
}

#[test]
fn no_image_pieces_without_graphics() {
    let mut app = App::with_opponent(Color::White, Game::new(), Box::new(Idle));
    app.set_piece_images(PieceImages::new(Picker::halfblocks()).unwrap());
    assert_eq!(app.piece_style, PieceStyle::Symbols);
    key(&mut app, KeyCode::Char('g'));
    key(&mut app, KeyCode::Char('g'));
    assert_eq!(app.piece_style, PieceStyle::Symbols, "g skips images");
}

#[test]
fn tiny_terminal() {
    let mut app = App::with_opponent(Color::White, Game::new(), Box::new(Idle));
    let mut terminal = Terminal::new(TestBackend::new(60, 8)).unwrap();
    terminal.draw(|frame| ui::render(&mut app, frame)).unwrap();
    assert!(text(terminal.backend().buffer()).contains("Terminal too small"));
    assert!(app.hitboxes.board.is_none());
}
