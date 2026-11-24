use super::*;
use crate::ui::blade_text::{blade_section, gap};
use components::Switch;

/// The opening sentence of the Application settings blade.
pub(super) const INTRODUCTION: &str = "Local tools used by Styrhous on this device.";

/// The "Terminal launcher" section: the system terminal or a custom command
/// template, as two radio cards.
pub(super) fn show_terminal_launcher(
    ui: &mut egui::Ui,
    draft: &mut TerminalLaunchSettings,
    error: &mut Option<String>,
) {
    blade_section(ui, "Terminal launcher", Some("where shells open"));
    let automatic = draft.custom_template.is_none();
    let automatic_description = format!(
        "Use your system’s preferred terminal ({}).",
        crate::terminal_launcher::default_terminal_name()
    );
    if launcher_choice(
        ui,
        automatic,
        "Automatic",
        &automatic_description,
        None,
        None,
    ) {
        draft.custom_template = None;
        *error = None;
    }
    gap(ui, space::S8);
    let custom_launcher_clicked = {
        let template_error = error
            .as_deref()
            .filter(|error| error.starts_with("The launcher template"));
        launcher_choice(
            ui,
            !automatic,
            "Custom launcher",
            "Use a command template for your preferred terminal.",
            draft.custom_template.as_mut(),
            template_error,
        )
    };
    if custom_launcher_clicked && draft.custom_template.is_none() {
        draft.custom_template = Some(String::new());
    }
}

/// The "Debug images" section: the presets offered for node and Pod debug
/// shells, as a reorderable table with an "Add image" button.
pub(super) fn show_debug_image_presets(
    ui: &mut egui::Ui,
    draft: &mut TerminalLaunchSettings,
    error: &mut Option<String>,
) {
    blade_section(
        ui,
        "Debug images",
        Some("offered for node and pod debug shells"),
    );
    let mut remove_preset = None;
    let moved =
        show_debug_image_preset_table(ui, &mut draft.debug_image_presets, &mut remove_preset);
    if moved {
        *error = None;
    }
    if let Some(index) = remove_preset {
        draft.debug_image_presets.remove(index);
        *error = None;
    }
    // The table's last row keeps its 4 point gap, then the button follows
    // (the table itself keeps a point under its rows).
    gap(ui, space::S12 - 1.0);
    if Button::soft("Add image")
        .leading_icon(icons::Icon::Plus)
        .size(ButtonSize::Sm)
        .show(ui)
        .clicked()
    {
        draft.debug_image_presets.push(DebugImagePreset {
            name: String::new(),
            image: String::new(),
            profile: DebugProfile::General,
        });
        *error = None;
    }
    if let Some(error) = error
        .as_deref()
        .filter(|error| !error.starts_with("The launcher template"))
    {
        ui.add_space(space::S12);
        show_validation_error(ui, "Debug image settings need attention", error);
    }
}

/// The "Updates" section: whether the application checks for updates on its
/// own, and how the current check went.
pub(super) fn show_updates(
    ui: &mut egui::Ui,
    draft: &mut TerminalLaunchSettings,
    update_status: &UpdateStatus,
) {
    blade_section(ui, "Updates", None);
    Switch::new(
        &mut draft.check_for_updates,
        "Check for updates automatically",
    )
    .show(ui);
    if matches!(
        update_status,
        UpdateStatus::LocalBuild | UpdateStatus::NotIncluded
    ) {
        return;
    }
    gap(ui, space::S8);
    type_scale::META.label(ui, &update_status.summary(), ui.tokens().text.muted);
}
