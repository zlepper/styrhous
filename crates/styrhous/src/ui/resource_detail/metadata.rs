use super::*;

pub(super) fn pod_phase_tone(phase: &str) -> Tone {
    match phase {
        "Running" | "Succeeded" => Tone::Success,
        "Pending" => Tone::Warning,
        "Failed" => Tone::Danger,
        _ => Tone::Neutral,
    }
}

pub(super) fn event_tone(event_type: &str) -> Tone {
    match event_type {
        "Normal" => Tone::Success,
        "Warning" => Tone::Warning,
        _ => Tone::Neutral,
    }
}

pub(super) fn condition_tone(condition_status: &str) -> Tone {
    match condition_status {
        "True" => Tone::Success,
        "False" => Tone::Neutral,
        "Unknown" => Tone::Warning,
        _ => Tone::Neutral,
    }
}

pub(super) fn section_header(ui: &mut egui::Ui, title: &str, detail: Option<String>) {
    let theme = ui.tokens();
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(title)
                .strong()
                .font(type_scale::TITLE.font_id(ui.ctx()))
                .color(theme.text.primary),
        );
        if let Some(detail) = detail {
            ui.label(
                egui::RichText::new(detail)
                    .font(type_scale::BODY.font_id(ui.ctx()))
                    .color(theme.text.secondary),
            );
        }
    });
    ui.add_space(6.0);
}
