use super::*;

/// A launcher option: a radio card whose body, while it is selected, holds
/// the option's command template. Returns whether the card was clicked.
pub(super) fn launcher_choice(
    ui: &mut egui::Ui,
    selected: bool,
    title: &str,
    description: &str,
    template: Option<&mut String>,
    template_error: Option<&str>,
) -> bool {
    let body = template.map(|template| {
        move |ui: &mut egui::Ui| {
            FormField::new("Command template")
                .help("Use {command} where the kubectl command goes. It is already quoted.")
                .error(template_error)
                .show(ui, |ui| {
                    show_command_template_input(ui, template, template_error.is_some());
                });
        }
    });
    RadioCard::new(selected, title, description)
        .show_with_body(ui, body)
        .response
        .clicked()
}

pub(super) fn show_command_template_input(
    ui: &mut egui::Ui,
    template: &mut String,
    invalid: bool,
) -> egui::Response {
    TextInput::new(template)
        .id_salt("terminal-command-template")
        .monospace()
        .placeholder("alacritty -e {command}")
        .accessibility_label("Command template")
        .invalid(invalid)
        .width(ui.available_width().min(TEMPLATE_INPUT_WIDTH))
        .show(ui)
}

pub(super) fn show_validation_error(ui: &mut egui::Ui, title: &str, error: &str) {
    components::Callout::new(components::design::theme::Tone::Danger, title)
        .body(error)
        .show(ui);
}
