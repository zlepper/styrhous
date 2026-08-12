use super::state::UiState;
use crate::terminal_launcher::{DebugImagePreset, DebugProfile, TerminalLaunchSettings};
use components::colors::{
    CONTENT_BACKGROUND, TABLE_BORDER, TABLE_HEADER_BACKGROUND, WHITE, gray, indigo,
};
use components::design::{radius, spacing, status, surface, typography};
use components::{
    BladeNavigator, BladeStack, ButtonSize, ButtonVariant, PointingHand, ReorderHandle,
    ReorderableTable, TailwindButton, TailwindCombobox, icons,
};

const FOOTER_HEIGHT: f32 = 52.0;
const CHOICE_CONTENT_MIN_HEIGHT: f32 = 44.0;
const DEBUG_IMAGE_TABLE_HEADER_HEIGHT: f32 = 40.0;
const DEBUG_IMAGE_TABLE_ROW_HEIGHT: f32 = 44.0;
const DEBUG_IMAGE_REORDER_COLUMN_WIDTH: f32 = 44.0;
const DEBUG_IMAGE_NAME_COLUMN_WIDTH: f32 = 170.0;
const DEBUG_IMAGE_PROFILE_COLUMN_WIDTH: f32 = 170.0;
const DEBUG_IMAGE_ACTIONS_COLUMN_WIDTH: f32 = 52.0;

/// Render application settings as a first-class workspace blade rather than a
/// transient native dialog, so its controls have room for explanation.
pub(super) fn show(
    ctx: &egui::Context,
    ui_state: &mut UiState,
    settings: &mut TerminalLaunchSettings,
) {
    if !ui_state.terminal_settings_open {
        return;
    }

    let mut close = ctx.input(|input| input.key_pressed(egui::Key::Escape));
    let mut save = false;
    let mut reset = false;
    let stack = BladeStack::new("settings-blade");
    let mut blade = ui_state
        .terminal_settings_blade
        .take()
        .unwrap_or_else(|| BladeNavigator::new(()));
    let response = stack.show_with_title(
        ctx,
        &mut blade,
        |_| "Settings".to_owned(),
        |ui, _, _| {
            show_settings_introduction(ui);
            ui.add_space(spacing::XL);
            ui.separator();
            ui.add_space(spacing::XL);

            let content_height = (ui.available_height() - FOOTER_HEIGHT).max(120.0);
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), content_height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    components::scroll::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            show_terminal_launcher(ui, ui_state);
                            ui.add_space(spacing::XL);
                            ui.separator();
                            ui.add_space(spacing::XL);
                            show_debug_image_presets(ui, ui_state);
                        });
                },
            );

            ui.separator();
            ui.add_space(spacing::SM);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                save |= TailwindButton::new("Save changes").show(ui).clicked();
                reset |= TailwindButton::secondary("Reset")
                    .size(ButtonSize::Md)
                    .show(ui)
                    .clicked();
            });
        },
    );
    close |= response.dismissed;

    if reset {
        ui_state.terminal_settings_draft = TerminalLaunchSettings::default();
        ui_state.terminal_settings_error = None;
    }
    if save {
        match ui_state.terminal_settings_draft.validate() {
            Ok(()) => {
                *settings = ui_state.terminal_settings_draft.clone();
                ui_state.terminal_settings_error = None;
                close = true;
            }
            Err(error) => ui_state.terminal_settings_error = Some(error),
        }
    }
    if close && blade.begin_close() {
        stack.seed_transition(ctx, &mut blade);
    }
    if response.close_finished {
        ui_state.terminal_settings_open = false;
        ui_state.terminal_settings_blade = None;
    } else {
        ui_state.terminal_settings_blade = Some(blade);
    }
}

fn show_settings_introduction(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("APPLICATION")
            .font(typography::metadata())
            .color(gray::_500),
    );
    ui.add_space(spacing::XS);
    ui.label(
        egui::RichText::new(
            "Configure local tools and display preferences.\nThese settings apply only to this application on this device.",
        )
        .font(typography::body())
        .color(gray::_600),
    );
}

