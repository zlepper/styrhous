//! Colour swatches: a row of small coloured squares to pick one colour from,
//! such as the avatar colour of a context (A09).
//!
//! Each swatch is [`size::SWATCH`] points square with the mini corner
//! radius. The chosen one wears the focus ring, a gap in the canvas colour
//! and then the ring, as a focused control does.

use egui::{CursorIcon, Rect, Sense, Ui, Vec2, WidgetInfo, WidgetType};

use crate::design::theme::ThemeExt;
use crate::design::{radius, size, space, stroke};

/// The space between neighbouring swatches.
const GAP: f32 = space::S6;

/// A row of swatches, one per colour.
#[must_use = "call `show` to add the swatches"]
pub struct Swatches<'a> {
    colours: &'a [egui::Color32],
    selected: Option<usize>,
    label: &'a str,
}

impl<'a> Swatches<'a> {
    /// Swatches for `colours`, `selected` ringed. Each is read as `label`
    /// and its number ("Colour 3").
    pub fn new(colours: &'a [egui::Color32], selected: Option<usize>, label: &'a str) -> Self {
        Self {
            colours,
            selected,
            label,
        }
    }

    /// Show the row, [`size::CONTROL_MD`] tall with the swatches centred in
    /// it. Returns the index of the swatch that was clicked, if any.
    pub fn show(self, ui: &mut Ui) -> Option<usize> {
        let theme = ui.tokens();
        let (row, _) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), size::CONTROL_MD),
            Sense::hover(),
        );
        let mut clicked = None;
        // One tab stop for the whole group: the chosen swatch, else the first.
        let tab_stop = self
            .selected
            .filter(|index| *index < self.colours.len())
            .unwrap_or(0);
        let group = ui.id().with("swatches-focus");
        if let Some(next) = ui.data_mut(|data| data.remove_temp::<usize>(group))
            && let Some(id) = (next < self.colours.len()).then(|| ui.id().with(("swatch", next)))
        {
            ui.memory_mut(|memory| memory.request_focus(id));
        }
        let ids = (0..self.colours.len())
            .map(|index| ui.id().with(("swatch", index)))
            .collect::<Vec<_>>();
        for (index, colour) in self.colours.iter().enumerate() {
            let left = row.left() + index as f32 * (size::SWATCH + GAP);
            let rect = Rect::from_min_size(
                egui::pos2(left, row.center().y - size::SWATCH / 2.0),
                Vec2::splat(size::SWATCH),
            );
            let sense = if index == tab_stop {
                Sense::click()
            } else {
                Sense::CLICK
            };
            let response = ui
                .interact(rect, ids[index], sense)
                .on_hover_cursor(CursorIcon::PointingHand);
            let chosen = self.selected == Some(index);
            let label = format!("{} {}", self.label, index + 1);
            response.widget_info(|| {
                WidgetInfo::selected(WidgetType::RadioButton, ui.is_enabled(), chosen, &label)
            });
            if chosen {
                stroke::paint_focus_ring(
                    ui.painter(),
                    rect,
                    radius::MINI,
                    theme.surface.canvas,
                    theme.focus.ring,
                );
            }
            ui.painter().rect_filled(rect, radius::MINI, *colour);
            if response.clicked() {
                clicked = Some(index);
            }
            // Arrow keys move the choice, and the focus with it.
            if response.has_focus() {
                let step = ui.input(|input| {
                    i32::from(input.key_pressed(egui::Key::ArrowRight))
                        - i32::from(input.key_pressed(egui::Key::ArrowLeft))
                });
                let next = index.saturating_add_signed(step as isize);
                if step != 0 && next < self.colours.len() {
                    clicked = Some(next);
                    // The new swatch is the tab stop only from the next frame on.
                    ui.data_mut(|data| data.insert_temp(group, next));
                }
            }
        }
        clicked
    }
}
