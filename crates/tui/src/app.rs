use std::{
    sync::mpsc::{self, Sender},
    time::{SystemTime, UNIX_EPOCH},
};

use game::{
    Color, Game, Match, Move, PieceKind, Player, Square, Status,
    engine::{EngineInfo, EnginePlayer, Level},
};
use ratatui::layout::Rect;

use crate::{human::HumanPlayer, pieces::PieceImages};

/// Promotion picker options, in display order.
pub const PROMOTION_CHOICES: [PieceKind; 4] = [
    PieceKind::Queen,
    PieceKind::Rook,
    PieceKind::Bishop,
    PieceKind::Knight,
];

/// New-game picker options, in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorChoice {
    White,
    Black,
    Random,
}

/// A row of the new-game picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewGameOption {
    Color(ColorChoice),
    Level(Level),
}

/// New-game picker rows, in display order.
pub const NEW_GAME_OPTIONS: [NewGameOption; 7] = [
    NewGameOption::Color(ColorChoice::White),
    NewGameOption::Color(ColorChoice::Black),
    NewGameOption::Color(ColorChoice::Random),
    NewGameOption::Level(Level::Beginner),
    NewGameOption::Level(Level::Easy),
    NewGameOption::Level(Level::Medium),
    NewGameOption::Level(Level::Hard),
];

pub const DEFAULT_LEVEL: Level = Level::Easy;

/// What the keyboard and mouse are currently driving.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Playing,
    /// A pawn is about to promote; waiting for the piece choice.
    Promotion {
        src: Square,
        dst: Square,
        choice: usize,
    },
    /// Choosing a color and an opponent for a new game.
    NewGame {
        color: ColorChoice,
        level: Level,
        /// Index into [`NEW_GAME_OPTIONS`] of the row under the keyboard cursor.
        focus: usize,
    },
}

/// How pieces are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PieceStyle {
    /// Pictures, through the terminal's graphics protocol.
    Images,
    /// Unicode chess symbols.
    Symbols,
    /// FEN letters.
    Letters,
}

/// Something clickable outside the board.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Promotion(usize),
    /// A row of the new-game picker, by index into [`NEW_GAME_OPTIONS`].
    NewGameOption(usize),
    StartGame,
    OpenNewGame,
    Quit,
}

/// Where the board was drawn, so clicks can be mapped back to squares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoardGeometry {
    /// Top-left cell of the a8 square (h1 when flipped).
    pub x: u16,
    pub y: u16,
    pub square_width: u16,
    pub square_height: u16,
    pub flipped: bool,
}

impl BoardGeometry {
    /// Board square drawn at screen row/column `(row, col)`, both 0..8 from the top left.
    #[expect(clippy::cast_possible_truncation, reason = "row and col are below 8")]
    pub fn square_at_cell(&self, row: u16, col: u16) -> Square {
        let (row, col) = if self.flipped {
            (7 - row, 7 - col)
        } else {
            (row, col)
        };
        Square((row * 8 + col) as u8)
    }

    /// Board square under the terminal cell `(x, y)`, if any.
    pub fn square_at_point(&self, x: u16, y: u16) -> Option<Square> {
        let col = x.checked_sub(self.x)? / self.square_width;
        let row = y.checked_sub(self.y)? / self.square_height;
        (col < 8 && row < 8).then(|| self.square_at_cell(row, col))
    }
}

/// Clickable regions from the last frame.
#[derive(Debug, Default, Clone)]
pub struct Hitboxes {
    pub board: Option<BoardGeometry>,
    pub buttons: Vec<(Rect, Button)>,
}

impl Hitboxes {
    pub fn button_at(&self, x: u16, y: u16) -> Option<Button> {
        self.buttons
            .iter()
            .find(|(rect, _)| rect.contains((x, y).into()))
            .map(|(_, button)| *button)
    }
}

/// Application.
pub struct App {
    /// should the application exit?
    pub should_quit: bool,
    pub game_match: Match,
    pub human_color: Color,
    /// What was picked for this game, offered again by the next new-game picker.
    pub color_choice: ColorChoice,
    pub level: Level,
    /// Progress of the computer's search, when the computer is the engine.
    pub engine_info: Option<EngineInfo>,
    /// Moves chosen on the board are sent here and picked up by the [`HumanPlayer`].
    human_moves: Sender<Move>,
    /// Square under the keyboard cursor.
    pub cursor: Square,
    /// Square of the piece the human picked up, if any.
    pub selected: Option<Square>,
    /// Moves played so far, in SAN.
    pub san_history: Vec<String>,
    pub mode: Mode,
    /// The game-over popup was closed to look at the final position.
    pub game_over_dismissed: bool,
    pub piece_style: PieceStyle,
    /// Present when the terminal was queried for image support.
    pub piece_images: Option<PieceImages>,
    /// Filled in by the renderer each frame.
    pub hitboxes: Hitboxes,
    /// Clear the terminal and redraw everything on the next frame.
    pub needs_full_redraw: bool,
}

