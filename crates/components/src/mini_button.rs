//! Mini buttons (`.mini`, `.rx`): 24 point icon buttons inside fields,
//! toasts and banners (clear, previous/next match, dismiss).

use std::sync::Arc;

use egui::{Color32, Galley, Id, Rect, Response, Sense, Ui, Vec2, WidgetInfo, WidgetType};

use crate::PointingHand;
use crate::control::with_opacity;
use crate::design::theme::{Theme, ThemeExt};
use crate::design::type_scale::{self, TextRole};
use crate::design::{radius, size, stroke};
use crate::icons::Icon;
use crate::tooltip::TooltipExt;

/// Side of a mini button.
pub const MINI_BUTTON: f32 = size::CONTROL_MINI;
/// Opacity of a mini button that cannot act (`.mini.is-disabled`).
const DISABLED_OPACITY: f32 = 0.4;
/// The regex toggle's glyphs (`.rx`): Plex Mono 11.5/16 medium.
const REGEX_ROLE: TextRole = type_scale::MONO
    .with_weight(type_scale::Weight::Medium)
    .with_size(11.5, 16.0);

/// What a mini button shows.
enum MiniContent {
    Icon(Icon),
    /// The regex toggle's `.*`, on or off.
    Regex(bool),
}

/// A 24 point frameless button with a 14 point glyph (`.mini`): secondary
/// text at rest, a `control-fill-hover` square with primary text on hover.
#[must_use = "call `show` to add the button"]
pub struct MiniButton {
    content: MiniContent,
    label: &'static str,
    enabled: bool,
    color: Option<Color32>,
}

impl MiniButton {
    /// A mini button showing `icon`, named `label` (also its tooltip).
    pub fn icon(icon: Icon, label: &'static str) -> Self {
        Self {
            content: MiniContent::Icon(icon),
            label,
            enabled: true,
            color: None,
        }
    }

    /// The × that dismisses a toast or banner.
    pub fn dismiss() -> Self {
        Self::icon(Icon::XMark, "Dismiss")
    }

    /// The regex toggle of a search field (`.rx`).
    pub(crate) fn regex(on: bool) -> Self {
        Self {
            content: MiniContent::Regex(on),
            label: "Regular expression",
            enabled: true,
            color: None,
        }
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// The glyph colour at rest, in place of `text-secondary` (CSS
    /// `color: inherit` on a tinted banner).
    pub fn color(mut self, color: Color32) -> Self {
        self.color = Some(color);
        self
    }

    /// The fill (transparent for none) and content colour.
    fn colors(&self, theme: &Theme, hovered: bool) -> (Color32, Color32) {
        match self.content {
            MiniContent::Regex(true) => (
                if hovered {
                    theme.accent.tint_hover
                } else {
                    theme.accent.tint
                },
                theme.accent.text,
            ),
            _ if hovered => (theme.control.fill_hover, theme.text.primary),
            MiniContent::Regex(false) => (Color32::TRANSPARENT, theme.text.muted),
            MiniContent::Icon(_) => (
                Color32::TRANSPARENT,
                self.color.unwrap_or(theme.text.secondary),
            ),
        }
    }

    /// Add the button at the cursor.
    pub fn show_here(self, ui: &mut Ui) -> Response {
        let (rect, slot) = ui.allocate_exact_size(Vec2::splat(MINI_BUTTON), Sense::hover());
        self.show(ui, rect, slot.id.with("mini"))
    }

    /// Show the button in `rect` (already inside allocated space) as `id`.
    pub fn show(self, ui: &mut Ui, rect: Rect, id: Id) -> Response {
        let enabled = self.enabled && ui.is_enabled();
        let sense = if enabled {
            Sense::click()
        } else {
            Sense::hover()
        };
        let response = ui.interact(rect, id, sense);
        let response = if enabled {
            response.with_pointing_hand().tooltip(self.label)
        } else {
            response
        };
        let label = self.label;
        match self.content {
            MiniContent::Regex(on) => response
                .widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, enabled, on, label)),
            MiniContent::Icon(_) => {
                response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, enabled, label));
            }
        }
        if !ui.is_rect_visible(rect.expand(size::FOCUS_RING_EXTENT)) {
            return response;
        }

        let theme = ui.tokens();
        let corner_radius = radius::MINI;
        if response.has_focus() {
            stroke::paint_focus_ring(
                ui.painter(),
                rect,
                corner_radius,
                theme.components.field_bg,
                theme.focus.ring_neutral,
            );
        }
        let (fill, color) = self.colors(&theme, enabled && response.hovered());
        let opacity = if enabled { 1.0 } else { DISABLED_OPACITY };
        with_opacity(ui, opacity, |ui| {
            if fill.a() > 0 {
                ui.painter().rect_filled(rect, corner_radius, fill);
            }
            match self.content {
                MiniContent::Icon(icon) => {
                    let glyph = Rect::from_center_size(rect.center(), Vec2::splat(size::ICON_SM));
                    icon.paint(ui, glyph, color);
                }
                MiniContent::Regex(_) => {
                    let galley: Arc<Galley> =
                        REGEX_ROLE.galley(ui.ctx(), ".*", color, f32::INFINITY);
                    let pos = rect.center() - galley.rect.size() / 2.0;
                    type_scale::paint_galley(ui.painter(), pos, galley, color);
                }
            }
        });
        response
    }
}
