//! Cluster avatars (`.av`): rounded squares with one or two initials, shared
//! by the cluster rail, settings and previews.
//!
//! A cluster avatar carries its connection state as a status square in its
//! bottom-right corner, cut out of the avatar by a border in the colour of
//! the surface behind it. The selected avatar has an accent ring with a gap;
//! a hovered one brightens by 10 %.

use crate::design::palette;
use crate::design::theme::{Theme, ThemeExt, Tone};
use crate::design::type_scale::{self, TextRole};
use crate::design::{radius, size, stroke};
use std::sync::Arc;

use egui::{Align2, Color32, CornerRadius, Galley, Pos2, Rect, Response, Ui, Vec2};

/// The widest initials may be, relative to the avatar, before they shrink
/// (wide pairs such as WW).
const INITIALS_WIDTH_RATIO: f32 = 0.75;

/// How much a hovered avatar brightens (CSS `filter: brightness(1.1)`).
const HOVER_BRIGHTNESS: f32 = 1.1;

/// The colour treatment of an [`Avatar`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AvatarTone {
    /// A quiet well, for avatars that are not identities.
    Neutral,
    /// A category colour chosen from the text, so each identity keeps its colour.
    Identity,
    /// The category colour `av-(n + 1)`.
    Category(usize),
    /// The accent tint, for the current selection.
    Accent,
}

#[must_use = "call `show` or `paint` to draw the avatar"]
pub struct Avatar<'a> {
    text: &'a str,
    size: f32,
    tone: AvatarTone,
    role: TextRole,
    status: Option<Tone>,
    selected: bool,
    hovered: bool,
    surface: Option<Color32>,
}

impl<'a> Avatar<'a> {
    /// A neutral [`size::AVATAR`] avatar showing `text` (one or two
    /// characters).
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            size: size::AVATAR,
            tone: AvatarTone::Neutral,
            role: type_scale::AVATAR,
            status: None,
            selected: false,
            hovered: false,
            surface: None,
        }
    }

    /// Use the identity treatment of navigation chrome: a category colour
    /// (`av-1` to `av-6`) picked deterministically from the avatar text.
    pub fn identity(mut self) -> Self {
        self.tone = AvatarTone::Identity;
        self
    }

    /// Use the category colour `av-(index + 1)`, wrapping after six.
    pub fn category(mut self, index: usize) -> Self {
        self.tone = AvatarTone::Category(index);
        self
    }

    pub fn accent(mut self) -> Self {
        self.tone = AvatarTone::Accent;
        self
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    /// The type role of the initials (default [`type_scale::AVATAR`]).
    pub fn font(mut self, role: TextRole) -> Self {
        self.role = role;
        self
    }

    /// Show the connection state as a status square in `tone`: success when
    /// connected, warning while connecting, danger on error, neutral when
    /// idle.
    pub fn status(mut self, tone: impl Into<Option<Tone>>) -> Self {
        self.status = tone.into();
        self
    }

    /// Draw the selected ring.
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Draw the hovered (brightened) state.
    pub fn hovered(mut self, hovered: bool) -> Self {
        self.hovered = hovered;
        self
    }

    /// The colour of the surface behind the avatar, which the status border
    /// and the ring gap are painted in (default `nav-rail`).
    pub fn surface(mut self, surface: Color32) -> Self {
        self.surface = Some(surface);
        self
    }

    /// The fill and text colours of this avatar in `theme`.
    fn colors(&self, theme: &Theme) -> (Color32, Color32) {
        let avatars = theme.components.avatars();
        match self.tone {
            AvatarTone::Neutral => (theme.components.well_bg, theme.text.secondary),
            AvatarTone::Identity => (
                avatars[identity_index(self.text, avatars.len())],
                theme.components.avatar_text,
            ),
            AvatarTone::Category(index) => {
                (avatars[index % avatars.len()], theme.components.avatar_text)
            }
            AvatarTone::Accent => (theme.accent.tint, theme.accent.text),
        }
    }

    /// The corner radius for this avatar's size: [`radius::AVATAR`] at
    /// [`size::AVATAR`], scaled with the size.
    fn corner_radius(&self) -> CornerRadius {
        let scaled = f32::from(radius::AVATAR) * self.size / size::AVATAR;
        CornerRadius::same(scaled.round() as u8)
    }

    /// Paint the avatar centred on `center`. The ring and status square
    /// extend past its square.
    pub fn paint(&self, ui: &Ui, center: Pos2) {
        self.paint_returning_initials(ui, center);
    }

    /// [`Self::paint`], returning where the initials' galley was painted
    /// (`None` when they had to shrink).
    fn paint_returning_initials(&self, ui: &Ui, center: Pos2) -> Option<(Pos2, Arc<Galley>)> {
        let theme = ui.tokens();
        let shade = |color: Color32| {
            if self.hovered {
                palette::brightness(color, HOVER_BRIGHTNESS)
            } else {
                color
            }
        };
        let (fill, text_color) = self.colors(&theme);
        let (fill, text_color) = (shade(fill), shade(text_color));
        let surface = shade(self.surface.unwrap_or(theme.nav.rail));
        let rect = Rect::from_center_size(center, Vec2::splat(self.size));
        let corner_radius = self.corner_radius();
        let painter = ui.painter();

        if self.selected {
            stroke::paint_focus_ring(
                painter,
                rect,
                corner_radius,
                surface,
                shade(theme.accent.fill),
            );
        }

        painter.rect_filled(rect, corner_radius, fill);
        let initials = self.paint_initials(ui, center, text_color);

        if let Some(tone) = self.status {
            let overhang = Vec2::splat(size::AVATAR_STATUS_OVERHANG);
            let square = Rect::from_min_max(
                rect.max + overhang - Vec2::splat(size::AVATAR_STATUS),
                rect.max + overhang,
            );
            painter.rect_filled(square, radius::AVATAR_STATUS, surface);
            painter.rect_filled(
                square.shrink(size::AVATAR_STATUS_BORDER),
                radius::AVATAR_STATUS.saturating_sub(size::AVATAR_STATUS_BORDER as u8),
                shade(theme.tone(tone).fill),
            );
        }
        initials
    }

    /// The initials centred in their CSS line box; wide pairs such as WW
    /// shrink to fit the square.
    fn paint_initials(&self, ui: &Ui, center: Pos2, color: Color32) -> Option<(Pos2, Arc<Galley>)> {
        let galley = self.role.galley(ui.ctx(), self.text, color, f32::INFINITY);
        let available_width = self.size * INITIALS_WIDTH_RATIO;
        if galley.rect.width() <= available_width {
            let min = center - galley.rect.size() / 2.0;
            type_scale::paint_galley(ui.painter(), min, galley.clone(), color);
            return Some((min, galley));
        }
        let mut font = self.role.font_id(ui.ctx());
        font.size *= available_width / galley.rect.width();
        ui.painter()
            .text(center, Align2::CENTER_CENTER, self.text, font, color);
        None
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let (rect, response) = type_scale::allocate_text(ui, Vec2::splat(self.size), self.text);
        if ui.is_rect_visible(rect)
            && let Some((pos, galley)) = self.paint_returning_initials(ui, rect.center())
        {
            type_scale::expose_text_runs(ui, &response, pos, &galley);
        }
        response
    }
}

