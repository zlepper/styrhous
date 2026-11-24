use super::*;
use crate::tooltip::TooltipExt;

pub(super) fn show_property_rows(
    ui: &mut Ui,
    rows: &[DetailRow<'_>],
    response: &mut InspectorDetailsResponse,
) {
    let mut rows = rows.iter().filter(|row| !row.cells.is_empty()).peekable();
    while let Some(row) = rows.next() {
        if row.framed {
            crate::Card::new()
                .padding(space::S12 as i8)
                .show(ui, |ui| show_property_row(ui, row, response));
        } else {
            show_property_row(ui, row, response);
        }
        if rows.peek().is_some() {
            // The property grid's own row gap.
            ui.add_space(space::S16);
        }
    }
}

pub(super) fn show_property_row(
    ui: &mut Ui,
    row: &DetailRow<'_>,
    response: &mut InspectorDetailsResponse,
) {
    let properties: Vec<Property<'_>> = row.cells.iter().map(property).collect();
    // A row with fewer cells than columns spreads them across the width.
    let grid = PropertyGrid::new(&properties)
        .max_columns(properties.len())
        .show(ui);
    response.activated.extend(grid.activated);
    response.copied.extend(grid.copied);
}

/// The [`Property`] that shows `cell`.
fn property<'a>(cell: &DetailCell<'a>) -> Property<'a> {
    let label = cell.label.clone();
    let property = match &cell.value {
        DetailValue::Text(text) => Property::text(label, text.clone()),
        DetailValue::Unavailable => Property::unavailable(label),
        DetailValue::Status { text, tone } => Property::status(label, text.clone(), *tone),
        DetailValue::Link { text, action } => Property::link(label, text.clone(), *action),
    };
    let property = if cell.free_text {
        property.wrap(FREE_TEXT_MAX_LINES)
    } else {
        property
    };
    match &cell.copy_text {
        Some(text) => property.copyable_as(text.clone()),
        None => property,
    }
}

pub(super) fn show_value(
    ui: &mut Ui,
    value: &DetailValue<'_>,
    copy: Option<(&str, &str)>,
    field_label: Option<&str>,
    response: &mut InspectorDetailsResponse,
) {
    match value {
        DetailValue::Text(text) => {
            let color = ui.tokens().text.primary;
            show_text_value(ui, text, color, copy, response);
        }
        DetailValue::Unavailable => {
            let unavailable = ui.label(
                RichText::new("Unavailable")
                    .font(type_scale::META.font_id(ui.ctx()))
                    .color(ui.tokens().text.muted),
            );
            let accessibility_label = field_label.map_or_else(
                || "Unavailable".to_owned(),
                |label| format!("{label}: unavailable"),
            );
            unavailable.widget_info(|| {
                WidgetInfo::labeled(WidgetType::Label, true, accessibility_label.clone())
            });
            unavailable.tooltip("This value is unavailable for this resource.");
        }
        DetailValue::Status { text, tone } => {
            // The other values are plain egui labels; share their baseline.
            crate::StatusLabel::new(text.as_ref(), *tone)
                .role(type_scale::META)
                .in_egui_row()
                .show(ui);
        }
        DetailValue::Link { text, action } => {
            let link = ui
                .add(
                    Label::new(
                        RichText::new(text.as_ref())
                            .font(type_scale::META.font_id(ui.ctx()))
                            .color(ui.tokens().accent.text),
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

pub(super) fn show_text_value(
    ui: &mut Ui,
    text: &str,
    color: egui::Color32,
    copy: Option<(&str, &str)>,
    response: &mut InspectorDetailsResponse,
) {
    if let Some((label, copy_text)) = copy {
        // The same copy control as property values: the whole value is the
        // target and a click confirms with "Copied".
        let copied = Copyable::new(text)
            .copy_as(copy_text)
            .label(label)
            .role(type_scale::META)
            .wrap(usize::MAX)
            .show(ui)
            .clicked();
        if copied {
            response.copied.push(copy_text.to_owned());
        }
    } else {
        ui.add(
            Label::new(
                RichText::new(text)
                    .font(type_scale::META.font_id(ui.ctx()))
                    .color(color),
            )
            .wrap(),
        );
    }
}

pub(super) fn show_table_row(
    ui: &mut Ui,
    widths: &[f32],
    mut show_cell: impl FnMut(&mut Ui, usize),
) {
    // Repeated values (for example, kubelet in every event) still need distinct copy IDs.
    ui.push_id(ui.next_auto_id(), |ui| {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = space::S8;
            for (index, &width) in widths.iter().enumerate() {
                ui.allocate_ui_with_layout(
                    egui::vec2(width, 0.0),
                    Layout::top_down(Align::LEFT),
                    |ui| {
                        ui.set_min_width(width);
                        show_cell(ui, index);
                    },
                );
            }
        });
    });
}