fn show_terminal_launcher(ui: &mut egui::Ui, ui_state: &mut UiState) {
    ui.label(
        egui::RichText::new("Terminal launcher")
            .font(typography::section_heading())
            .color(gray::_900),
    );
    ui.add_space(spacing::SM);
    ui.label(
        egui::RichText::new("Choose how shells open on this computer.")
            .font(typography::body())
            .color(gray::_600),
    );
    ui.add_space(spacing::XL);

    let automatic = ui_state.terminal_settings_draft.custom_template.is_none();
    if launcher_choice(
        ui,
        automatic,
        "Automatic",
        "Use your system’s preferred terminal.",
        None,
        false,
        None,
    ) {
        ui_state.terminal_settings_draft.custom_template = None;
        ui_state.terminal_settings_error = None;
    }
    ui.add_space(spacing::LG);
    let custom_launcher_clicked = {
        let template_error = ui_state.terminal_settings_error.clone();
        let template_error =
            template_error.filter(|error| error.starts_with("The launcher template"));
        let template_invalid = template_error.is_some();
        let template = ui_state.terminal_settings_draft.custom_template.as_mut();
        launcher_choice(
            ui,
            !automatic,
            "Custom launcher",
            "Use a command template for your preferred terminal.",
            template,
            template_invalid,
            template_error.as_deref(),
        )
    };
    if custom_launcher_clicked && ui_state.terminal_settings_draft.custom_template.is_none() {
        ui_state.terminal_settings_draft.custom_template = Some(String::new());
    }
}

fn show_debug_image_presets(ui: &mut egui::Ui, ui_state: &mut UiState) {
    ui.label(
        egui::RichText::new("Debug images")
            .font(typography::section_heading())
            .color(gray::_900),
    );
    ui.add_space(spacing::SM);
    ui.label(
        egui::RichText::new(
            "Choose the images and profiles offered for Debug images and Pod debug shells. Node sessions create a debug Pod with the node filesystem available at /host; Pod sessions add an ephemeral debug container.",
        )
        .font(typography::body())
        .color(gray::_600),
    );
    ui.add_space(spacing::LG);

    let mut remove_preset = None;
    let table_width = ui.available_width();
    let original_item_spacing = ui.spacing().item_spacing;
    ui.spacing_mut().item_spacing.y = 0.0;
    show_debug_image_preset_table_header(ui, table_width);
    let moved = ReorderableTable::new("debug-image-presets", DEBUG_IMAGE_TABLE_ROW_HEIGHT).show(
        ui,
        &mut ui_state.terminal_settings_draft.debug_image_presets,
        table_width,
        |ui, preset, index, handle| {
            show_debug_image_preset_row(ui, table_width, preset, index, handle, &mut remove_preset);
        },
        |ui, preset| show_debug_image_preset_preview_row(ui, table_width, preset),
    );
    ui.spacing_mut().item_spacing = original_item_spacing;
    if moved {
        ui_state.terminal_settings_error = None;
    }
    if let Some(index) = remove_preset {
        ui_state
            .terminal_settings_draft
            .debug_image_presets
            .remove(index);
        ui_state.terminal_settings_error = None;
    }
    if TailwindButton::icon(
        icons::plus_icon()
            .fit_to_exact_size(egui::Vec2::splat(16.0))
            .tint(indigo::_600),
    )
    .variant(ButtonVariant::Secondary)
    .size(ButtonSize::Sm)
    .accessibility_label("Add debug image")
    .show(ui)
    .clicked()
    {
        ui_state
            .terminal_settings_draft
            .debug_image_presets
            .push(DebugImagePreset {
                name: String::new(),
                image: String::new(),
                profile: DebugProfile::General,
            });
        ui_state.terminal_settings_error = None;
    }
    if let Some(error) = ui_state
        .terminal_settings_error
        .as_deref()
        .filter(|error| !error.starts_with("The launcher template"))
    {
        ui.add_space(spacing::MD);
        show_validation_error(ui, "Debug image settings need attention", error);
    }
}

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

