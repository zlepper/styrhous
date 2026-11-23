//! Circular text avatars shared by navigation, settings, and previews.

use crate::design::theme::{Theme, ThemeExt};
use crate::design::type_scale::{self, TextRole};
use egui::{Align2, Color32, Pos2, Response, Sense, Ui, Vec2};

/// The colour treatment of a [`TextAvatar`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AvatarTone {
    /// A quiet well, for avatars that are not identities.
    Neutral,
    /// A category colour chosen from the text, so each identity keeps its colour.
    Identity,
    /// The accent tint, for the current selection.
    Accent,
}

pub struct TextAvatar<'a> {
    text: &'a str,
    size: f32,
    tone: AvatarTone,
    role: TextRole,
}

impl<'a> TextAvatar<'a> {
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            size: 28.0,
            tone: AvatarTone::Neutral,
            role: type_scale::BODY,
        }
    }

    /// Use the identity treatment of navigation chrome: a category colour
    /// picked deterministically from the avatar text.
    pub fn identity(mut self) -> Self {
        self.tone = AvatarTone::Identity;
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
    /// The type role of the initials (default [`type_scale::BODY`]).
    pub fn font(mut self, role: TextRole) -> Self {
        self.role = role;
        self
    }

    /// The fill and text colours of this avatar in `theme`.
    fn colors(&self, theme: &Theme) -> (Color32, Color32) {
        match self.tone {
            AvatarTone::Neutral => (theme.components.well_bg, theme.text.secondary),
            AvatarTone::Identity => {
                let avatars = theme.components.avatars();
                (
                    avatars[identity_index(self.text, avatars.len())],
                    theme.components.avatar_text,
                )
            }
            AvatarTone::Accent => (theme.accent.tint, theme.accent.text),
        }
    }

    pub fn paint(&self, ui: &Ui, center: Pos2) {
        let (fill, text_color) = self.colors(&ui.tokens());
        let mut font = self.role.font_id(ui.ctx());
        let width = ui
            .painter()
            .layout_no_wrap(self.text.to_owned(), font.clone(), text_color)
            .size()
            .x;
        // Wide pairs such as WW must fit within the same circle as single initials.
        let available_width = self.size * 0.75;
        if width > available_width {
            font.size *= available_width / width;
        }
        ui.painter().circle_filled(center, self.size / 2.0, fill);
        ui.painter()
            .text(center, Align2::CENTER_CENTER, self.text, font, text_color);
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(self.size), Sense::hover());
        self.paint(ui, rect.center());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Label, ui.is_enabled(), self.text)
        });
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
        let colour = |text| TextAvatar::new(text).identity().colors(&theme);
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
}
