//! Property grids (`.pg`, C13): label/value pairs in responsive columns, and
//! [`Copyable`] values.
//!
//! Labels are 11/16 condensed captions in `text-muted` above 13/20 values.
//! Mono values (IPs, UIDs, images) use mono 12; status values a lamp and
//! words; references accent links that push a blade. A value too long for
//! its column is elided and shown in full as a tooltip.
//!
//! The grid has three columns from [`size::PROPERTY_GRID_THREE_COLUMNS`],
//! two from [`size::PROPERTY_GRID_TWO_COLUMNS`] and one below that, with
//! [`space::S24`] between columns and [`space::S16`] between rows. Show it
//! in a [`crate::Card`] for the design's inspector look.

use std::borrow::Cow;

use egui::{Color32, Id, Rect, Response, Sense, Ui, pos2, vec2};

use crate::copy::{copy_button, paint_copy_icon};
use crate::design::theme::{ThemeExt, Tone};
use crate::design::type_scale::{self, TextRole};
use crate::design::{radius, size, space};
use crate::interaction::{ControlState, PointingHand};
use crate::status::StatusLabel;
use crate::tooltip::TooltipExt;

/// Lines a free-text value (a description) wraps onto before it elides.
pub const FREE_TEXT_MAX_LINES: usize = 3;

/// What a property shows.
#[derive(Clone, Debug)]
pub enum PropertyValue<'a> {
    /// Plain text.
    Text(Cow<'a, str>),
    /// Monospaced text: IPs, UIDs, image references.
    Mono(Cow<'a, str>),
    /// Status words with their lamp.
    Status { text: Cow<'a, str>, tone: Tone },
    /// A reference that opens `action` (reported in
    /// [`PropertyGridResponse::activated`]).
    Link { text: Cow<'a, str>, action: Id },
    /// A value the resource does not have.
    Unavailable,
}

impl PropertyValue<'_> {
    fn text(&self) -> &str {
        match self {
            Self::Text(text) | Self::Mono(text) => text,
            Self::Status { text, .. } | Self::Link { text, .. } => text,
            Self::Unavailable => UNAVAILABLE,
        }
    }
}

/// The words of an unavailable value.
const UNAVAILABLE: &str = "Unavailable";

/// One label/value pair of a [`PropertyGrid`].
#[derive(Clone, Debug)]
pub struct Property<'a> {
    label: Cow<'a, str>,
    value: PropertyValue<'a>,
    copy: Option<Cow<'a, str>>,
    forced_state: Option<ControlState>,
    max_lines: usize,
}

impl<'a> Property<'a> {
    pub fn new(label: impl Into<Cow<'a, str>>, value: PropertyValue<'a>) -> Self {
        Self {
            label: label.into(),
            value,
            copy: None,
            forced_state: None,
            max_lines: 1,
        }
    }

    /// A plain text value.
    pub fn text(label: impl Into<Cow<'a, str>>, text: impl Into<Cow<'a, str>>) -> Self {
        Self::new(label, PropertyValue::Text(text.into()))
    }

    /// A monospaced value.
    pub fn mono(label: impl Into<Cow<'a, str>>, text: impl Into<Cow<'a, str>>) -> Self {
        Self::new(label, PropertyValue::Mono(text.into()))
    }

    /// A status value.
    pub fn status(
        label: impl Into<Cow<'a, str>>,
        text: impl Into<Cow<'a, str>>,
        tone: Tone,
    ) -> Self {
        Self::new(
            label,
            PropertyValue::Status {
                text: text.into(),
                tone,
            },
        )
    }

    /// A reference that opens `action`.
    pub fn link(label: impl Into<Cow<'a, str>>, text: impl Into<Cow<'a, str>>, action: Id) -> Self {
        Self::new(
            label,
            PropertyValue::Link {
                text: text.into(),
                action,
            },
        )
    }

    /// A value the resource does not have.
    pub fn unavailable(label: impl Into<Cow<'a, str>>) -> Self {
        Self::new(label, PropertyValue::Unavailable)
    }

    /// Make a text or mono value copyable (see [`Copyable`]).
    pub fn copyable(mut self) -> Self {
        self.copy = Some(Cow::Owned(self.value.text().to_owned()));
        self
    }

    /// Copy `text` instead of the shown value, for example a label shown as
    /// a chip but copied as `key=value`.
    pub fn copyable_as(mut self, text: impl Into<Cow<'a, str>>) -> Self {
        self.copy = Some(text.into());
        self
    }

    /// Paint a copyable value in `state` instead of the pointer's (design
    /// boards and snapshots).
    pub fn force_state(mut self, state: ControlState) -> Self {
        self.forced_state = Some(state);
        self
    }

    /// Wrap a free-text value (a description) onto up to `max_lines` lines
    /// before eliding it, instead of one.
    pub fn wrap(mut self, max_lines: usize) -> Self {
        self.max_lines = max_lines.max(1);
        self
    }

    /// The text of a value that wraps, if this one does.
    fn wrapping_text(&self) -> Option<&str> {
        match &self.value {
            PropertyValue::Text(text) if self.max_lines > 1 => Some(text),
            _ => None,
        }
    }
}

/// What the user did in a [`PropertyGrid`].
#[derive(Debug, Default)]
pub struct PropertyGridResponse {
    /// The actions of links clicked this frame.
    pub activated: Vec<Id>,
    /// Text copied this frame.
    pub copied: Vec<String>,
}

/// A responsive grid of [`Property`] pairs filling the available width.
#[must_use = "call `show` to add the grid"]
pub struct PropertyGrid<'p, 'a> {
    properties: &'p [Property<'a>],
    max_columns: usize,
}

