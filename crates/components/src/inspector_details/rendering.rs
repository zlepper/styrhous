use super::*;

pub(super) fn show_property_rows(
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

pub(super) fn show_group_title(ui: &mut Ui, group_rect: Rect, title: &str) {
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
            ui.add_space(spacing::SM);
        }
    }
}

pub(super) fn column_capacity(available_width: f32) -> usize {
    for columns in (2..=3).rev() {
        let gaps = spacing::SM * (columns - 1) as f32;
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

pub(super) fn show_value(
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

pub(super) fn show_text_value(
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

pub(super) fn tone_color(tone: DetailTone) -> egui::Color32 {
    match tone {
        DetailTone::Neutral => gray::_400,
        DetailTone::Success => status::SUCCESS,
        DetailTone::Warning => status::WARNING,
        DetailTone::Danger => status::DANGER,
    }
}

pub(super) fn show_table_row(
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