impl App {
    /// Starts with the new-game picker open.
    pub fn new() -> Self {
        let mut app = Self::with_engine(Color::White, DEFAULT_LEVEL);
        app.open_new_game();
        app
    }

    /// A new game against the engine at `level`.
    pub fn with_engine(human_color: Color, level: Level) -> Self {
        let engine = EnginePlayer::with_level(level);
        let engine_info = engine.info();
        Self {
            level,
            engine_info: Some(engine_info),
            ..Self::with_opponent(human_color, Game::new(), Box::new(engine))
        }
    }

    /// Continues `game` with the human playing `human_color` against `computer`.
    pub fn with_opponent(human_color: Color, game: Game, computer: Box<dyn Player>) -> Self {
        let (human_moves, receiver) = mpsc::channel();
        let human = Box::new(HumanPlayer::new(receiver));
        let game_match = match human_color {
            Color::White => Match::from_game(game, human, computer),
            Color::Black => Match::from_game(game, computer, human),
        };
        Self {
            should_quit: false,
            game_match,
            human_color,
            color_choice: match human_color {
                Color::White => ColorChoice::White,
                Color::Black => ColorChoice::Black,
            },
            level: DEFAULT_LEVEL,
            engine_info: None,
            human_moves,
            cursor: match human_color {
                Color::White => Square(52), // e2
                Color::Black => Square(12), // e7
            },
            selected: None,
            san_history: vec![],
            mode: Mode::Playing,
            game_over_dismissed: false,
            piece_style: PieceStyle::Symbols,
            piece_images: None,
            hitboxes: Hitboxes::default(),
            needs_full_redraw: false,
        }
    }

    /// Advances the match; called after every terminal event. Paused while choosing a new game.
    pub fn tick(&mut self) {
        if matches!(self.mode, Mode::NewGame { .. }) {
            return;
        }
        // Keep going after a move so the next player is asked for theirs right away.
        loop {
            let before = *self.game_match.game().position();
            match self.game_match.tick() {
                Ok(Some(m)) => self.san_history.push(before.to_san(&m)),
                _ => break,
            }
        }
    }

    /// Set `should_quit` to true to quit the application.
    pub fn quit(&mut self) {
        self.should_quit = true;
    }

    pub fn is_human_turn(&self) -> bool {
        let game = self.game_match.game();
        game.status() == Status::Ongoing && game.position().side_to_move() == self.human_color
    }

    pub fn is_game_over(&self) -> bool {
        self.game_match.game().status() != Status::Ongoing
    }

    pub fn show_game_over_popup(&self) -> bool {
        self.mode == Mode::Playing && self.is_game_over() && !self.game_over_dismissed
    }

    /// Moves the cursor by whole ranks/files in screen directions, clamped to the board.
    /// Positive `up` moves toward the top of the screen.
    pub fn move_cursor(&mut self, up: i8, right: i8) {
        let (up, right) = if self.human_color == Color::Black {
            (-up, -right)
        } else {
            (up, right)
        };
        let row = (self.cursor.0 / 8)
            .saturating_add_signed(up.saturating_neg())
            .min(7);
        let file = (self.cursor.0 % 8).saturating_add_signed(right).min(7);
        self.cursor = Square(row * 8 + file);
    }

    /// Picks up the piece under the cursor, or drops the selected piece there.
    pub fn select(&mut self) {
        if !self.is_human_turn() {
            return;
        }
        let position = *self.game_match.game().position();

        if let Some(src) = self.selected {
            let candidates: Vec<Move> = position
                .legal_moves_from(src)
                .into_iter()
                .filter(|m| m.dst() == self.cursor.0)
                .collect();
            match candidates.as_slice() {
                [] => {}
                [m] => {
                    self.send_move(*m);
                    return;
                }
                _ => {
                    self.mode = Mode::Promotion {
                        src,
                        dst: self.cursor,
                        choice: 0,
                    };
                    return;
                }
            }
        }

        self.selected = if self.selected != Some(self.cursor)
            && !position.legal_moves_from(self.cursor).is_empty()
        {
            Some(self.cursor)
        } else {
            None
        };
    }

    /// Selects `sq` as if the cursor were moved there and enter pressed.
    pub fn click_square(&mut self, sq: Square) {
        self.cursor = sq;
        self.select();
    }

    pub fn cancel_selection(&mut self) -> bool {
        self.selected.take().is_some()
    }

