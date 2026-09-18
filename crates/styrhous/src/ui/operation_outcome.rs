use super::state::OperationOutcome;
use components::design::{radius, status, typography};

pub(super) fn show_status_tile(ui: &mut egui::Ui, outcome: OperationOutcome, size: f32) {
    let (color, fill, glyph) = match outcome {
        OperationOutcome::Success => (status::SUCCESS, status::SUCCESS_SOFT, "✓"),
        OperationOutcome::Failure => (status::DANGER, status::DANGER_SOFT, "×"),
    };
    let (icon_rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    ui.painter().rect_filled(icon_rect, radius::control(), fill);
    ui.painter()
        .circle_stroke(icon_rect.center(), 9.0, egui::Stroke::new(1.8, color));
    ui.painter().text(
        icon_rect.center(),
        egui::Align2::CENTER_CENTER,
        glyph,
        typography::semibold_or_proportional(ui.ctx(), 14.0),
        color,
    );
}