impl<'p, 'a> PropertyGrid<'p, 'a> {
    pub fn new(properties: &'p [Property<'a>]) -> Self {
        Self {
            properties,
            max_columns: usize::MAX,
        }
    }

    /// Use at most `columns` columns, so a short group (one long message, a
    /// pair of values) spreads across the width instead of leaving columns
    /// empty.
    pub fn max_columns(mut self, columns: usize) -> Self {
        self.max_columns = columns.max(1);
        self
    }

    /// The number of columns at `width`.
    pub fn columns(width: f32) -> usize {
        if width >= size::PROPERTY_GRID_THREE_COLUMNS {
            3
        } else if width >= size::PROPERTY_GRID_TWO_COLUMNS {
            2
        } else {
            1
        }
    }

    pub fn show(self, ui: &mut Ui) -> PropertyGridResponse {
        let mut response = PropertyGridResponse::default();
        let width = ui.available_width();
        let columns = Self::columns(width).min(self.max_columns);
        let column_width = (width - space::S24 * (columns - 1) as f32) / columns as f32;
        // Each row is as tall as its tallest value.
        let value_height = |property: &Property<'_>| {
            let lines = property.wrapping_text().map_or(1, |text| {
                type_scale::UI
                    .galley_elided(
                        ui.ctx(),
                        text,
                        Color32::PLACEHOLDER,
                        if property.copy.is_some() {
                            Copyable::wrapping_text_width(column_width)
                        } else {
                            column_width
                        },
                        property.max_lines,
                    )
                    .rows
                    .len()
            });
            lines as f32 * type_scale::UI.line_height()
        };
        let row_heights: Vec<f32> = self
            .properties
            .chunks(columns)
            .map(|row| {
                type_scale::CAPTION.line_height() + row.iter().map(value_height).fold(0.0, f32::max)
            })
            .collect();
        let gaps = row_heights.len().saturating_sub(1) as f32 * space::S16;
        let height = row_heights.iter().sum::<f32>() + gaps;
        let (grid, grid_response) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
        let mut top = grid.top();
        for (row, (cells, row_height)) in
            self.properties.chunks(columns).zip(row_heights).enumerate()
        {
            for (column, property) in cells.iter().enumerate() {
                let min = pos2(
                    grid.left() + column as f32 * (column_width + space::S24),
                    top,
                );
                let cell = Rect::from_min_size(min, vec2(column_width, row_height));
                let id = grid_response.id.with(row * columns + column);
                show_cell(ui, id, cell, property, &mut response);
            }
            top += row_height + space::S16;
        }
        response
    }
}

