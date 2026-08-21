//! Responsive inspector property and detail-table layouts.

use std::borrow::Cow;

use egui::{
    Align, Frame, Id, Label, Layout, Margin, Rect, RichText, Ui, UiBuilder, Vec2, WidgetInfo,
    WidgetType,
};

use crate::PointingHand;
use crate::colors::WHITE;
use crate::colors::gray;
use crate::design::{radius, spacing, status, surface, typography};

const MIN_COLUMN_WIDTH: f32 = 200.0;
const COPY_ICON_SIZE: f32 = 14.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DetailTone {
    Neutral,
    Success,
    Warning,
    Danger,
}

#[derive(Clone, Debug)]
pub enum DetailValue<'a> {
    Text(Cow<'a, str>),
    Unavailable,
    Status {
        text: Cow<'a, str>,
        tone: DetailTone,
    },
    Link {
        text: Cow<'a, str>,
        action: Id,
    },
}

impl<'a> From<&'a str> for DetailValue<'a> {
    fn from(value: &'a str) -> Self {
        Self::Text(value.into())
    }
}

impl From<String> for DetailValue<'static> {
    fn from(value: String) -> Self {
        Self::Text(value.into())
    }
}

#[derive(Clone, Debug)]
pub struct DetailCell<'a> {
    pub label: Cow<'a, str>,
    pub value: DetailValue<'a>,
    copy_text: Option<Cow<'a, str>>,
}

impl<'a> DetailCell<'a> {
    pub fn new(label: impl Into<Cow<'a, str>>, value: impl Into<DetailValue<'a>>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            copy_text: None,
        }
    }

    pub fn unavailable(label: impl Into<Cow<'a, str>>) -> Self {
        Self {
            label: label.into(),
            value: DetailValue::Unavailable,
            copy_text: None,
        }
    }

    pub fn status(
        label: impl Into<Cow<'a, str>>,
        text: impl Into<Cow<'a, str>>,
        tone: DetailTone,
    ) -> Self {
        Self {
            label: label.into(),
            value: DetailValue::Status {
                text: text.into(),
                tone,
            },
            copy_text: None,
        }
    }

    pub fn link(label: impl Into<Cow<'a, str>>, text: impl Into<Cow<'a, str>>, action: Id) -> Self {
        Self {
            label: label.into(),
            value: DetailValue::Link {
                text: text.into(),
                action,
            },
            copy_text: None,
        }
    }

    pub fn copyable(mut self) -> Self {
        let DetailValue::Text(text) = &self.value else {
            panic!("only text property values can be copyable");
        };
        self.copy_text = Some(text.clone());
        self
    }

    /// Copy a value that is more useful than the rendered text, for example
    /// a label rendered as `key` / `value` but copied as `key=value`.
    pub fn copyable_as(mut self, text: impl Into<Cow<'a, str>>) -> Self {
        self.copy_text = Some(text.into());
        self
    }
}

#[derive(Clone, Debug)]
pub struct DetailRow<'a> {
    pub cells: Vec<DetailCell<'a>>,
    pub framed: bool,
}

impl<'a> DetailRow<'a> {
    pub fn new(cells: impl IntoIterator<Item = DetailCell<'a>>) -> Self {
        Self {
            cells: cells.into_iter().collect(),
            framed: false,
        }
    }

    /// Render this row in a separate bordered surface.
    pub fn framed(mut self) -> Self {
        self.framed = true;
        self
    }
}

#[derive(Clone, Debug)]
pub struct DetailColumn<'a> {
    label: Cow<'a, str>,
    weight: f32,
}

impl<'a> DetailColumn<'a> {
    pub fn new(label: impl Into<Cow<'a, str>>) -> Self {
        Self {
            label: label.into(),
            weight: 1.0,
        }
    }

    pub fn weight(mut self, weight: f32) -> Self {
        self.weight = if weight.is_finite() {
            weight.max(0.1)
        } else {
            1.0
        };
        self
    }
}

#[derive(Clone, Debug)]
pub struct DetailTableRow<'a> {
    pub cells: Vec<DetailTableCell<'a>>,
}