    pub fn move_choice(&mut self, delta: isize) {
        match &mut self.mode {
            Mode::Promotion { choice, .. } => {
                *choice = wrap(*choice, delta, PROMOTION_CHOICES.len());
            }
            Mode::NewGame { focus, .. } => {
                let row = wrap(*focus, delta, NEW_GAME_OPTIONS.len());
                self.choose(NEW_GAME_OPTIONS[row]);
            }
            Mode::Playing => {}
        }
    }

    /// Picks a color or level in the new-game picker and moves the cursor to it.
    pub fn choose(&mut self, option: NewGameOption) {
        let Mode::NewGame {
            color,
            level,
            focus,
        } = &mut self.mode
        else {
            return;
        };
        match option {
            NewGameOption::Color(choice) => *color = choice,
            NewGameOption::Level(choice) => *level = choice,
        }
        *focus = NEW_GAME_OPTIONS
            .iter()
            .position(|o| *o == option)
            .unwrap_or(0);
    }

    /// Confirms the highlighted option in the open picker.
    pub fn confirm_choice(&mut self) {
        match self.mode {
            Mode::Promotion { choice, .. } => self.promote(PROMOTION_CHOICES[choice]),
            Mode::NewGame { color, level, .. } => self.start_new_game(color, level),
            Mode::Playing => {}
        }
    }

    pub fn promote(&mut self, kind: PieceKind) {
        let Mode::Promotion { src, dst, .. } = self.mode else {
            return;
        };
        let chosen = self
            .game_match
            .game()
            .position()
            .legal_moves_from(src)
            .into_iter()
            .find(|m| m.dst() == dst.0 && m.promotion() == Some(kind));
        self.mode = Mode::Playing;
        if let Some(m) = chosen {
            self.send_move(m);
        }
    }

    pub fn open_new_game(&mut self) {
        self.selected = None;
        self.mode = Mode::NewGame {
            color: self.color_choice,
            level: self.level,
            focus: 0,
        };
        self.choose(NewGameOption::Color(self.color_choice));
    }

    /// Closes the open picker or popup without acting on it.
    pub fn close_popup(&mut self) {
        match self.mode {
            Mode::Promotion { .. } | Mode::NewGame { .. } => self.mode = Mode::Playing,
            Mode::Playing => self.game_over_dismissed = true,
        }
    }

    pub fn start_new_game(&mut self, choice: ColorChoice, level: Level) {
        let color = match choice {
            ColorChoice::White => Color::White,
            ColorChoice::Black => Color::Black,
            ColorChoice::Random => random_color(),
        };
        *self = Self {
            color_choice: choice,
            piece_style: self.piece_style,
            piece_images: self.piece_images.take(),
            ..Self::with_engine(color, level)
        };
    }

    /// Switches to image pieces, unless the terminal can only approximate images with
    /// half-block characters, which is too coarse for pieces.
    pub fn set_piece_images(&mut self, images: PieceImages) {
        if images.has_graphics() {
            self.piece_style = PieceStyle::Images;
            self.piece_images = Some(images);
        }
    }

    /// Cycles images, symbols, letters; images are skipped when unavailable.
    pub fn toggle_piece_style(&mut self) {
        self.piece_style = match self.piece_style {
            PieceStyle::Symbols => PieceStyle::Letters,
            PieceStyle::Letters if self.piece_images.is_some() => PieceStyle::Images,
            PieceStyle::Images | PieceStyle::Letters => PieceStyle::Symbols,
        };
        self.request_full_redraw();
    }

    /// Repaints the whole screen on the next frame, so nothing a terminal left behind (such as
    /// image fragments) survives. Images are sent again because clearing may delete them.
    pub fn request_full_redraw(&mut self) {
        self.needs_full_redraw = true;
        if let Some(images) = self.piece_images.as_mut() {
            images.clear_cache();
        }
    }

    /// Whether text such as captured pieces should use chess symbols rather than letters.
    pub fn unicode_pieces(&self) -> bool {
        self.piece_style != PieceStyle::Letters
    }

    /// Whether the board should be drawn with images.
    pub fn image_pieces(&self) -> bool {
        self.piece_style == PieceStyle::Images && self.piece_images.is_some()
    }

    fn send_move(&mut self, m: Move) {
        self.selected = None;
        let _ = self.human_moves.send(m);
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

fn wrap(index: usize, delta: isize, len: usize) -> usize {
    let step = delta.unsigned_abs() % len;
    if delta < 0 {
        (index + len - step) % len
    } else {
        (index + step) % len
    }
}

fn random_color() -> Color {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    if nanos.is_multiple_of(2) {
        Color::White
    } else {
        Color::Black
    }
}