/// A stable index in `0..count` derived from `text` (FNV-1a), so an identity
/// keeps its colour across frames, runs and platforms.
fn identity_index(text: &str, count: usize) -> usize {
    let hash = text.bytes().fold(0x811c_9dc5_u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    });
    hash as usize % count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_colours_are_stable_and_spread() {
        let theme = crate::test_support::distinct_role_theme();
        let avatars = theme.components.avatars();
        let colour = |text| Avatar::new(text).identity().colors(&theme);
        assert_eq!(colour("kind"), colour("kind"));
        assert_eq!(colour("kind").1, theme.components.avatar_text);

        let distinct: std::collections::HashSet<_> = ["kind", "prod", "staging", "dev", "eu", "us"]
            .into_iter()
            .map(|text| avatars.iter().position(|&fill| fill == colour(text).0))
            .collect();
        assert!(
            distinct.len() > 1,
            "identities should not all share one colour"
        );
    }

    #[test]
    fn categories_pick_their_avatar_colour_and_wrap() {
        let theme = crate::test_support::distinct_role_theme();
        assert_eq!(
            Avatar::new("DE").category(0).colors(&theme).0,
            theme.components.av_1
        );
        assert_eq!(
            Avatar::new("AD").category(5).colors(&theme).0,
            theme.components.av_6
        );
        assert_eq!(
            Avatar::new("DE").category(6).colors(&theme).0,
            theme.components.av_1
        );
    }

    #[test]
    fn corners_scale_with_the_size() {
        assert_eq!(
            Avatar::new("DE").corner_radius(),
            CornerRadius::same(radius::AVATAR)
        );
        assert_eq!(
            Avatar::new("DE").size(32.0).corner_radius(),
            CornerRadius::same(10)
        );
        assert_eq!(
            stroke::grow(CornerRadius::same(12), 3.0),
            CornerRadius::same(15)
        );
    }
}
