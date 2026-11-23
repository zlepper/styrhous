//! The debug image presets table: a reorderable table (C08) whose cells
//! edit each preset in place with small controls.

use super::*;
use components::CellAlign;

/// The debug image presets, reorderable by their grips. Sets
/// `remove_preset` to a preset whose Remove button was clicked; returns
/// whether a preset was moved.
pub(super) fn show_debug_image_preset_table(
    ui: &mut egui::Ui,
    presets: &mut Vec<DebugImagePreset>,
    remove_preset: &mut Option<usize>,
) -> bool {
    ReorderableTable::new("debug-image-presets")
        .column("name", "Name", |column| {
            column.initial_width(DEBUG_IMAGE_NAME_COLUMN_WIDTH).fields()
        })
        .column("image", "Image", |column| column.fill_remaining().fields())
        .column("profile", "Debug profile", |column| {
            column
                .initial_width(DEBUG_IMAGE_PROFILE_COLUMN_WIDTH)
                .fields()
        })
        .column("actions", "", |column| {
            column
                .initial_width(components::design::size::TABLE_ACTIONS_COLUMN)
                .align(CellAlign::Center)
        })
        .show(
            ui,
            presets,
            |preset| format!("Reorder {}", preset.name),
            |ui, preset, index, column| match column {
                0 => {
                    TextInput::new(&mut preset.name)
                        .size(FieldSize::Sm)
                        .id_salt(("debug-image-name", index))
                        .accessibility_label(format!("Debug image {} name", index + 1))
                        .show(ui);
                }
                1 => {
                    TextInput::new(&mut preset.image)
                        .size(FieldSize::Sm)
                        .id_salt(("debug-image-image", index))
                        .accessibility_label(format!("Debug image {} image", index + 1))
                        .show(ui);
                }
                2 => show_profile_select(ui, preset, index),
                _ => {
                    if Button::icon(icons::Icon::Trash, format!("Remove {}", preset.name))
                        .variant(ButtonVariant::DangerQuiet)
                        .size(ButtonSize::Sm)
                        .show(ui)
                        .clicked()
                    {
                        *remove_preset = Some(index);
                    }
                }
            },
        )
}

fn show_profile_select(ui: &mut egui::Ui, preset: &mut DebugImagePreset, index: usize) {
    let label = format!("Debug image {} debug profile", index + 1);
    let response = Combobox::new(("debug-image-profile", index))
        .accessibility_label(label.clone())
        .selected_text(preset.profile.label())
        .width(ui.available_width())
        .size(FieldSize::Sm)
        .searchable(false)
        .filter_by(|profile: &DebugProfile| profile.label())
        .show_items(ui, &DebugProfile::ALL, |options, profile| {
            if options
                .item(profile.label(), *profile == preset.profile)
                .clicked()
            {
                preset.profile = *profile;
            }
        });
    response.response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, ui.is_enabled(), &label)
    });
}
