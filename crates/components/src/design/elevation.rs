//! Elevation: the shadow roles of the theme.
//!
//! Harbour separates materials by tone, not borders. Flat surfaces (cards)
//! have no shadow; raised and overlay surfaces cast a soft drop shadow. The
//! geometry of each level is a fixed scale, while its colours are palette
//! entries, so a theme file that overrides `black-a55` darkens every shadow
//! that uses it.
//!
//! Resolved shadows live on the theme as [`Theme::shadow`]; read them from
//! there rather than from [`Elevation`] directly.
//!
//! [`Theme::shadow`]: super::theme::Theme::shadow

use egui::{Shadow, Stroke};

use super::palette::PaletteKey;

/// Flat surfaces: tone only, no shadow and no border.
pub const FLAT: Shadow = Shadow::NONE;

/// A raised elevation level, mirroring a CSS `--shadow-*` token.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Elevation {
    /// Popovers, menus, toasts and floating toolbars.
    Raised,
    /// Blades, dialogs and the command palette.
    Overlay,
    /// Tooltips.
    Tooltip,
    /// A row or tile being dragged.
    Drag,
}

/// Unresolved definition of an elevation level: fixed geometry plus palette
/// references for its colours.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShadowSpec {
    /// Vertical offset in points.
    pub offset_y: i8,
    /// Blur width in points.
    pub blur: u8,
    /// Colour of the drop shadow.
    pub color: PaletteKey,
    /// Colour of a 1 point hairline ring around the surface (CSS
    /// `0 0 0 1px …`), if the level has one.
    pub ring: Option<PaletteKey>,
}

/// A resolved elevation level.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowRole {
    /// The drop shadow, ready for `Frame::shadow` or `Visuals::popup_shadow`.
    pub shadow: Shadow,
    /// The hairline ring drawn just outside the surface, if any.
    pub ring: Option<Stroke>,
}

/// How much wider egui's blur must be than the CSS blur radius.
///
/// CSS blurs a shadow with a Gaussian of standard deviation `blur / 2`; egui
/// fades it linearly over `blur` points centred on the edge. A linear ramp
/// this much wider matches the Gaussian's 10–90 % transition, measured on
/// the calibration board (`docs/design-oracles/harbour/src/CAL.html`).
pub const CSS_BLUR_TO_EGUI: f32 = 1.5;

/// The egui blur width for a CSS blur radius.
fn egui_blur(css_blur: u8) -> u8 {
    (f32::from(css_blur) * CSS_BLUR_TO_EGUI)
        .round()
        .min(f32::from(u8::MAX)) as u8
}

/// Width of the hairline ring of raised and overlay surfaces.
pub const RING_WIDTH: f32 = 1.0;

impl Elevation {
    /// Every raised level, in design order.
    pub const ALL: [Self; 4] = [Self::Raised, Self::Overlay, Self::Tooltip, Self::Drag];

    /// The CSS token name, for example `"shadow-raised"`.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Raised => "shadow-raised",
            Self::Overlay => "shadow-overlay",
            Self::Tooltip => "shadow-tooltip",
            Self::Drag => "shadow-drag",
        }
    }

    /// The level's geometry and palette references.
    pub const fn spec(self) -> ShadowSpec {
        match self {
            Self::Raised => ShadowSpec {
                offset_y: 24,
                blur: 60,
                color: PaletteKey::BlackA55,
                ring: Some(PaletteKey::WhiteA04),
            },
            Self::Overlay => ShadowSpec {
                offset_y: 30,
                blur: 70,
                color: PaletteKey::BlackA55,
                ring: Some(PaletteKey::WhiteA04),
            },
            Self::Tooltip => ShadowSpec {
                offset_y: 8,
                blur: 24,
                color: PaletteKey::BlackA45,
                ring: None,
            },
            Self::Drag => ShadowSpec {
                offset_y: 16,
                blur: 40,
                color: PaletteKey::BlackA55,
                ring: None,
            },
        }
    }
}

impl ShadowSpec {
    /// Resolve the spec against a palette lookup.
    pub fn resolve(self, palette: impl Fn(PaletteKey) -> egui::Color32) -> ShadowRole {
        ShadowRole {
            shadow: Shadow {
                offset: [0, self.offset_y],
                blur: egui_blur(self.blur),
                spread: 0,
                color: palette(self.color),
            },
            ring: self.ring.map(|ring| Stroke::new(RING_WIDTH, palette(ring))),
        }
    }
}

impl ShadowRole {
    /// The ring as a frame stroke: the CSS `0 0 0 1px` ring for levels that
    /// have one, otherwise a transparent stroke of the same width, so a frame
    /// keeps its size whichever level it uses.
    pub fn outline(&self) -> Stroke {
        self.ring
            .unwrap_or(Stroke::new(RING_WIDTH, egui::Color32::TRANSPARENT))
    }

    /// Paint this elevation behind a surface occupying `rect` with
    /// `corner_radius`: the hairline ring just outside the surface, then the
    /// drop shadow over it (CSS paints the first `box-shadow` on top). Paint
    /// the surface itself afterwards.
    pub fn paint(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        corner_radius: impl Into<egui::CornerRadius>,
    ) {
        let corner_radius = corner_radius.into();
        if let Some(ring) = self.ring {
            painter.rect_stroke(rect, corner_radius, ring, egui::StrokeKind::Outside);
        }
        painter.add(self.shadow.as_shape(rect, corner_radius));
    }
}
