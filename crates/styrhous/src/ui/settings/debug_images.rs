use super::*;

fn centered_table_control_ui(ui: &mut egui::Ui) -> egui::Ui {
    let cell_rect = ui.available_rect_before_wrap();
    let control_rect = egui::Rect::from_min_max(
        egui::pos2(cell_rect.left(), cell_rect.center().y - 16.0),
        egui::pos2(cell_rect.right(), cell_rect.center().y + 16.0),
    );
    ui.new_child(
        egui::UiBuilder::new()
            .max_rect(control_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    )
}

pub(super) fn show_debug_image_preset_table_header(ui: &mut egui::Ui, table_width: f32) {
    let image_column_width = debug_image_column_width(table_width);
    let original_item_spacing = ui.spacing().item_spacing;
    ui.spacing_mut().item_spacing.x = 0.0;
    ui.horizontal(|ui| {
        show_debug_image_table_header_cell(ui, DEBUG_IMAGE_REORDER_COLUMN_WIDTH, "");
        show_debug_image_table_header_cell(ui, DEBUG_IMAGE_NAME_COLUMN_WIDTH, "Name");
        show_debug_image_table_header_cell(ui, image_column_width, "Image");
        show_debug_image_table_header_cell(ui, DEBUG_IMAGE_PROFILE_COLUMN_WIDTH, "Debug profile");
        show_debug_image_table_header_cell(ui, DEBUG_IMAGE_ACTIONS_COLUMN_WIDTH, "");
    });
    ui.spacing_mut().item_spacing = original_item_spacing;
}

pub(super) fn show_debug_image_preset_row(
    ui: &mut egui::Ui,
    table_width: f32,
    preset: &mut DebugImagePreset,
    index: usize,
    drag_handle: &ReorderHandle,
    remove_preset: &mut Option<usize>,
) -> egui::Response {
    let image_column_width = debug_image_column_width(table_width);
    let original_item_spacing = ui.spacing().item_spacing;
    ui.spacing_mut().item_spacing.x = 0.0;
    let response = ui
        .allocate_ui_with_layout(
            egui::vec2(table_width, DEBUG_IMAGE_TABLE_ROW_HEIGHT),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                show_debug_image_table_cell(ui, DEBUG_IMAGE_REORDER_COLUMN_WIDTH, |ui| {
                    let (handle_rect, _) =
                        ui.allocate_exact_size(egui::Vec2::splat(16.0), egui::Sense::hover());
                    let mut handle_ui = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(handle_rect)
                            .layout(egui::Layout::left_to_right(egui::Align::Center)),
                    );
                    icons::bars_3(&mut handle_ui, 16.0, gray::_500);
                    let response = ui
                        .interact(
                            handle_rect,
                            ui.id().with(("debug-image-preset-handle", index)),
                            egui::Sense::drag(),
                        )
                        .with_pointing_hand();
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::Button,
                            ui.is_enabled(),
                            format!("Reorder {}", preset.name),
                        )
                    });
                    drag_handle.register(&response);
                });
                show_debug_image_table_cell(ui, DEBUG_IMAGE_NAME_COLUMN_WIDTH, |ui| {
                    let mut control_ui = centered_table_control_ui(ui);
                    TailwindTextInput::new(&mut preset.name)
                        .id_salt(("debug-image-name", index))
                        .accessibility_label(format!("Debug image {} name", index + 1))
                        .show(&mut control_ui);
                });
                show_debug_image_table_cell(ui, image_column_width, |ui| {
                    let mut control_ui = centered_table_control_ui(ui);
                    TailwindTextInput::new(&mut preset.image)
                        .id_salt(("debug-image-image", index))
                        .accessibility_label(format!("Debug image {} image", index + 1))
                        .show(&mut control_ui);
                });
                show_debug_image_table_cell(ui, DEBUG_IMAGE_PROFILE_COLUMN_WIDTH, |ui| {
                    let mut combobox_ui = centered_table_control_ui(ui);
                    let response = TailwindCombobox::new(("debug-image-profile", index))
                        .accessibility_label(format!("Debug image {} debug profile", index + 1))
                        .selected_text(preset.profile.label())
                        .width(150.0)
                        .compact()
                        .filter_by(|profile: &DebugProfile| profile.label())
                        .show_items(&mut combobox_ui, &DebugProfile::ALL, |options, profile| {
                            if options
                                .item(profile.label(), *profile == preset.profile)
                                .clicked()
                            {
                                preset.profile = *profile;
                            }
                        });
                    response.response.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::ComboBox,
                            combobox_ui.is_enabled(),
                            format!("Debug image {} debug profile", index + 1),
                        )
                    });
                });
                show_debug_image_table_cell(ui, DEBUG_IMAGE_ACTIONS_COLUMN_WIDTH, |ui| {
                    let mut control_ui = centered_table_control_ui(ui);
                    if TailwindButton::icon(
                        icons::trash_icon()
                            .fit_to_exact_size(egui::Vec2::splat(16.0))
                            .tint(status::DANGER),
                    )
                    .variant(ButtonVariant::Secondary)
                    .size(ButtonSize::Sm)
                    .accessibility_label(format!("Remove {}", preset.name))
                    .show(&mut control_ui)
                    .clicked()
                    {
                        *remove_preset = Some(index);
                    }
                });
            },
        )
        .response;
    ui.spacing_mut().item_spacing = original_item_spacing;
    response
}

