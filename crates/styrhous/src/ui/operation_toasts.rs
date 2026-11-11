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
            ui.horizontal_top(|ui| {
                let row_origin = ui.cursor().min;
                let item_spacing = ui.spacing().item_spacing.x;
                ui.add_space(TOAST_STATUS_TILE_SIZE + spacing::MD + item_spacing);
                let text_rect = ui
                    .scope_builder(
                        egui::UiBuilder::new().layout(
                            egui::Layout::top_down(egui::Align::LEFT)
                                .with_main_align(egui::Align::Center),
                        ),
                        |ui| {
                            ui.set_min_height(TOAST_STATUS_TILE_SIZE);
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
                        },
                    )
                    .response
                    .rect;
                // The tile is centred against the (possibly wrapped) text block.
                let tile_rect = egui::Rect::from_min_size(
                    egui::pos2(
                        row_origin.x,
                        text_rect.center().y - TOAST_STATUS_TILE_SIZE / 2.0,
                    ),
                    egui::Vec2::splat(TOAST_STATUS_TILE_SIZE),
                );
                let mut tile_ui = ui.new_child(egui::UiBuilder::new().max_rect(tile_rect));
                show_status_tile(&mut tile_ui, toast.outcome, TOAST_STATUS_TILE_SIZE);
            });
        });
}
