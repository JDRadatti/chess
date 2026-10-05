use std::collections::HashMap;

use game::{Color, Piece, PieceKind};
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage, imageops};
use ratatui::layout::{Rect, Size};
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

/// Dot radius as a fraction of the square's shorter side.
const DOT_RADIUS: f32 = 0.16;

/// What to paint on a square.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Graphic {
    Piece(Piece),
    /// A round marker for a square the selected piece can move to.
    Dot {
        color: [u8; 3],
    },
}

/// Piece artwork, encoded for the terminal's graphics protocol on demand.
pub struct PieceImages {
    picker: Picker,
    artwork: HashMap<Piece, RgbaImage>,
    /// Artwork scaled to a side length in pixels.
    scaled: HashMap<(Piece, u32), RgbaImage>,
    /// Encoded images must outlive a single frame: the kitty protocol transmits each image once
    /// and afterwards only refers to it. Only a handful of sizes are ever used (one per board
    /// scale, plus the captured-piece trays), so nothing is evicted.
    cache: HashMap<(Graphic, [u8; 3], Size, Rect), Protocol>,
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

    /// The `crop` cells of `graphic` drawn centered on a square of `square` cells filled with
    /// `background`.
    pub fn get(
        &mut self,
        graphic: Graphic,
        background: [u8; 3],
        square: Size,
        crop: Rect,
    ) -> Option<&Protocol> {
        let key = (graphic, background, square, crop);
        if !self.cache.contains_key(&key) {
            let mut image = self.compose(graphic, background, square);
            if crop.as_size() != square {
                let font = self.picker.font_size();
                let (fw, fh) = (u32::from(font.width), u32::from(font.height));
                image = image.crop_imm(
                    u32::from(crop.x) * fw,
                    u32::from(crop.y) * fh,
                    u32::from(crop.width) * fw,
                    u32::from(crop.height) * fh,
                );
            }
            let protocol = self
                .picker
                .new_protocol(image, crop.as_size(), Resize::Fit(None))
                .ok()?;
            self.cache.insert(key, protocol);
        }
        self.cache.get(&key)
    }

    /// Forgets encoded images so they are sent to the terminal again, e.g. after it was cleared.
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    /// Paints the graphic onto an opaque square so it looks the same whatever the terminal does
    /// with transparency.
    #[expect(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "image sizes are a few hundred pixels"
    )]
    fn compose(&mut self, graphic: Graphic, [r, g, b]: [u8; 3], square: Size) -> DynamicImage {
        let font = self.picker.font_size();
        let width = u32::from(square.width) * u32::from(font.width);
        let height = u32::from(square.height) * u32::from(font.height);
        let mut canvas = RgbaImage::from_pixel(width, height, Rgba([r, g, b, 255]));

        match graphic {
            Graphic::Piece(piece) => {
                let side = (width.min(height) as f32 * PIECE_FILL) as u32;
                if side > 0 {
                    let artwork = &self.artwork;
                    let art = self
                        .scaled
                        .entry((piece, side))
                        .or_insert_with(|| imageops::thumbnail(&artwork[&piece], side, side));
                    let left = i64::from((width - side) / 2);
                    let top = i64::from((height - side) / 2);
                    imageops::overlay(&mut canvas, art, left, top);
                }
            }
            Graphic::Dot { color } => {
                let radius = width.min(height) as f32 * DOT_RADIUS;
                draw_dot(&mut canvas, radius, color);
            }
        }
        DynamicImage::ImageRgba8(canvas)
    }
}

/// Anti-aliased filled circle in the middle of `canvas`.
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "image sizes are a few hundred pixels and each channel blends two u8 values"
)]
fn draw_dot(canvas: &mut RgbaImage, radius: f32, color: [u8; 3]) {
    let (cx, cy) = (canvas.width() as f32 / 2.0, canvas.height() as f32 / 2.0);
    for (x, y, pixel) in canvas.enumerate_pixels_mut() {
        let distance = (x as f32 + 0.5 - cx).hypot(y as f32 + 0.5 - cy);
        let coverage = (radius + 0.5 - distance).clamp(0.0, 1.0);
        if coverage > 0.0 {
            for (channel, &target) in pixel.0.iter_mut().zip(&color) {
                *channel = (f32::from(*channel) * (1.0 - coverage) + f32::from(target) * coverage)
                    .round() as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dot_is_centered() {
        let mut canvas = RgbaImage::from_pixel(90, 80, Rgba([200, 200, 200, 255]));
        draw_dot(&mut canvas, 12.0, [10, 20, 30]);
        assert_eq!(canvas.get_pixel(45, 40).0, [10, 20, 30, 255]);
        assert_eq!(canvas.get_pixel(0, 0).0, [200, 200, 200, 255]);
        // Same distance from the center in every direction.
        for (x, y) in [(45 - 13, 40), (44 + 13, 40), (45, 40 - 13), (45, 39 + 13)] {
            assert_eq!(canvas.get_pixel(x, y).0, [200, 200, 200, 255], "({x}, {y})");
        }
        for (x, y) in [(45 - 11, 40), (44 + 11, 40), (45, 40 - 11), (45, 39 + 11)] {
            assert_eq!(canvas.get_pixel(x, y).0, [10, 20, 30, 255], "({x}, {y})");
        }
    }
}
