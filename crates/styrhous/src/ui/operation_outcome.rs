use super::state::OperationOutcome;
use components::design::theme::{ThemeExt, Tone};
use components::design::{radius, type_scale};

pub(super) fn show_status_tile(ui: &mut egui::Ui, outcome: OperationOutcome, size: f32) {
    let theme = ui.tokens();
    let (tone, glyph) = match outcome {
        OperationOutcome::Success => (Tone::Success, "✓"),
        OperationOutcome::Failure => (Tone::Danger, "×"),
    };
    let colors = theme.tone(tone);
    let (color, fill) = (colors.text, colors.tint);
    let (icon_rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    ui.painter()
        .rect_filled(icon_rect, egui::CornerRadius::same(radius::KEY), fill);
    ui.painter()
        .circle_stroke(icon_rect.center(), 9.0, egui::Stroke::new(1.8, color));
    ui.painter().text(
        icon_rect.center(),
        egui::Align2::CENTER_CENTER,
        glyph,
        type_scale::BODY_STRONG.font_id(ui.ctx()),
        color,
    );
}