/// Render the floating drag preview without registering a second set of
/// editable controls or accessibility nodes for the preset.
pub(super) fn show_debug_image_preset_preview_row(
    ui: &mut egui::Ui,
    table_width: f32,
    preset: &DebugImagePreset,
) {
    let image_column_width = debug_image_column_width(table_width);
    let original_item_spacing = ui.spacing().item_spacing;
    ui.spacing_mut().item_spacing.x = 0.0;
    ui.allocate_ui_with_layout(
        egui::vec2(table_width, DEBUG_IMAGE_TABLE_ROW_HEIGHT),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            show_debug_image_table_cell(ui, DEBUG_IMAGE_REORDER_COLUMN_WIDTH, |ui| {
                icons::bars_3(ui, 16.0, gray::_500);
            });
            show_debug_image_table_cell(ui, DEBUG_IMAGE_NAME_COLUMN_WIDTH, |ui| {
                show_debug_image_preview_text_input(ui, &preset.name);
            });
            show_debug_image_table_cell(ui, image_column_width, |ui| {
                show_debug_image_preview_text_input(ui, &preset.image);
            });
            show_debug_image_table_cell(ui, DEBUG_IMAGE_PROFILE_COLUMN_WIDTH, |ui| {
                show_debug_image_preview_combobox(ui, preset.profile.label());
            });
            show_debug_image_table_cell(ui, DEBUG_IMAGE_ACTIONS_COLUMN_WIDTH, |ui| {
                show_debug_image_preview_remove_button(ui);
            });
        },
    );
    ui.spacing_mut().item_spacing = original_item_spacing;
}

pub(super) fn show_debug_image_preview_text_input(ui: &mut egui::Ui, value: &str) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 21.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, radius::control(), WHITE);
    ui.painter().rect_stroke(
        rect,
        radius::control(),
        surface::control_border(),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        rect.left_center() + egui::vec2(4.0, 0.0),
        egui::Align2::LEFT_CENTER,
        value,
        typography::body(),
        gray::_800,
    );
}

pub(super) fn show_debug_image_preview_combobox(ui: &mut egui::Ui, label: &str) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(150.0, 32.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, radius::control(), WHITE);
    ui.painter().rect_stroke(
        rect,
        radius::control(),
        surface::control_border(),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        rect.left_center() + egui::vec2(spacing::MD, 0.0),
        egui::Align2::LEFT_CENTER,
        label,
        typography::metadata(),
        gray::_800,
    );
    let icon_rect = egui::Rect::from_center_size(
        rect.right_center() - egui::vec2(spacing::MD, 0.0),
        egui::Vec2::splat(16.0),
    );
    let mut icon_ui = ui.new_child(egui::UiBuilder::new().max_rect(icon_rect));
    icons::chevron_down(&mut icon_ui, 16.0, gray::_400);
}

pub(super) fn show_debug_image_preview_remove_button(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(32.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, radius::control(), WHITE);
    ui.painter().rect_stroke(
        rect,
        radius::control(),
        surface::control_border(),
        egui::StrokeKind::Inside,
    );
    let icon_rect = egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(16.0));
    let mut icon_ui = ui.new_child(egui::UiBuilder::new().max_rect(icon_rect));
    icon_ui.add(
        icons::trash_icon()
            .fit_to_exact_size(egui::Vec2::splat(16.0))
            .tint(status::DANGER),
    );
}

pub(super) fn debug_image_column_width(table_width: f32) -> f32 {
    (table_width
        - DEBUG_IMAGE_REORDER_COLUMN_WIDTH
        - DEBUG_IMAGE_NAME_COLUMN_WIDTH
        - DEBUG_IMAGE_PROFILE_COLUMN_WIDTH
        - DEBUG_IMAGE_ACTIONS_COLUMN_WIDTH)
        .max(120.0)
}

pub(super) fn show_debug_image_table_header_cell(ui: &mut egui::Ui, width: f32, label: &str) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(width, DEBUG_IMAGE_TABLE_HEADER_HEIGHT),
        egui::Sense::hover(),
    );
    ui.painter().rect_filled(rect, 0.0, TABLE_HEADER_BACKGROUND);
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        egui::Stroke::new(1.0, TABLE_BORDER),
    );
    let mut cell_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    cell_ui.add_space(spacing::LG);
    cell_ui.label(
        egui::RichText::new(label)
            .font(typography::body())
            .color(gray::_900)
            .strong(),
    );
}

pub(super) fn show_debug_image_table_cell(
    ui: &mut egui::Ui,
    width: f32,
    content: impl FnOnce(&mut egui::Ui),
) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(width, DEBUG_IMAGE_TABLE_ROW_HEIGHT),
        egui::Sense::hover(),
    );
    ui.painter().rect_filled(rect, 0.0, CONTENT_BACKGROUND);
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        egui::Stroke::new(1.0, TABLE_BORDER),
    );
    let mut cell_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    cell_ui.add_space(spacing::LG);
    content(&mut cell_ui);
}
