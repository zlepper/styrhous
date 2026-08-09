//! Shared interaction affordances for custom egui controls.

use egui::{CursorIcon, Response};

/// Adds the standard pointing-hand cursor to a clickable response.
pub trait PointingHand {
    /// Mark an enabled clickable response with the standard pointing-hand cursor.
    ///
    /// Disabled egui responses are never hovered, so they retain the platform's
    /// default cursor without additional branching at every call site.
    fn with_pointing_hand(self) -> Response;
}

impl PointingHand for Response {
    fn with_pointing_hand(self) -> Response {
        self.on_hover_cursor(CursorIcon::PointingHand)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_button(enabled: bool) -> egui::PlatformOutput {
        let ctx = egui::Context::default();
        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(120.0, 80.0));
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(screen_rect),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.add_enabled(enabled, egui::Button::new("Action"))
                        .with_pointing_hand();
                });
            },
        );
        ctx.run(
            egui::RawInput {
                screen_rect: Some(screen_rect),
                events: vec![egui::Event::PointerMoved(egui::pos2(16.0, 16.0))],
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.add_enabled(enabled, egui::Button::new("Action"))
                        .with_pointing_hand();
                });
            },
        )
        .platform_output
    }

    #[test]
    fn pointing_hand_is_only_emitted_for_enabled_hovered_actions() {
        assert_eq!(render_button(true).cursor_icon, CursorIcon::PointingHand);
        assert_eq!(render_button(false).cursor_icon, CursorIcon::Default);
    }
}
