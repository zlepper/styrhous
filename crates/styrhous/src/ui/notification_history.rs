use super::global_blade::{GlobalBladeContent, GlobalBladeRenderContext, GlobalBladeRenderResult};
use super::operation_outcome::status_tile;
use super::state::OperationHistoryDisplayEntry;
use components::design::theme::ThemeExt;
use components::design::{space, type_scale};
use components::tooltip::{TooltipExt, elided_label};
use components::{BladeLayer, Card};
use egui::Align;

/// A history entry's status tile (A12 `.tile.sm`).
const HISTORY_STATUS_TILE: components::TileSize = components::TileSize::Sm;
const HISTORY_CARD_CONTENT_HEIGHT: f32 = 76.0;
const HISTORY_TWO_LINE_TEXT_HEIGHT: f32 = 37.0;
const HISTORY_THREE_LINE_TEXT_HEIGHT: f32 = 59.0;

/// The session-scoped record of resource operation outcomes. It intentionally
/// has no destructive controls: opening it merely marks the existing entries
/// as read, while reconnecting a cluster starts a new session.
#[derive(Debug, Default)]
pub(crate) struct NotificationHistoryBlade;

impl GlobalBladeContent for NotificationHistoryBlade {
    fn render_header(
        &mut self,
        ui: &mut egui::Ui,
        _layer: BladeLayer,
        _context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult {
        components::blade_title(ui, None, "Notifications");
        GlobalBladeRenderResult::default()
    }

    fn render_body(
        &mut self,
        ui: &mut egui::Ui,
        _layer: BladeLayer,
        context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult {
        let theme = ui.tokens();
        ui.label(
            egui::RichText::new("Recent resource operation outcomes from this session.")
                .font(type_scale::BODY.font_id(ui.ctx()))
                .color(theme.text.secondary),
        );
        ui.add_space(space::S24);
        ui.separator();
        ui.add_space(space::S24);
        ui.label(
            egui::RichText::new("TODAY")
                .font(type_scale::BODY_STRONG.font_id(ui.ctx()))
                .color(theme.text.muted),
        );
        ui.add_space(space::S12);

        let entries = context.operation_history_entries();
        if entries.is_empty() {
            ui.label(
                egui::RichText::new("Resource operation outcomes will appear here.")
                    .font(type_scale::BODY.font_id(ui.ctx()))
                    .color(theme.text.muted),
            );
        } else {
            for (index, entry) in entries.iter().enumerate() {
                notification_card(ui, entry);
                if index + 1 < entries.len() {
                    ui.add_space(space::S8 + 6.0);
                }
            }
        }
        GlobalBladeRenderResult::default()
    }
}

fn notification_card(ui: &mut egui::Ui, display: &OperationHistoryDisplayEntry) {
    let entry = &display.entry;
    let theme = ui.tokens();
    Card::new().show_inner(ui, |ui| {
        let content_rect = egui::Rect::from_min_size(
            ui.cursor().min,
            egui::vec2(ui.available_width(), HISTORY_CARD_CONTENT_HEIGHT),
        );
        ui.allocate_rect(content_rect, egui::Sense::hover());
        let mut content_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(content_rect)
                .layout(egui::Layout::left_to_right(Align::Center)),
        );
        status_tile(entry.outcome)
            .size(HISTORY_STATUS_TILE)
            .show(&mut content_ui);
        content_ui.add_space(space::S8);
        let text_rect = egui::Rect::from_min_size(
            content_ui.cursor().min,
            egui::vec2(content_ui.available_width(), HISTORY_CARD_CONTENT_HEIGHT),
        );
        content_ui.allocate_rect(text_rect, egui::Sense::hover());
        let mut text_ui = content_ui.new_child(
            egui::UiBuilder::new()
                .max_rect(text_rect)
                .layout(egui::Layout::top_down(Align::LEFT).with_main_align(Align::Center)),
        );
        let ui = &mut text_ui;
        let text_height = if entry.details.is_some() {
            HISTORY_THREE_LINE_TEXT_HEIGHT
        } else {
            HISTORY_TWO_LINE_TEXT_HEIGHT
        };
        ui.add_space((HISTORY_CARD_CONTENT_HEIGHT - text_height) / 2.0);
        let header_width = ui.available_width();
        let metadata = format!("{} · Just now", display.cluster_name);
        let metadata_width = ui
            .painter()
            .layout_no_wrap(
                metadata.clone(),
                type_scale::META.font_id(ui.ctx()),
                theme.text.muted,
            )
            .size()
            .x
            .min(header_width * 0.45);
        let title_width = (header_width - metadata_width - space::S8).max(0.0);
        let title_response = ui
            .allocate_ui_with_layout(
                egui::vec2(header_width, 0.0),
                egui::Layout::left_to_right(Align::Center),
                |ui| {
                    let title_response = ui.allocate_ui_with_layout(
                        egui::vec2(title_width, 0.0),
                        egui::Layout::left_to_right(Align::Center),
                        |ui| {
                            elided_label(
                                ui,
                                type_scale::BODY_STRONG,
                                &entry.title,
                                theme.text.primary,
                            )
                        },
                    );
                    ui.add_space(space::S8 + metadata_width);
                    title_response.inner
                },
            )
            .inner;
        let metadata_rect = egui::Rect::from_center_size(
            egui::pos2(
                ui.max_rect().right() - metadata_width / 2.0,
                title_response.rect.center().y,
            ),
            egui::vec2(metadata_width, title_response.rect.height()),
        );
        let mut metadata_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(metadata_rect)
                .layout(egui::Layout::right_to_left(Align::Center)),
        );
        elided_label(
            &mut metadata_ui,
            type_scale::META,
            &metadata,
            theme.text.muted,
        );
        ui.add_space(2.0);
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), 0.0),
            egui::Layout::left_to_right(Align::Center),
            |ui| {
                elided_label(ui, type_scale::META, &entry.target, theme.text.muted);
            },
        );
        if let Some(details) = &entry.details {
            ui.add_space(space::S4);
            let summary = details.lines().next().unwrap_or_default();
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), 0.0),
                egui::Layout::left_to_right(Align::Center),
                |ui| {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(summary)
                                .font(type_scale::META.font_id(ui.ctx()))
                                .color(theme.text.secondary),
                        )
                        .truncate()
                        .show_tooltip_when_elided(false)
                        .halign(Align::LEFT),
                    )
                    .tooltip(details);
                },
            );
        }
    });
}