fn show_cell(
    ui: &mut Ui,
    id: Id,
    cell: Rect,
    property: &Property<'_>,
    response: &mut PropertyGridResponse,
) {
    let theme = ui.tokens();
    let label_role = type_scale::CAPTION;
    let label_rect = Rect::from_min_size(cell.min, vec2(cell.width(), label_role.line_height()));
    let galley =
        label_role.galley_elided(ui.ctx(), &property.label, theme.text.muted, cell.width(), 1);
    type_scale::text_at(
        ui,
        id,
        label_rect.min,
        galley,
        &property.label,
        theme.text.muted,
    );

    let value_rect = Rect::from_min_max(pos2(cell.left(), label_rect.bottom()), cell.max);
    let mut value_ui = ui.new_child(egui::UiBuilder::new().id_salt(id).max_rect(value_rect));
    value_ui.set_clip_rect(value_rect.intersect(ui.clip_rect()));
    let value_ui = &mut value_ui;
    if let Some(copy) = &property.copy {
        let role = match property.value {
            PropertyValue::Mono(_) => type_scale::MONO_ROW,
            _ => type_scale::UI,
        };
        let mut copyable = Copyable::new(property.value.text())
            .copy_as(copy)
            .label(&property.label)
            .role(role);
        if property.wrapping_text().is_some() {
            copyable = copyable.wrap(property.max_lines);
        }
        if let Some(state) = property.forced_state {
            copyable = copyable.force_state(state);
        }
        if copyable.show(value_ui).clicked() {
            response.copied.push(copy.to_string());
        }
        return;
    }
    match &property.value {
        PropertyValue::Text(text) if property.max_lines > 1 => {
            let galley = type_scale::UI.galley_elided(
                value_ui.ctx(),
                text,
                theme.text.primary,
                value_rect.width(),
                property.max_lines,
            );
            selectable_text(value_ui, galley, text);
        }
        PropertyValue::Text(text) => {
            value_text(value_ui, type_scale::UI, text, theme.text.primary);
        }
        PropertyValue::Mono(text) => {
            value_text(value_ui, type_scale::MONO_ROW, text, theme.text.primary);
        }
        PropertyValue::Unavailable => {
            value_text(value_ui, type_scale::UI, UNAVAILABLE, theme.text.muted)
                .tooltip("This value is unavailable for this resource.");
        }
        PropertyValue::Status { text, tone } => {
            StatusLabel::new(text, *tone).show(value_ui);
        }
        PropertyValue::Link { text, action } => {
            let galley = type_scale::UI.galley_elided(
                value_ui.ctx(),
                text,
                theme.accent.text,
                value_rect.width(),
                1,
            );
            let rect = Rect::from_min_size(value_rect.min, galley.rect.size());
            let link = value_ui
                .interact(rect, value_ui.id().with("link"), Sense::click())
                .with_pointing_hand();
            let name = format!("Open details for {} {text}", property.label);
            link.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Link, true, &name));
            let elided = galley.elided;
            type_scale::paint_text_as(
                value_ui,
                &link,
                rect.min,
                galley,
                theme.accent.text,
                egui::accesskit::Role::Link,
            );
            let link = if elided {
                link.tooltip(text.as_ref())
            } else {
                link
            };
            if link.clicked() {
                response.activated.push(*action);
            }
        }
    }
}

/// A one-row value at the cursor, elided to the available width with the
/// full text as a tooltip.
fn value_text(ui: &mut Ui, role: TextRole, text: &str, color: Color32) -> Response {
    let galley = role.galley_elided(ui.ctx(), text, color, ui.available_width(), 1);
    selectable_text(ui, galley, text)
}

