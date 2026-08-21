use super::*;

pub(super) fn show_settings_introduction(ui: &mut egui::Ui) {
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

pub(super) fn show_update_status(ui: &mut egui::Ui, update_status: &UpdateStatus) {
    if matches!(
        update_status,
        UpdateStatus::LocalBuild | UpdateStatus::NotIncluded
    ) {
        return;
    }

    ui.add_space(spacing::XL);
    ui.label(
        egui::RichText::new("Application updates")
            .font(typography::section_heading())
            .color(gray::_900),
    );
    ui.add_space(spacing::XS);
    ui.label(
        egui::RichText::new(update_status.summary())
            .font(typography::body())
            .color(gray::_600),
    );
}

pub(super) fn show_terminal_launcher(
    ui: &mut egui::Ui,
    draft: &mut TerminalLaunchSettings,
    error: &mut Option<String>,
) {
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
    ui.add_space(spacing::LG);
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
        &mut draft.debug_image_presets,
        table_width,
        |ui, preset, index, handle| {
            show_debug_image_preset_row(ui, table_width, preset, index, handle, &mut remove_preset);
        },
        |ui, preset| show_debug_image_preset_preview_row(ui, table_width, preset),
    );
    ui.spacing_mut().item_spacing = original_item_spacing;
    if moved {
        *error = None;
    }
    if let Some(index) = remove_preset {
        draft.debug_image_presets.remove(index);
        *error = None;
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
        ui.add_space(spacing::MD);
        show_validation_error(ui, "Debug image settings need attention", error);
    }
}