fn show_debug_image_preset_table_header(ui: &mut egui::Ui, table_width: f32) {
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

fn show_debug_image_preset_row(
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
                    show_table_text_input(
                        &mut control_ui,
                        &mut preset.name,
                        ("debug-image-name", index),
                        format!("Debug image {} name", index + 1),
                    );
                });
                show_debug_image_table_cell(ui, image_column_width, |ui| {
                    let mut control_ui = centered_table_control_ui(ui);
                    show_table_text_input(
                        &mut control_ui,
                        &mut preset.image,
                        ("debug-image-image", index),
                        format!("Debug image {} image", index + 1),
                    );
                });
                show_debug_image_table_cell(ui, DEBUG_IMAGE_PROFILE_COLUMN_WIDTH, |ui| {
                    let mut combobox_ui = centered_table_control_ui(ui);
                    let response = TailwindCombobox::new(("debug-image-profile", index))
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
fn show_debug_image_preset_preview_row(
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

fn show_debug_image_preview_text_input(ui: &mut egui::Ui, value: &str) {
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

fn show_debug_image_preview_combobox(ui: &mut egui::Ui, label: &str) {
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

fn show_debug_image_preview_remove_button(ui: &mut egui::Ui) {
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

fn debug_image_column_width(table_width: f32) -> f32 {
    (table_width
        - DEBUG_IMAGE_REORDER_COLUMN_WIDTH
        - DEBUG_IMAGE_NAME_COLUMN_WIDTH
        - DEBUG_IMAGE_PROFILE_COLUMN_WIDTH
        - DEBUG_IMAGE_ACTIONS_COLUMN_WIDTH)
        .max(120.0)
}

fn show_debug_image_table_header_cell(ui: &mut egui::Ui, width: f32, label: &str) {
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

fn show_debug_image_table_cell(ui: &mut egui::Ui, width: f32, content: impl FnOnce(&mut egui::Ui)) {
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

fn show_table_text_input(
    ui: &mut egui::Ui,
    value: &mut String,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    accessibility_label: String,
) {
    let id = ui.make_persistent_id(id_salt);
    let focused = ui.memory(|memory| memory.has_focus(id));
    let stroke = if focused {
        egui::Stroke::new(1.0, indigo::_500)
    } else {
        surface::control_border()
    };
    let response = egui::Frame::new()
        .fill(WHITE)
        .stroke(stroke)
        .corner_radius(radius::control())
        .inner_margin(egui::Margin::symmetric(
            spacing::SM as i8,
            spacing::XS as i8,
        ))
        .show(ui, |ui| {
            ui.add_sized(
                egui::vec2(ui.available_width(), 20.0),
                egui::TextEdit::singleline(value).id(id),
            )
        })
        .inner;
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::TextEdit,
            ui.is_enabled(),
            accessibility_label.clone(),
        )
    });
}

fn launcher_choice(
    ui: &mut egui::Ui,
    selected: bool,
    title: &str,
    description: &str,
    template: Option<&mut String>,
    template_invalid: bool,
    template_error: Option<&str>,
) -> bool {
    let stroke = if selected {
        egui::Stroke::new(1.0, indigo::_500)
    } else {
        surface::muted_border()
    };
    egui::Frame::new()
        .fill(if selected { indigo::_50 } else { WHITE })
        .stroke(stroke)
        .corner_radius(radius::surface())
        .inner_margin(egui::Margin::same(spacing::XXL as i8))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            let choice_clicked = ui
                .allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), CHOICE_CONTENT_MIN_HEIGHT),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.spacing_mut().icon_width = 20.0;
                        ui.spacing_mut().icon_width_inner = 12.0;
                        let response = ui.radio(selected, "").with_pointing_hand();
                        response.widget_info(|| {
                            egui::WidgetInfo::selected(
                                egui::WidgetType::RadioButton,
                                ui.is_enabled(),
                                selected,
                                title,
                            )
                        });
                        ui.add_space(spacing::SM);
                        ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new(title)
                                    .font(typography::semibold(18.0))
                                    .color(gray::_800),
                            );
                            ui.add_space(spacing::XS);
                            ui.label(
                                egui::RichText::new(description)
                                    .font(typography::body())
                                    .color(gray::_600),
                            );
                        });
                        response.clicked()
                    },
                )
                .inner;
            if let Some(template) = template {
                ui.add_space(spacing::XXL);
                ui.label(
                    egui::RichText::new("Command template")
                        .font(typography::body())
                        .color(gray::_800),
                );
                ui.add_space(spacing::SM);
                show_command_template_input(ui, template, template_invalid);
                ui.add_space(spacing::MD);
                egui::Frame::new()
                    .fill(indigo::_100)
                    .stroke(egui::Stroke::new(1.0, indigo::_200))
                    .corner_radius(radius::surface())
                    .inner_margin(egui::Margin::same(spacing::LG as i8))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.horizontal_top(|ui| {
                            let (icon_rect, _) = ui
                                .allocate_exact_size(egui::Vec2::splat(20.0), egui::Sense::hover());
                            ui.painter().circle_stroke(
                                icon_rect.center(),
                                9.0,
                                egui::Stroke::new(1.5, indigo::_600),
                            );
                            ui.painter().text(
                                icon_rect.center(),
                                egui::Align2::CENTER_CENTER,
                                "i",
                                typography::semibold(14.0),
                                indigo::_600,
                            );
                            ui.add_space(spacing::SM);
                            ui.label(template_guidance());
                        });
                    });
                if let Some(error) = template_error {
                    ui.add_space(spacing::LG);
                    show_validation_error(ui, "Command template needs attention", error);
                }
            }
            choice_clicked
        })
        .inner
}

