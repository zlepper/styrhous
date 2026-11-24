//! Icon tiles (`.tile`): a rounded square holding one icon, leading list
//! rows, window headers, dialogs and empty states (C11, C12, C13, C14).

use egui::{Color32, Rect, Response, Sense, Ui, Vec2};

use crate::design::theme::{Theme, ThemeExt, Tone};
use crate::design::{radius, size};
use crate::icons::Icon;

/// The size of a [`Tile`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TileSize {
    /// 28 points with a 16 point glyph (`.tile.sm`): window headers.
    Sm,
    /// 36 points with a 20 point glyph (`.tile`): list rows.
    #[default]
    Md,
    /// 40 points with a 20 point glyph (`.tile.lg`).
    Lg,
}

impl TileSize {
    /// The tile's side.
    pub const fn side(self) -> f32 {
        match self {
            Self::Sm => size::TILE_SM,
            Self::Md => size::TILE,
            Self::Lg => size::TILE_LG,
        }
    }

    const fn glyph(self) -> f32 {
        match self {
            Self::Sm => size::ICON,
            Self::Md | Self::Lg => size::ICON_LG,
        }
    }

    const fn radius(self) -> u8 {
        match self {
            Self::Sm => radius::TILE_SM,
            Self::Md => radius::TILE,
            Self::Lg => radius::TILE_LG,
        }
    }
}

/// An icon tile: `icon` centred on a rounded square in `tone`
/// ([`tile_colors`]: tinted for accent and info, solid otherwise).
///
/// A tile is decorative: put the words it stands for next to it.
#[must_use = "call `show` or `paint` to draw the tile"]
#[derive(Clone, Copy, Debug)]
pub struct Tile {
    icon: Icon,
    tone: Tone,
    size: TileSize,
    tinted: bool,
}

impl Tile {
    /// An accent tile of [`TileSize::Md`].
    pub fn new(icon: Icon) -> Self {
        Self {
            icon,
            tone: Tone::Accent,
            size: TileSize::Md,
            tinted: false,
        }
    }

    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    /// Tint the tile in its tone with a coloured glyph, whatever the tone
    /// (a danger tile on a code surface).
    pub fn tinted(mut self) -> Self {
        self.tinted = true;
        self
    }

    pub fn size(mut self, size: TileSize) -> Self {
        self.size = size;
        self
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let (rect, response) =
            ui.allocate_exact_size(Vec2::splat(self.size.side()), Sense::hover());
        self.paint(ui, rect);
        response
    }

    /// Paint the tile into `rect` (normally [`TileSize::side`] square).
    pub fn paint(self, ui: &Ui, rect: Rect) {
        if !ui.is_rect_visible(rect) {
            return;
        }
        let theme = ui.tokens();
        let (fill, glyph) = if self.tinted {
            let colors = theme.tone(self.tone);
            (colors.tint, colors.text)
        } else {
            tile_colors(&theme, self.tone)
        };
        ui.painter().rect_filled(rect, self.size.radius(), fill);
        let glyph_rect = Rect::from_center_size(rect.center(), Vec2::splat(self.size.glyph()));
        self.icon.paint(ui, glyph_rect, glyph);
    }
}

/// The fill and glyph colours of an icon tile in `tone` (the design's
/// `.tile` variants): accent and info tiles are tinted with a coloured glyph
/// (`.tile`); neutral (`.tile.gray`) and status tiles (`.tile.ok`,
/// `.tile.warn`, `.tile.bad`) are solid ([`Theme::solid`]).
fn tile_colors(theme: &Theme, tone: Tone) -> (Color32, Color32) {
    match tone {
        Tone::Accent | Tone::Info => {
            let colors = theme.tone(tone);
            (colors.tint, colors.text)
        }
        tone => theme.solid(tone),
    }
}
