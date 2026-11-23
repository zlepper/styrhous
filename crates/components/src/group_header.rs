//! The header of a group of table rows (C17 `.gh`): a chip with the
//! group's chevron and name, its row count, its failing count and the part
//! of the group on screen.

use egui::{Id, Rect, Response, Sense, Ui, Vec2, pos2, vec2};

use crate::PointingHand;
use crate::design::theme::ThemeExt;
use crate::design::type_scale::{self, TextRole};
use crate::design::{radius, size, space};
use crate::icons::Icon;
use crate::interaction::{ControlState, Interaction};
use crate::search::format_count;

/// The group name in its chip and the row count (`.gchip`, `.gn`).
const NAME_ROLE: TextRole = type_scale::MONO_ROW;
/// The failing count and the window (`.gf`, `.gr`).
const META_ROLE: TextRole = type_scale::META_ROW;
/// Gap between the chip, the count and the failing count (`.gh` gap 10).
const GAP: f32 = 10.0;
/// Gap between a chip's chevron and its name.
const CHIP_GAP: f32 = space::S6;

/// What a [`GroupHeader`] did this frame.
pub struct GroupHeaderResponse {
    /// The chip, which collapses and expands the group.
    pub chip: Response,
    /// The whole header row.
    pub row: Response,
}

impl GroupHeaderResponse {
    /// Whether the chip was clicked: toggle the group.
    pub fn toggled(&self) -> bool {
        self.chip.clicked()
    }
}

/// A group header: "⌄ search  642 pods  7 failing … rows 389–397 of 642",
/// [`size::GROUP_HEADER`] tall. Clicking the chip collapses the group;
/// a collapsed group keeps its header and counts (S17).
#[must_use = "call `show` to add the group header"]
pub struct GroupHeader<'a> {
    name: &'a str,
    count: usize,
    unit: (&'a str, &'a str),
    failing: usize,
    window: Option<&'a str>,
    expanded: bool,
    forced_state: Option<ControlState>,
}

impl<'a> GroupHeader<'a> {
    /// The header of the group `name` holding `count` rows, expanded.
    pub fn new(name: &'a str, count: usize) -> Self {
        Self {
            name,
            count,
            unit: ("row", "rows"),
            failing: 0,
            window: None,
            expanded: true,
            forced_state: None,
        }
    }

    /// The unit of the count, singular and plural ("pod", "pods").
    pub fn unit(mut self, singular: &'a str, plural: &'a str) -> Self {
        self.unit = (singular, plural);
        self
    }

    /// How many of the group's rows have failed (shown in `danger-text`
    /// when any have).
    pub fn failing(mut self, failing: usize) -> Self {
        self.failing = failing;
        self
    }

    /// Which part of the group is on screen ("rows 389–397 of 642",
    /// "first 20 of 1,210"), against the header's end.
    pub fn window(mut self, window: &'a str) -> Self {
        self.window = Some(window);
        self
    }

    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = expanded;
        self
    }

    /// Whether the group is expanded.
    pub(crate) fn is_expanded(&self) -> bool {
        self.expanded
    }

    /// Paint the chip in `state` whatever the pointer does (boards).
    pub fn force_state(mut self, state: ControlState) -> Self {
        self.forced_state = Some(state);
        self
    }

    /// The count with its unit: "642 pods", "1 pod".
    pub fn count_text(&self) -> String {
        crate::search::counted(self.count, self.unit.0, self.unit.1)
    }

    /// Add the header across the available width.
    pub fn show(self, ui: &mut Ui) -> GroupHeaderResponse {
        let (rect, row) = ui.allocate_exact_size(
            vec2(ui.available_width(), size::GROUP_HEADER),
            Sense::hover(),
        );
        let chip = self.paint(ui, rect, row.id);
        GroupHeaderResponse { chip, row }
    }

    /// Show the header in `rect` (already allocated, a row of a grouped
    /// table) under `id`; returns the chip's response.
    pub(crate) fn paint(self, ui: &mut Ui, rect: Rect, id: Id) -> Response {
        let theme = ui.tokens();
        let center_y = rect.center().y;
        let name = NAME_ROLE.galley(ui.ctx(), self.name, theme.text.primary, f32::INFINITY);
        let chip_width = size::GROUP_CHIP_PADDING_LEFT
            + size::ICON_XS
            + CHIP_GAP
            + name.rect.width()
            + size::GROUP_CHIP_PADDING_RIGHT;
        let chip_rect = Rect::from_min_size(
            pos2(
                rect.left() + size::GROUP_HEADER_PADDING_X,
                center_y - size::GROUP_CHIP / 2.0,
            ),
            vec2(chip_width, size::GROUP_CHIP),
        );
        let verb = if self.expanded { "Collapse" } else { "Expand" };
        let chip = ui
            .interact(chip_rect, id.with("chip"), Sense::click())
            .with_pointing_hand();
        let enabled = ui.is_enabled();
        chip.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Button,
                enabled,
                format!("{verb} {}", self.name),
            )
        });
        let expanded = self.expanded;
        ui.ctx()
            .accesskit_node_builder(chip.id, |node| node.set_expanded(expanded));
        let state = Interaction::of(&chip, self.forced_state, false);
        if ui.is_rect_visible(chip_rect) {
            let fill = state.pick(
                theme.components.well_bg,
                theme.control.fill_hover,
                theme.control.fill_active,
            );
            ui.painter().rect_filled(chip_rect, radius::FULL, fill);
            let chevron = if self.expanded {
                Icon::ChevronDown
            } else {
                Icon::ChevronRight
            };
            let icon_left = chip_rect.left() + size::GROUP_CHIP_PADDING_LEFT;
            chevron.paint(
                ui,
                Rect::from_min_size(
                    pos2(icon_left, center_y - size::ICON_XS / 2.0),
                    Vec2::splat(size::ICON_XS),
                ),
                theme.text.muted,
            );
            let pos = pos2(
                icon_left + size::ICON_XS + CHIP_GAP,
                center_y - name.rect.height() / 2.0,
            );
            type_scale::paint_galley(ui.painter(), pos, name, theme.text.primary);
        }

        let mut left = chip_rect.right() + GAP;
        let label =
            |ui: &Ui, role: TextRole, text: &str, color: egui::Color32, left: f32, salt: &str| {
                let galley = role.galley(ui.ctx(), text, color, f32::INFINITY);
                let pos = pos2(left, center_y - galley.rect.height() / 2.0);
                let width = galley.rect.width();
                type_scale::text_at(ui, id.with(salt), pos, galley, text, color);
                width
            };
        left += label(
            ui,
            NAME_ROLE,
            &self.count_text(),
            theme.text.muted,
            left,
            "count",
        ) + GAP;
        if self.failing > 0 {
            label(
                ui,
                META_ROLE,
                &format!("{} failing", format_count(self.failing)),
                theme.danger.text,
                left,
                "failing",
            );
        }
        if let Some(window) = self.window {
            let width = META_ROLE
                .galley(ui.ctx(), window, theme.text.muted, f32::INFINITY)
                .rect
                .width();
            let right = rect.right() - size::GROUP_HEADER_PADDING_X;
            label(
                ui,
                META_ROLE,
                window,
                theme.text.muted,
                right - width,
                "window",
            );
        }
        chip
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_take_the_singular_for_one_row() {
        let header = |count| GroupHeader::new("search", count).unit("pod", "pods");
        assert_eq!(header(1).count_text(), "1 pod");
        assert_eq!(header(642).count_text(), "642 pods");
        assert_eq!(header(1_210).count_text(), "1,210 pods");
    }
}
