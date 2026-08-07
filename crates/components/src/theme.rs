//! Application-wide visual defaults for the Tailwind-inspired components.

use egui::{Context, Stroke, Visuals};

use crate::colors::{gray, indigo, WHITE};

/// Apply the light visual theme used by this component library's references.
///
/// The components use Tailwind's light palette for their fills, borders, and
/// text. Configuring the surrounding egui context to the same baseline avoids
/// dark pane backgrounds showing through table gaps and component spacing.
pub fn apply_light_theme(ctx: &Context) {
    let mut visuals = Visuals::light();
    visuals.panel_fill = WHITE;
    visuals.window_fill = WHITE;
    visuals.extreme_bg_color = WHITE;
    visuals.faint_bg_color = gray::_50;
    visuals.code_bg_color = gray::_50;
    visuals.window_stroke = Stroke::new(1.0, gray::_200);
    visuals.hyperlink_color = indigo::_600;

    ctx.set_visuals(visuals);
}
