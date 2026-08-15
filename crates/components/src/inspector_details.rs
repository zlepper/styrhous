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

fn show_property_rows(
    ui: &mut Ui,
    rows: &[DetailRow<'_>],
    response: &mut InspectorDetailsResponse,
) {
    let mut rows = rows.iter().filter(|row| !row.cells.is_empty()).peekable();
    while let Some(row) = rows.next() {
        if row.framed {
            Frame::new()
                .fill(WHITE)
                .stroke(surface::muted_border())
                .corner_radius(radius::surface())
                .inner_margin(Margin::same(spacing::MD as i8))
                .show(ui, |ui| show_property_row(ui, row, response));
        } else {
            show_property_row(ui, row, response);
        }
        if rows.peek().is_some() {
            ui.add_space(spacing::SM);
        }
    }
}

fn show_group_title(ui: &mut Ui, group_rect: Rect, title: &str) {
    let title_galley =
        ui.painter()
            .layout_no_wrap(title.to_owned(), typography::section_heading(), gray::_800);
    let title_rect = Rect::from_min_size(
        egui::pos2(
            group_rect.left() + spacing::MD,
            group_rect.top() - title_galley.size().y / 2.0,
        ),
        title_galley.size(),
    );
    ui.painter()
        .rect_filled(title_rect.expand2(egui::vec2(spacing::XS, 2.0)), 0.0, WHITE);
    ui.painter()
        .galley(title_rect.min, title_galley, gray::_800);
}

fn show_property_row(ui: &mut Ui, row: &DetailRow<'_>, response: &mut InspectorDetailsResponse) {
    let capacity = column_capacity(ui.available_width());
    let columns = row.cells.len().min(capacity);
    for cells in row.cells.chunks(columns) {
        ui.columns(cells.len(), |columns| {
            for (cell, column) in cells.iter().zip(columns) {
                column.with_layout(Layout::top_down(Align::LEFT), |ui| {
                    show_property_cell(ui, cell, response);
                });
            }
        });
        if cells.len() < row.cells.len() {
            ui.add_space(spacing::SM);
        }
    }
}

fn column_capacity(available_width: f32) -> usize {
    for columns in (2..=3).rev() {
        let gaps = spacing::SM * (columns - 1) as f32;
        if (available_width - gaps) / columns as f32 >= MIN_COLUMN_WIDTH {
            return columns;
        }
    }
    1
}

fn show_property_cell(ui: &mut Ui, cell: &DetailCell<'_>, response: &mut InspectorDetailsResponse) {
    ui.label(
        RichText::new(cell.label.as_ref())
            .font(typography::metadata())
            .color(gray::_500),
    );
    show_value(
        ui,
        &cell.value,
        cell.copy_text
            .as_deref()
            .map(|text| (cell.label.as_ref(), text)),
        Some(cell.label.as_ref()),
        response,
    );
}

fn show_value(
    ui: &mut Ui,
    value: &DetailValue<'_>,
    copy: Option<(&str, &str)>,
    field_label: Option<&str>,
    response: &mut InspectorDetailsResponse,
) {
    match value {
        DetailValue::Text(text) => show_text_value(ui, text, gray::_900, copy, response),
        DetailValue::Unavailable => {
            let unavailable = ui.label(
                RichText::new("Unavailable")
                    .font(typography::metadata())
                    .color(gray::_500),
            );
            let accessibility_label = field_label.map_or_else(
                || "Unavailable".to_owned(),
                |label| format!("{label}: unavailable"),
            );
            unavailable.widget_info(|| {
                WidgetInfo::labeled(WidgetType::Label, true, accessibility_label.clone())
            });
            unavailable.on_hover_text("This value is unavailable for this resource.");
        }
        DetailValue::Status { text, tone } => {
            ui.horizontal_top(|ui| {
                let (marker_rect, _) =
                    ui.allocate_exact_size(egui::vec2(12.0, 0.0), egui::Sense::hover());
                let label = ui.add(
                    Label::new(
                        RichText::new(text.as_ref())
                            .font(typography::metadata())
                            .color(gray::_900),
                    )
                    .wrap(),
                );
                ui.painter().circle_filled(
                    egui::pos2(marker_rect.center().x, label.rect.center().y),
                    4.0,
                    tone_color(*tone),
                );
            });
        }
        DetailValue::Link { text, action } => {
            let link = ui
                .add(
                    Label::new(
                        RichText::new(text.as_ref())
                            .font(typography::metadata())
                            .color(crate::colors::indigo::_600),
                    )
                    .wrap()
                    .sense(egui::Sense::click()),
                )
                .with_pointing_hand();
            let accessibility_label = field_label.map_or_else(
                || format!("Open details for {text}"),
                |label| format!("Open details for {label} {text}"),
            );
            link.widget_info(|| {
                WidgetInfo::labeled(
                    WidgetType::Button,
                    link.enabled(),
                    accessibility_label.clone(),
                )
            });
            if link.clicked() {
                response.activated.push(*action);
            }
        }
    }
}

fn show_text_value(
    ui: &mut Ui,
    text: &str,
    color: egui::Color32,
    copy: Option<(&str, &str)>,
    response: &mut InspectorDetailsResponse,
) {
    if let Some((label, copy_text)) = copy {
        let metadata_line_height = ui.fonts_mut(|fonts| fonts.row_height(&typography::metadata()));
        ui.horizontal_top(|ui| {
            let text_layout = ui.allocate_ui_with_layout(
                egui::vec2(
                    (ui.available_width() - COPY_ICON_SIZE - ui.spacing().item_spacing.x).max(0.0),
                    0.0,
                ),
                Layout::top_down(Align::LEFT),
                |ui| {
                    ui.add(
                        Label::new(
                            RichText::new(text)
                                .font(typography::metadata())
                                .color(color),
                        )
                        .wrap(),
                    )
                },
            );
            let icon_rect = Rect::from_center_size(
                egui::pos2(
                    ui.cursor().left() + COPY_ICON_SIZE / 2.0,
                    text_layout.response.rect.top() + metadata_line_height / 2.0,
                ),
                Vec2::splat(COPY_ICON_SIZE),
            );
            let copy_action = ui
                .interact(
                    icon_rect,
                    ui.id().with(("copy", label, copy_text)),
                    egui::Sense::click(),
                )
                .with_pointing_hand();
            copy_action.widget_info(|| {
                WidgetInfo::labeled(WidgetType::Button, ui.is_enabled(), format!("Copy {label}"))
            });
            if copy_action.hovered() {
                crate::icons::document_duplicate_icon()
                    .fit_to_exact_size(Vec2::splat(COPY_ICON_SIZE))
                    .tint(gray::_700)
                    .paint_at(ui, icon_rect);
            }
            if copy_action.clicked() {
                ui.ctx().copy_text(copy_text.to_owned());
                response.copied.push(copy_text.to_owned());
            }
        });
    } else {
        ui.add(
            Label::new(
                RichText::new(text)
                    .font(typography::metadata())
                    .color(color),
            )
            .wrap(),
        );
    }
}

fn tone_color(tone: DetailTone) -> egui::Color32 {
    match tone {
        DetailTone::Neutral => gray::_400,
        DetailTone::Success => status::SUCCESS,
        DetailTone::Warning => status::WARNING,
        DetailTone::Danger => status::DANGER,
    }
}

fn show_table_row(
    ui: &mut Ui,
    columns: &[DetailColumn<'_>],
    mut show_cell: impl FnMut(&mut Ui, usize),
) {
    let gap_width = spacing::SM * (columns.len().saturating_sub(1)) as f32;
    let total_weight: f32 = columns.iter().map(|column| column.weight).sum();
    let available = (ui.available_width() - gap_width).max(0.0);
    ui.horizontal_top(|ui| {
        for (index, column) in columns.iter().enumerate() {
            let width = available * column.weight / total_weight;
            ui.allocate_ui_with_layout(
                egui::vec2(width, 0.0),
                Layout::top_down(Align::LEFT),
                |ui| {
                    ui.set_min_width(width);
                    show_cell(ui, index);
                },
            );
            if index + 1 != columns.len() {
                ui.add_space(spacing::SM);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detail_column_weight_is_always_positive_and_finite() {
        assert_eq!(DetailColumn::new("Label").weight(0.0).weight, 0.1);
        assert_eq!(DetailColumn::new("Label").weight(f32::NAN).weight, 1.0);
        assert_eq!(DetailColumn::new("Label").weight(f32::INFINITY).weight, 1.0);
    }
}