/// A laid-out value as a selectable label (so part of it can be copied),
/// read as its full `text`; an elided value shows `text` as its tooltip.
fn selectable_text(ui: &mut Ui, galley: std::sync::Arc<egui::Galley>, text: &str) -> Response {
    // Like a selectable `egui::Label`, but drag-to-select is the only
    // interaction, and the text keeps its exact position.
    let sense = (Sense::click_and_drag() - Sense::FOCUSABLE) | Sense::hover();
    let (rect, response) = ui.allocate_exact_size(galley.rect.size(), sense);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, text));
    let elided = galley.elided;
    let color = galley
        .job
        .sections
        .first()
        .map_or(Color32::PLACEHOLDER, |section| section.format.color);
    type_scale::paint_selectable_text(ui, &response, rect.min, galley, color);
    if elided {
        response.tooltip(text)
    } else {
        response
    }
}

/// A copyable value: the value alone at rest; on hover a `control-fill`
/// pill behind it and a copy icon. A click copies it and confirms with a
/// "Copied" tooltip for 1.5 seconds.
///
/// It fills the available width, elides a value too long for it (the full
/// value is the tooltip) and is read as a button "Copy …".
#[must_use = "call `show` to add the value"]
pub struct Copyable<'a> {
    value: &'a str,
    copy: Option<&'a str>,
    label: Option<&'a str>,
    role: TextRole,
    forced_state: Option<ControlState>,
    max_lines: usize,
}

impl<'a> Copyable<'a> {
    /// `value` in 13/20 UI text.
    pub fn new(value: &'a str) -> Self {
        Self {
            value,
            copy: None,
            label: None,
            role: type_scale::UI,
            forced_state: None,
            max_lines: 1,
        }
    }

    /// Wrap a free-text value onto up to `max_lines` lines before eliding
    /// it. A wrapping value always keeps room for its copy icon, so it does
    /// not reflow on hover.
    pub fn wrap(mut self, max_lines: usize) -> Self {
        self.max_lines = max_lines.max(1);
        self
    }

    /// The width a wrapping value's text gets in `width`.
    pub(crate) fn wrapping_text_width(width: f32) -> f32 {
        width - size::COPYABLE_INSET - size::COPYABLE_ICON_GAP - size::ICON_SM
    }

    /// Copy `text` instead of the shown value.
    pub fn copy_as(mut self, text: &'a str) -> Self {
        self.copy = Some(text);
        self
    }

    /// What the value is ("UID"): the control is then read as "Copy UID"
    /// (followed by the value), instead of "Copy" and the value.
    pub fn label(mut self, label: &'a str) -> Self {
        self.label = Some(label);
        self
    }

    /// Set the value in `role` (for example mono 12/20 for UIDs).
    pub fn role(mut self, role: TextRole) -> Self {
        self.role = role;
        self
    }

