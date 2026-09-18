use super::operation_outcome::show_status_tile;
use super::state::{TransientOperationToast, UiState};
use components::colors::{WHITE, gray};
use components::design::{radius, spacing, surface, typography};
use egui::{Align2, Area, Color32, Frame, Id, Margin, Order, Shadow};
use std::time::Instant;

const TOAST_WIDTH: f32 = 360.0;
const TOAST_INSET: f32 = 24.0;
const TOAST_GAP: f32 = spacing::MD;
const TOAST_STATUS_TILE_SIZE: f32 = 40.0;
const TOAST_CONTENT_MIN_HEIGHT: f32 = 48.0;

pub(super) fn show(ctx: &egui::Context, ui_state: &mut UiState) {
    let now = Instant::now();
    let toasts = ui_state.visible_operation_toasts(now);
    let Some(next_expiration) = toasts.iter().map(|toast| toast.expires_at).min() else {
        return;
    };
    ctx.request_repaint_after(next_expiration.saturating_duration_since(now));

    Area::new(Id::new("operation-outcome-toasts"))
        .order(Order::Foreground)
        .anchor(Align2::RIGHT_BOTTOM, egui::vec2(-TOAST_INSET, -TOAST_INSET))
        .interactable(false)
        .show(ctx, |ui| {
            ui.set_width(TOAST_WIDTH);
            for (index, toast) in toasts.iter().enumerate() {
                toast_card(ui, toast);
                if index + 1 < toasts.len() {
                    ui.add_space(TOAST_GAP);
                }
            }
        });
}

fn toast_card(ui: &mut egui::Ui, toast: &TransientOperationToast) {
    Frame::new()
        .fill(WHITE)
        .stroke(surface::muted_border())
        .corner_radius(radius::surface())
        .shadow(Shadow {
            offset: [0, 3],
            blur: 12,
            spread: 0,
            color: Color32::BLACK.gamma_multiply(0.12),
        })
        .inner_margin(Margin::same(spacing::LG as i8))
        .show(ui, |ui| {
            ui.set_min_width(TOAST_WIDTH - 2.0 * spacing::LG);
            ui.set_min_height(TOAST_CONTENT_MIN_HEIGHT);
            ui.horizontal(|ui| {
                show_status_tile(ui, toast.outcome, TOAST_STATUS_TILE_SIZE);
                ui.add_space(spacing::MD);
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(&toast.title)
                            .font(typography::semibold(typography::BODY_SIZE))
                            .color(gray::_900),
                    );
                    ui.add_space(2.0);
                    ui.label(
                        egui::RichText::new(&toast.target)
                            .font(typography::metadata())
                            .color(gray::_500),
                    );
                });
            });
        });
}