fn show_command_template_input(
    ui: &mut egui::Ui,
    template: &mut String,
    invalid: bool,
) -> egui::Response {
    let id = ui.make_persistent_id("terminal-command-template");
    let focused = ui.memory(|memory| memory.has_focus(id));
    let stroke = if invalid {
        egui::Stroke::new(1.0, status::DANGER)
    } else if focused {
        egui::Stroke::new(1.0, indigo::_500)
    } else {
        surface::control_border()
    };
    let response = egui::Frame::new()
        .fill(WHITE)
        .stroke(stroke)
        .corner_radius(radius::control())
        .inner_margin(egui::Margin::symmetric(
            spacing::MD as i8,
            spacing::SM as i8,
        ))
        .show(ui, |ui| {
            let full_width = ui.available_width() + spacing::XL;
            ui.set_min_width(full_width);
            ui.add_sized(
                egui::vec2(full_width, 20.0),
                egui::TextEdit::singleline(template)
                    .id(id)
                    .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(4, 2)))
                    .font(typography::monospace())
                    .text_color(gray::_800)
                    .hint_text("alacritty -e {command}"),
            )
        })
        .inner;
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::TextEdit,
            ui.is_enabled(),
            "Command template",
        )
    });
    response
}

fn template_guidance() -> egui::text::LayoutJob {
    let mut guidance = egui::text::LayoutJob::default();
    let text = egui::TextFormat {
        font_id: typography::body(),
        color: indigo::_800,
        ..Default::default()
    };
    guidance.append("Use ", 0.0, text.clone());
    guidance.append(
        "{command}",
        0.0,
        egui::TextFormat {
            font_id: typography::monospace(),
            color: indigo::_900,
            ..Default::default()
        },
    );
    guidance.append(
        " as the placeholder.\nIt is replaced with the complete kubectl shell command.",
        0.0,
        text,
    );
    guidance
}

fn show_validation_error(ui: &mut egui::Ui, title: &str, error: &str) {
    egui::Frame::new()
        .fill(WHITE)
        .stroke(egui::Stroke::new(1.0, status::DANGER))
        .corner_radius(radius::control())
        .inner_margin(egui::Margin::same(spacing::MD as i8))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(
                egui::RichText::new(title)
                    .font(typography::semibold(typography::BODY_SIZE))
                    .color(status::DANGER),
            );
            ui.add_space(spacing::XS);
            ui.label(
                egui::RichText::new(error)
                    .font(typography::metadata())
                    .color(gray::_700),
            );
        });
}