    /// Paint `state` instead of the pointer's (design boards and snapshots).
    pub fn force_state(mut self, state: ControlState) -> Self {
        self.forced_state = Some(state);
        self
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let theme = ui.tokens();
        let width = ui.available_width();
        let wrapping = self.max_lines > 1;
        let wrapped_galley = wrapping.then(|| {
            self.role.galley_elided(
                ui.ctx(),
                self.value,
                theme.text.primary,
                Self::wrapping_text_width(width),
                self.max_lines,
            )
        });
        let height = wrapped_galley
            .as_ref()
            .map_or(self.role.line_height(), |galley| galley.rect.height());
        let name = match self.label {
            Some(label) => format!("Copy {label}"),
            None => format!("Copy {}", self.value),
        };
        let copy = self.copy.unwrap_or(self.value);
        copy_button(
            ui,
            vec2(width, height),
            &name,
            copy,
            self.forced_state,
            |ui, rect, active| {
                let galley = wrapped_galley.unwrap_or_else(|| {
                    let text_width = if active {
                        Self::wrapping_text_width(width)
                    } else {
                        width
                    };
                    self.role
                        .galley_elided(ui.ctx(), self.value, theme.text.primary, text_width, 1)
                });
                let elided = galley.elided;
                if active && ui.is_rect_visible(rect) {
                    // The pill reaches past the value on both sides; the cell clips
                    // its left end like the design's `overflow: hidden` value box.
                    let pill =
                        Rect::from_min_max(rect.min - vec2(size::COPYABLE_INSET, 0.0), rect.max);
                    ui.painter()
                        .rect_filled(pill, radius::COPYABLE, theme.control.fill);
                    paint_copy_icon(ui, rect, size::COPYABLE_INSET, self.role.line_height());
                }
                let (text_rect, label) =
                    type_scale::allocate_text(ui, galley.rect.size(), self.value);
                type_scale::paint_text(ui, &label, text_rect.min, galley, theme.text.primary);
                elided.then(|| self.value.to_owned())
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::setup_egui;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    #[test]
    fn columns_follow_the_available_width() {
        assert_eq!(PropertyGrid::columns(640.0), 3);
        assert_eq!(PropertyGrid::columns(600.0), 3);
        assert_eq!(PropertyGrid::columns(599.0), 2);
        assert_eq!(PropertyGrid::columns(400.0), 2);
        assert_eq!(PropertyGrid::columns(399.0), 1);
    }

    #[test]
    fn copying_a_value_reports_it_and_confirms() {
        let mut harness = Harness::new_ui_state(
            |ui, copied: &mut Vec<String>| {
                ui.set_width(640.0);
                let properties = [
                    Property::text("Kind", "Pod"),
                    Property::mono("UID", "d1f2c3a4").copyable(),
                    Property::text("Labels", "app=web").copyable_as("app=web\n"),
                ];
                copied.extend(PropertyGrid::new(&properties).show(ui).copied);
            },
            Vec::new(),
        );
        setup_egui(&mut harness);
        harness.run();
        harness.get_by_label("Copy UID").click();
        harness.run();
        assert_eq!(harness.state(), &["d1f2c3a4".to_owned()]);
        assert!(harness.query_by_label(crate::copy::COPIED).is_some());
        harness.get_by_label("Copy Labels").click();
        harness.run();
        assert_eq!(harness.state()[1], "app=web\n");
    }

    #[test]
    fn cells_flow_into_rows_of_columns() {
        let mut harness = Harness::new_ui(|ui| {
            ui.set_width(500.0);
            let properties = [
                Property::text("Kind", "Pod"),
                Property::text("Name", "web"),
                Property::text("Node", "kind"),
            ];
            let _ = PropertyGrid::new(&properties).show(ui);
        });
        setup_egui(&mut harness);
        harness.run();
        let kind = harness.get_by_label("Kind").rect();
        let name = harness.get_by_label("Name").rect();
        let node = harness.get_by_label("Node").rect();
        assert_eq!(kind.top(), name.top(), "two columns at 500 points");
        assert_eq!(node.left(), kind.left());
        assert_eq!(
            node.top() - kind.top(),
            type_scale::CAPTION.line_height() + type_scale::UI.line_height() + space::S16
        );
        assert_eq!(
            name.left() - kind.left(),
            (500.0 - space::S24) / 2.0 + space::S24
        );
    }

    #[test]
    fn the_copied_confirmation_shows_then_expires() {
        const VALUE: &str = "d1f2c3a4-b5e6-47f8-9a0b-1c2d3e4f5a6b-and-a-long-suffix";
        let mut harness = Harness::new_ui(|ui| {
            ui.set_width(160.0);
            let _ = Copyable::new(VALUE).label("UID").show(ui);
        });
        setup_egui(&mut harness);
        harness.run();
        let value = harness.get_by_label("Copy UID").rect().center();
        let copied = crate::test_support::click_and_collect_copies(&mut harness, value);
        assert_eq!(copied, [VALUE]);
        assert!(harness.query_by_label(crate::copy::COPIED).is_some());
        harness.run_steps(120);
        assert!(
            harness.query_by_label(crate::copy::COPIED).is_none(),
            "the confirmation expires after 1.5 s"
        );
    }
}