impl<'a> DetailTableRow<'a> {
    pub fn new(cells: impl IntoIterator<Item = impl Into<DetailTableCell<'a>>>) -> Self {
        Self {
            cells: cells.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct DetailTableCell<'a> {
    pub value: DetailValue<'a>,
    copy_text: Option<Cow<'a, str>>,
}

impl<'a> DetailTableCell<'a> {
    pub fn new(value: impl Into<DetailValue<'a>>) -> Self {
        Self {
            value: value.into(),
            copy_text: None,
        }
    }

    pub fn copyable(mut self) -> Self {
        let DetailValue::Text(text) = &self.value else {
            panic!("only text table values can be copyable");
        };
        self.copy_text = Some(text.clone());
        self
    }

    pub fn copyable_as(mut self, text: impl Into<Cow<'a, str>>) -> Self {
        self.copy_text = Some(text.into());
        self
    }
}

impl<'a> From<DetailValue<'a>> for DetailTableCell<'a> {
    fn from(value: DetailValue<'a>) -> Self {
        Self::new(value)
    }
}

impl<'a> From<&'a str> for DetailTableCell<'a> {
    fn from(value: &'a str) -> Self {
        Self::new(value)
    }
}

impl From<String> for DetailTableCell<'static> {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

#[derive(Default, Debug)]
pub struct InspectorDetailsResponse {
    pub activated: Vec<Id>,
    pub copied: Vec<String>,
}

/// A responsive inspector details renderer.
///
/// Property rows preserve the supplied grouping, reducing their column count from three to two
/// to one when the available width cannot give every column a readable width. Tables keep their
/// headers aligned while allowing their cells to wrap naturally at narrow widths.
pub struct InspectorDetails;

impl InspectorDetails {
    pub fn show_properties(ui: &mut Ui, rows: &[DetailRow<'_>]) -> InspectorDetailsResponse {
        let mut response = InspectorDetailsResponse::default();
        show_property_rows(ui, rows, &mut response);
        response
    }

    /// Render property rows as one titled group with a fieldset-style border.
    pub fn show_titled_properties(
        ui: &mut Ui,
        title: &str,
        rows: &[DetailRow<'_>],
    ) -> InspectorDetailsResponse {
        let mut response = InspectorDetailsResponse::default();
        let title_galley = ui.painter().layout_no_wrap(
            title.to_owned(),
            typography::section_heading(),
            gray::_800,
        );
        ui.add_space(title_galley.size().y / 2.0);
        let title_rect = Rect::from_min_size(
            egui::pos2(
                ui.cursor().left() + spacing::MD,
                ui.cursor().top() - title_galley.size().y / 2.0,
            ),
            title_galley.size(),
        );
        let mut title_semantics_ui = ui.new_child(
            UiBuilder::new()
                .max_rect(title_rect)
                .layout(Layout::top_down(Align::LEFT)),
        );
        title_semantics_ui.add(Label::new(
            RichText::new(title)
                .font(typography::section_heading())
                .color(egui::Color32::TRANSPARENT),
        ));
        let frame = Frame::new()
            .fill(WHITE)
            .stroke(surface::muted_border())
            .corner_radius(radius::surface())
            .inner_margin(Margin::same(spacing::MD as i8))
            .show(ui, |ui| show_property_rows(ui, rows, &mut response));
        show_group_title(ui, frame.response.rect, title);
        response
    }

    pub fn show_table(
        ui: &mut Ui,
        columns: &[DetailColumn<'_>],
        rows: &[DetailTableRow<'_>],
    ) -> InspectorDetailsResponse {
        let mut response = InspectorDetailsResponse::default();
        if columns.is_empty() {
            return response;
        }
        show_table_row(ui, columns, |ui, index| {
            ui.label(
                RichText::new(columns[index].label.as_ref())
                    .font(typography::metadata())
                    .color(gray::_500),
            );
        });
        ui.add_space(spacing::SM);
        ui.separator();
        for row in rows {
            ui.add_space(spacing::SM);
            show_table_row(ui, columns, |ui, index| {
                if let Some(cell) = row.cells.get(index) {
                    let label = columns[index].label.as_ref();
                    show_value(
                        ui,
                        &cell.value,
                        cell.copy_text.as_deref().map(|text| (label, text)),
                        Some(label),
                        &mut response,
                    );
                }
            });
            ui.add_space(spacing::SM);
            ui.separator();
        }
        response
    }
}

mod rendering;
use rendering::*;

#[cfg(test)]
mod tests;
