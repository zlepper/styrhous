//! Navigation list rows (`.lrow`, C13): settings sections and other lists
//! that push a blade.
//!
//! A [`ListRow`] is [`size::LIST_ROW`] tall: an icon [`Tile`], a title with
//! a muted description, optional trailing meta text or a status pill, and a
//! chevron. The whole row is the hit target and Enter activates it. Rows are
//! grouped in a card with dividers between them by [`list_group`].

use egui::{Align, Layout, Rect, Response, Sense, Ui, UiBuilder, Vec2, pos2, vec2};

use crate::badge::Pill;
use crate::design::theme::{ThemeExt, Tone};
use crate::design::type_scale;
use crate::design::{size, space, stroke};
use crate::icons::Icon;
use crate::interaction::{ControlState, Interaction, clickable_scope};
use crate::tile::{Tile, TileSize};
use crate::tooltip::TooltipExt;

/// Gap between the items of a row.
const ROW_GAP: f32 = space::S12;

/// What a list row shows before its chevron.
#[derive(Clone, Copy, Debug)]
pub enum ListRowTrailing<'a> {
    /// Muted meta text: "10 contexts".
    Meta(&'a str),
    /// A status pill: "● Active".
    Status(&'a str, Tone),
}

impl ListRowTrailing<'_> {
    fn text(&self) -> &str {
        match self {
            Self::Meta(text) | Self::Status(text, _) => text,
        }
    }
}

/// A navigation list row. Show it through a [`ListGroup`].
#[must_use = "add the row to a `ListGroup`"]
pub struct ListRow<'a> {
    icon: Icon,
    title: &'a str,
    description: Option<&'a str>,
    trailing: Option<ListRowTrailing<'a>>,
    forced_state: Option<ControlState>,
    accessibility_label: Option<String>,
}

impl<'a> ListRow<'a> {
    pub fn new(icon: Icon, title: &'a str) -> Self {
        Self {
            icon,
            title,
            description: None,
            trailing: None,
            forced_state: None,
            accessibility_label: None,
        }
    }

    /// The muted line under the title.
    pub fn description(mut self, description: &'a str) -> Self {
        self.description = Some(description);
        self
    }

    pub fn trailing(mut self, trailing: ListRowTrailing<'a>) -> Self {
        self.trailing = Some(trailing);
        self
    }

    /// Read the row as `label` instead of its words (for example the action
    /// it performs: "Open context settings: …").
    pub fn accessibility_label(mut self, label: impl Into<String>) -> Self {
        self.accessibility_label = Some(label.into());
        self
    }

    /// Paint `state` instead of the pointer's (design boards and snapshots).
    pub fn force_state(mut self, state: ControlState) -> Self {
        self.forced_state = Some(state);
        self
    }

    fn accessible_name(&self) -> String {
        if let Some(label) = &self.accessibility_label {
            return label.clone();
        }
        [
            Some(self.title),
            self.description,
            self.trailing.as_ref().map(ListRowTrailing::text),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ")
    }

    /// Show the row, `size` points large, as one button read as its words
    /// (or its accessibility label), with a divider above it after the first
    /// row. Words that had to be elided are its tooltip.
    fn show_in(self, ui: &mut Ui, size: Vec2, divider: bool) -> Response {
        let name = self.accessible_name();
        let words = [Some(self.title), self.description]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" — ");
        let layout = Layout::left_to_right(Align::Center);
        let inner = clickable_scope(ui, size, layout, &name, |ui, interaction| {
            let slot = ui.max_rect();
            let mut content = slot;
            if divider {
                content.min.y += size::HAIRLINE;
                ui.painter().hline(
                    slot.x_range(),
                    slot.top() + size::HAIRLINE / 2.0,
                    stroke::divider(&ui.tokens()),
                );
            }
            self.paint(ui, content, interaction)
        });
        if inner.inner {
            inner.response.tooltip(words)
        } else {
            inner.response
        }
    }

