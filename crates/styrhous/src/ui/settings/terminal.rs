use super::*;

pub(super) fn show_settings_introduction(ui: &mut egui::Ui) {
    let theme = ui.tokens();
    ui.label(
        egui::RichText::new("APPLICATION")
            .font(type_scale::META.font_id(ui.ctx()))
            .color(theme.text.muted),
    );
    ui.add_space(space::S4);
    ui.label(
        egui::RichText::new(
            "Configure local tools and display preferences.\nThese settings apply only to this application on this device.",
        )
        .font(type_scale::BODY.font_id(ui.ctx()))
        .color(theme.text.secondary),
    );
}

pub(super) fn show_update_status(ui: &mut egui::Ui, update_status: &UpdateStatus) {
    let theme = ui.tokens();
    if matches!(
        update_status,
        UpdateStatus::LocalBuild | UpdateStatus::NotIncluded
    ) {
        return;
    }

    ui.add_space(space::S24);
    ui.label(
        egui::RichText::new("Application updates")
            .font(type_scale::TITLE.font_id(ui.ctx()))
            .color(theme.text.primary),
    );
    ui.add_space(space::S4);
    ui.label(
        egui::RichText::new(update_status.summary())
            .font(type_scale::BODY.font_id(ui.ctx()))
            .color(theme.text.secondary),
    );
}

pub(super) fn show_terminal_launcher(
    ui: &mut egui::Ui,
    draft: &mut TerminalLaunchSettings,
    error: &mut Option<String>,
) {
    let theme = ui.tokens();
    ui.label(
        egui::RichText::new("Terminal launcher")
            .font(type_scale::TITLE.font_id(ui.ctx()))
            .color(theme.text.primary),
    );
    ui.add_space(space::S8);
    ui.label(
        egui::RichText::new("Choose how shells open on this computer.")
            .font(type_scale::BODY.font_id(ui.ctx()))
            .color(theme.text.secondary),
    );
    ui.add_space(space::S24);

    let automatic = draft.custom_template.is_none();
    if launcher_choice(
        ui,
        automatic,
        "Automatic",
        "Use your system’s preferred terminal.",
        None,
        false,
        None,
    ) {
        draft.custom_template = None;
        *error = None;
    }
    ui.add_space(space::S16);
    let custom_launcher_clicked = {
        let template_error = error.clone();
        let template_error =
            template_error.filter(|error| error.starts_with("The launcher template"));
        let template_invalid = template_error.is_some();
        let template = draft.custom_template.as_mut();
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
    if custom_launcher_clicked && draft.custom_template.is_none() {
        draft.custom_template = Some(String::new());
    }
}

pub(super) fn show_debug_image_presets(
    ui: &mut egui::Ui,
    draft: &mut TerminalLaunchSettings,
    error: &mut Option<String>,
) {
    let theme = ui.tokens();
    ui.label(
        egui::RichText::new("Debug images")
            .font(type_scale::TITLE.font_id(ui.ctx()))
            .color(theme.text.primary),
    );
    ui.add_space(space::S8);
    ui.label(
        egui::RichText::new(
            "Choose the images and profiles offered for Debug images and Pod debug shells. Node sessions create a debug Pod with the node filesystem available at /host; Pod sessions add an ephemeral debug container.",
        )
        .font(type_scale::BODY.font_id(ui.ctx()))
        .color(theme.text.secondary),
    );
    ui.add_space(space::S16);

    let mut remove_preset = None;
    let moved =
        show_debug_image_preset_table(ui, &mut draft.debug_image_presets, &mut remove_preset);
    ui.add_space(space::S12);
    if moved {
        *error = None;
    }
    if let Some(index) = remove_preset {
        draft.debug_image_presets.remove(index);
        *error = None;
    }
    if Button::icon(icons::Icon::Plus, "Add debug image")
        .variant(ButtonVariant::Soft)
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
