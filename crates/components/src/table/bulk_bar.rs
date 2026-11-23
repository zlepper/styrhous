//! The bulk selection bar (C08): shown between a table's toolbar and its
//! header while rows are selected, with the count and the bulk actions.

use egui::{Frame, Margin, Response, Stroke, Ui, vec2};

use crate::design::theme::ThemeExt;
use crate::design::{radius, size, space, type_scale};
use crate::selection::{CheckState, Checkbox};
use crate::{Button, ButtonSize};

/// The bar's height, its border included (S08).
const BAR_HEIGHT: f32 = 44.0;
/// The divider between the count and the actions (`.vr`).
const DIVIDER: egui::Vec2 = vec2(1.0, 20.0);
/// The default hint at the trailing end.
const RANGE_HINT: &str = "Shift-click selects a range";

/// What the bulk selection bar did this frame.
pub struct BulkSelectionResponse {
    /// The whole bar.
    pub response: Response,
    /// The selection checkbox or "Clear selection" was clicked: clear the
    /// selection.
    pub clear: bool,
}

/// The bulk selection bar (`accent-tint`, 44 points): a checkbox that
/// clears the selection, "N selected", the caller's actions (a quiet danger
/// Delete, …), "Clear selection" and a hint at the trailing end.
#[must_use = "call `show` to add the bar"]
pub struct BulkSelectionBar<'a> {
    selected: usize,
    total: usize,
    hint: &'a str,
}

impl<'a> BulkSelectionBar<'a> {
    /// A bar for `selected` of `total` selectable rows.
    pub fn new(selected: usize, total: usize) -> Self {
        Self {
            selected,
            total,
            hint: RANGE_HINT,
        }
    }

    /// Replace the trailing hint ("Shift-click selects a range").
    pub fn hint(mut self, hint: &'a str) -> Self {
        self.hint = hint;
        self
    }

    /// Add the bar across the available width, with `add_actions` adding
    /// the bulk action buttons (small) after the count.
    pub fn show(self, ui: &mut Ui, add_actions: impl FnOnce(&mut Ui)) -> BulkSelectionResponse {
        let theme = ui.tokens();
        let mut clear = false;
        let inner = Frame::new()
            .fill(theme.accent.tint)
            .stroke(Stroke::new(size::HAIRLINE, theme.accent.tint_active))
            .corner_radius(radius::CARD)
            .inner_margin(Margin::symmetric(space::S12 as i8, 0))
            .show(ui, |ui| {
                let height = BAR_HEIGHT - 2.0 * size::HAIRLINE;
                ui.set_height(height);
                ui.set_width(ui.available_width());
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing = vec2(space::S12, 0.0);
                    let state = CheckState::from_counts(self.selected, self.total);
                    clear |= Checkbox::with_state(state, "")
                        .accessibility_label("Selected rows")
                        .show(ui)
                        .clicked();
                    type_scale::UI_CONTROL.label(
                        ui,
                        &format!("{} selected", self.selected),
                        theme.accent.text,
                    );
                    let (divider, _) = ui.allocate_exact_size(DIVIDER, egui::Sense::hover());
                    ui.painter().rect_filled(divider, 0, theme.divider.base);
                    add_actions(ui);
                    clear |= Button::ghost("Clear selection")
                        .size(ButtonSize::Sm)
                        .show(ui)
                        .clicked();
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        type_scale::META.label(ui, self.hint, theme.text.muted);
                    });
                });
            });
        BulkSelectionResponse {
            response: inner.response,
            clear,
        }
    }
}
