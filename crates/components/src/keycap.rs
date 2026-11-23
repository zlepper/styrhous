//! Keycaps (`.kbd`): shortcut keys in hints, menus and tooltips.

use egui::{Color32, Rect, Response, Ui, Vec2, pos2, vec2};

use crate::design::theme::ThemeExt;
use crate::design::{radius, size, type_scale};

/// A keycap: the key's name in Plex Mono 10/16 on `key-bg`, at least as
/// wide as it is tall.
pub struct Keycap<'a> {
    key: &'a str,
}

impl<'a> Keycap<'a> {
    pub fn new(key: &'a str) -> Self {
        Self { key }
    }

    /// The keycap's size.
    pub fn size(&self, ui: &Ui) -> Vec2 {
        let text = type_scale::KEY
            .galley(
                ui.ctx(),
                self.key,
                ui.tokens().text.secondary,
                f32::INFINITY,
            )
            .rect
            .width();
        vec2(
            (text + 2.0 * size::KEY_PADDING_X).max(size::KEY_HEIGHT),
            size::KEY_HEIGHT,
        )
    }

    /// Add the keycap, read as its key's name.
    pub fn show(self, ui: &mut Ui) -> Response {
        let (rect, response) = type_scale::allocate_text(ui, self.size(ui), self.key);
        self.paint(ui, rect);
        response
    }

    /// Paint the keycap into `rect` (normally [`Self::size`]).
    pub fn paint(&self, ui: &Ui, rect: Rect) {
        let theme = ui.tokens();
        self.paint_in(ui, rect, theme.components.key_bg, theme.text.secondary);
    }

    // C19
    /// Paint the keycap into `rect` on `fill` with its key in `color` (a
    /// keycap on a filled button: `key-on-fill-bg`, the button's content
    /// colour).
    pub fn paint_in(&self, ui: &Ui, rect: Rect, fill: Color32, color: Color32) {
        if !ui.is_rect_visible(rect) {
            return;
        }
        ui.painter().rect_filled(rect, radius::KEY, fill);
        let galley = type_scale::KEY.galley(ui.ctx(), self.key, color, f32::INFINITY);
        let pos = pos2(
            rect.center().x - galley.rect.width() / 2.0,
            rect.center().y - galley.rect.height() / 2.0,
        );
        type_scale::paint_galley(ui.painter(), pos, galley, color);
    }
}
