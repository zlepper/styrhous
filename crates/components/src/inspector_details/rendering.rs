use super::*;

pub(super) fn show_property_rows(
    ui: &mut Ui,
    rows: &[DetailRow<'_>],
    response: &mut InspectorDetailsResponse,
) {
    let mut rows = rows.iter().filter(|row| !row.cells.is_empty()).peekable();
    while let Some(row) = rows.next() {
        if row.framed {
            show_card(ui, space::S12 as i8, |ui| {
                show_property_row(ui, row, response)
            });
        } else {
            show_property_row(ui, row, response);
        }
        if rows.peek().is_some() {
            ui.add_space(space::S8);
        }
    }
}

pub(super) fn show_group_title(ui: &mut Ui, group_rect: Rect, title: &str) {
    let tokens = ui.tokens();
    let title_galley = ui.painter().layout_no_wrap(
        title.to_owned(),
        type_scale::TITLE.font_id(ui.ctx()),
        tokens.text.primary,
    );
    let title_rect = Rect::from_min_size(
        egui::pos2(
            group_rect.left() + space::S12,
            group_rect.top() - title_galley.size().y / 2.0,
        ),
        title_galley.size(),
    );
    // Cut the title into the card's border, like a fieldset legend.
    ui.painter().rect_filled(
        title_rect.expand2(egui::vec2(space::S4, 2.0)),
        0.0,
        tokens.card_fill(),
    );
    ui.painter()
        .galley(title_rect.min, title_galley, tokens.text.primary);
}

pub(super) fn show_property_row(
    ui: &mut Ui,
    row: &DetailRow<'_>,
    response: &mut InspectorDetailsResponse,
) {
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
            ui.add_space(space::S8);
        }
    }
}

pub(super) fn column_capacity(available_width: f32) -> usize {
    for columns in (2..=3).rev() {
        let gaps = space::S8 * (columns - 1) as f32;
        if (available_width - gaps) / columns as f32 >= MIN_COLUMN_WIDTH {
            return columns;
        }
    }
    1
}

pub(super) fn show_property_cell(
    ui: &mut Ui,
    cell: &DetailCell<'_>,
    response: &mut InspectorDetailsResponse,
) {
    ui.label(
        RichText::new(cell.label.as_ref())
            .font(type_scale::META.font_id(ui.ctx()))
            .color(ui.tokens().text.muted),
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
            unavailable.on_hover_text("This value is unavailable for this resource.");
        }
        DetailValue::Status { text, tone } => {
            ui.horizontal_top(|ui| {
                let (marker_rect, _) =
                    ui.allocate_exact_size(egui::vec2(12.0, 0.0), egui::Sense::hover());
                let label = ui.add(
                    Label::new(
                        RichText::new(text.as_ref())
                            .font(type_scale::META.font_id(ui.ctx()))
                            .color(ui.tokens().text.primary),
                    )
                    .wrap(),
                );
                ui.painter().circle_filled(
                    egui::pos2(marker_rect.center().x, label.rect.center().y),
                    4.0,
                    ui.tokens().tone(*tone).fill,
                );
            });
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
        let metadata_font = type_scale::META.font_id(ui.ctx());
        let metadata_line_height = ui.fonts_mut(|fonts| fonts.row_height(&metadata_font));
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
                                .font(type_scale::META.font_id(ui.ctx()))
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
                    .tint(ui.tokens().text.secondary)
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