    /// Paint the row into `content`; returns whether a word was elided.
    fn paint(self, ui: &mut Ui, content: Rect, response: &Response) -> bool {
        let theme = ui.tokens();
        let interaction = Interaction::of(response, self.forced_state, false);
        if interaction.hovered || interaction.pressed {
            ui.painter()
                .rect_filled(content, 0, theme.surface.panel_hover);
        }

        let inner = content.shrink2(vec2(space::S16, 0.0));
        let center_y = inner.center().y;
        let tile = Tile::new(self.icon);
        let tile_side = TileSize::Md.side();
        tile.paint(
            ui,
            Rect::from_min_size(
                pos2(inner.left(), center_y - tile_side / 2.0),
                Vec2::splat(tile_side),
            ),
        );

        // Trailing items, right to left: chevron, then meta or pill.
        let mut trailing_ui = ui.new_child(
            UiBuilder::new()
                .max_rect(inner)
                .layout(Layout::right_to_left(Align::Center)),
        );
        trailing_ui.spacing_mut().item_spacing = vec2(ROW_GAP, 0.0);
        let (chevron, _) =
            trailing_ui.allocate_exact_size(Vec2::splat(size::ICON_SM), Sense::hover());
        Icon::ChevronRight.paint(ui, chevron, theme.text.muted);
        match self.trailing {
            Some(ListRowTrailing::Meta(text)) => {
                let galley =
                    type_scale::META.galley(ui.ctx(), text, theme.text.muted, f32::INFINITY);
                let (rect, _) = trailing_ui.allocate_exact_size(galley.rect.size(), Sense::hover());
                type_scale::paint_galley(trailing_ui.painter(), rect.min, galley, theme.text.muted);
            }
            Some(ListRowTrailing::Status(text, tone)) => {
                Pill::new(text, tone).show(&mut trailing_ui);
            }
            None => {}
        }
        let trailing_left = trailing_ui.min_rect().left();

        // Title and description, stacked and centred.
        let left = inner.left() + tile_side + ROW_GAP;
        let width = (trailing_left - ROW_GAP - left).max(0.0);
        let title_role = type_scale::UI_CONTROL;
        let height = title_role.line_height()
            + self
                .description
                .map_or(0.0, |_| type_scale::META.line_height());
        let mut top = center_y - height / 2.0;
        // The words are the row's own (its accessible name), not labels.
        let mut elided = false;
        let mut text = |role: type_scale::TextRole, text: &str, color| {
            let galley = role.galley_elided(ui.ctx(), text, color, width, 1);
            elided |= galley.elided;
            type_scale::paint_galley(ui.painter(), pos2(left, top), galley, color);
            top += role.line_height();
        };
        text(title_role, self.title, theme.text.primary);
        if let Some(description) = self.description {
            text(type_scale::META, description, theme.text.muted);
        }
        elided
    }
}

/// The rows of a [`list_group`].
pub struct ListGroup<'u> {
    ui: &'u mut Ui,
    rows: usize,
}

impl ListGroup<'_> {
    /// Add `row` below the previous one, a divider between them.
    pub fn row(&mut self, row: ListRow<'_>) -> Response {
        let ui = &mut *self.ui;
        let divider = self.rows > 0;
        self.rows += 1;
        row.show_in(ui, vec2(ui.available_width(), size::LIST_ROW), divider)
    }
}

/// A card of [`ListRow`]s with dividers between them.
pub fn list_group<R>(ui: &mut Ui, add_rows: impl FnOnce(&mut ListGroup<'_>) -> R) -> R {
    crate::card::Card::new().padding(0).show(ui, |ui| {
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        add_rows(&mut ListGroup { ui, rows: 0 })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::setup_egui;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    #[test]
    fn the_whole_row_is_one_button_named_by_its_words() {
        let mut harness = Harness::new_ui_state(
            |ui, clicks: &mut usize| {
                list_group(ui, |group| {
                    let row = group.row(
                        ListRow::new(Icon::CircleStack, "Contexts")
                            .description("Display names")
                            .trailing(ListRowTrailing::Meta("10 contexts")),
                    );
                    if row.clicked() {
                        *clicks += 1;
                    }
                    group.row(
                        ListRow::new(Icon::UserCircle, "License")
                            .trailing(ListRowTrailing::Status("Active", Tone::Success)),
                    );
                });
            },
            0,
        );
        setup_egui(&mut harness);
        harness.run();
        let row = harness.get_by_label("Contexts Display names 10 contexts");
        assert_eq!(row.rect().height(), size::LIST_ROW);
        row.click();
        harness.run();
        assert_eq!(*harness.state(), 1);
        let second = harness.get_by_label("License Active").rect();
        assert_eq!(second.height(), size::LIST_ROW);
        assert_eq!(second.top(), row_top(&harness) + size::LIST_ROW);
    }

    fn row_top(harness: &Harness<'_, usize>) -> f32 {
        harness
            .get_by_label("Contexts Display names 10 contexts")
            .rect()
            .top()
    }

    #[test]
    fn a_focused_row_activates_with_enter() {
        let mut harness = Harness::new_ui_state(
            |ui, clicks: &mut usize| {
                list_group(ui, |group| {
                    if group
                        .row(ListRow::new(Icon::CircleStack, "Contexts"))
                        .clicked()
                    {
                        *clicks += 1;
                    }
                });
            },
            0,
        );
        setup_egui(&mut harness);
        harness.run();
        harness.get_by_label("Contexts").focus();
        harness.run();
        harness.key_press(egui::Key::Enter);
        harness.run();
        assert_eq!(*harness.state(), 1);
    }
}
