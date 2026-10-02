use std::collections::HashMap;

use game::{Color, Piece, PieceKind};
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage, imageops};
use ratatui::layout::Size;
use ratatui_image::{Resize, picker::Picker, protocol::Protocol};

/// Piece height as a fraction of the square's shorter side.
const PIECE_FILL: f32 = 0.9;

const ARTWORK: [(Color, PieceKind, &[u8]); 12] = [
    (
        Color::White,
        PieceKind::King,
        include_bytes!("../assets/pieces/wK.png"),
    ),
    (
        Color::White,
        PieceKind::Queen,
        include_bytes!("../assets/pieces/wQ.png"),
    ),
    (
        Color::White,
        PieceKind::Rook,
        include_bytes!("../assets/pieces/wR.png"),
    ),
    (
        Color::White,
        PieceKind::Bishop,
        include_bytes!("../assets/pieces/wB.png"),
    ),
    (
        Color::White,
        PieceKind::Knight,
        include_bytes!("../assets/pieces/wN.png"),
    ),
    (
        Color::White,
        PieceKind::Pawn,
        include_bytes!("../assets/pieces/wP.png"),
    ),
    (
        Color::Black,
        PieceKind::King,
        include_bytes!("../assets/pieces/bK.png"),
    ),
    (
        Color::Black,
        PieceKind::Queen,
        include_bytes!("../assets/pieces/bQ.png"),
    ),
    (
        Color::Black,
        PieceKind::Rook,
        include_bytes!("../assets/pieces/bR.png"),
    ),
    (
        Color::Black,
        PieceKind::Bishop,
        include_bytes!("../assets/pieces/bB.png"),
    ),
    (
        Color::Black,
        PieceKind::Knight,
        include_bytes!("../assets/pieces/bN.png"),
    ),
    (
        Color::Black,
        PieceKind::Pawn,
        include_bytes!("../assets/pieces/bP.png"),
    ),
];

/// Piece artwork, encoded for the terminal's graphics protocol on demand.
pub struct PieceImages {
    picker: Picker,
    artwork: HashMap<Piece, RgbaImage>,
    /// Artwork scaled to a side length in pixels.
    scaled: HashMap<(Piece, u32), RgbaImage>,
    /// Encoded images must outlive a single frame: the kitty protocol transmits each image once
    /// and afterwards only refers to it. Only a handful of sizes are ever used (one per board
    /// scale, plus the captured-piece trays), so nothing is evicted.
    cache: HashMap<(Piece, [u8; 3], Size), Protocol>,
}

impl PieceImages {
    pub fn new(picker: Picker) -> Result<Self, image::ImageError> {
        let artwork = ARTWORK
            .iter()
            .map(|&(color, kind, png)| {
                let image = image::load_from_memory_with_format(png, ImageFormat::Png)?;
                Ok((Piece { color, kind }, image.into_rgba8()))
            })
            .collect::<Result<_, image::ImageError>>()?;
        Ok(Self {
            picker,
            artwork,
            scaled: HashMap::new(),
            cache: HashMap::new(),
        })
    }

    /// Whether the terminal can show real images rather than half-block approximations.
    pub fn has_graphics(&self) -> bool {
        self.picker.protocol_type() != ratatui_image::picker::ProtocolType::Halfblocks
    }

    /// `piece` drawn on a square of `square` cells filled with `background`.
    pub fn get(&mut self, piece: Piece, background: [u8; 3], square: Size) -> Option<&Protocol> {
        let key = (piece, background, square);
        if !self.cache.contains_key(&key) {
            let image = self.compose(piece, background, square);
            let protocol = self
                .picker
                .new_protocol(image, square, Resize::Fit(None))
                .ok()?;
            self.cache.insert(key, protocol);
        }
        self.cache.get(&key)
    }

    /// Paints the piece onto an opaque square so it looks the same whatever the terminal does
    /// with transparency.
    fn compose(&mut self, piece: Piece, [r, g, b]: [u8; 3], square: Size) -> DynamicImage {
        let font = self.picker.font_size();
        let width = u32::from(square.width) * u32::from(font.width);
        let height = u32::from(square.height) * u32::from(font.height);
        let mut canvas = RgbaImage::from_pixel(width, height, Rgba([r, g, b, 255]));

        let side = (width.min(height) as f32 * PIECE_FILL) as u32;
        if side > 0 {
            let artwork = &self.artwork;
            let art = self
                .scaled
                .entry((piece, side))
                .or_insert_with(|| imageops::thumbnail(&artwork[&piece], side, side));
            let x = i64::from((width - side) / 2);
            let y = i64::from((height - side) / 2);
            imageops::overlay(&mut canvas, art, x, y);
        }
        DynamicImage::ImageRgba8(canvas)
    }
}
