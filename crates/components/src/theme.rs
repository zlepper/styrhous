//! Application-wide visual defaults for the Tailwind-inspired components.

use egui::{Context, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle, Visuals};
use std::sync::Arc;

use crate::colors::{CONTENT_BACKGROUND, TABLE_BORDER, gray, indigo};

/// A true semibold face for headings and selected labels.
///
/// `RichText::strong()` only changes egui's text color; it does not select a
/// bold font face. Callers that need typographic hierarchy should use this.
pub fn semibold_font(size: f32) -> FontId {
    crate::design::typography::semibold(size)
}

fn apply_inter_fonts(ctx: &Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "inter-regular".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/inter/Inter-Regular.ttf"
        ))),
    );
    fonts.font_data.insert(
        "inter-semibold".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/inter/Inter-SemiBold.ttf"
        ))),
    );

    // Inter is the deterministic application face. Keep egui's emoji fallbacks
    // after it so resource names remain robust for non-Latin text.
    fonts.families.insert(
        FontFamily::Proportional,
        vec![
            "inter-regular".into(),
            "NotoEmoji-Regular".into(),
            "emoji-icon-font".into(),
        ],
    );
    fonts.families.insert(
        FontFamily::Name("Inter SemiBold".into()),
        vec![
            "inter-semibold".into(),
            "inter-regular".into(),
            "NotoEmoji-Regular".into(),
            "emoji-icon-font".into(),
        ],
    );
    ctx.set_fonts(fonts);
}

/// Apply the light visual theme used by this component library's references.
///
/// The components use Tailwind's light palette for their fills, borders, and
/// text. Configuring the surrounding egui context to the same baseline avoids
/// dark pane backgrounds showing through table gaps and component spacing.
pub fn apply_light_theme(ctx: &Context) {
    apply_inter_fonts(ctx);
    ctx.set_theme(egui::Theme::Light);

    ctx.style_mut_of(egui::Theme::Light, |style| {
        style
            .text_styles
            .insert(TextStyle::Small, crate::design::typography::metadata());
        style
            .text_styles
            .insert(TextStyle::Body, crate::design::typography::body());
        style
            .text_styles
            .insert(TextStyle::Button, crate::design::typography::body());
        style
            .text_styles
            .insert(TextStyle::Heading, crate::design::typography::page_title());
        style
            .text_styles
            .insert(TextStyle::Monospace, crate::design::typography::monospace());
    });

    let mut visuals = Visuals::light();
    visuals.panel_fill = CONTENT_BACKGROUND;
    visuals.window_fill = CONTENT_BACKGROUND;
    visuals.extreme_bg_color = CONTENT_BACKGROUND;
    visuals.faint_bg_color = gray::_50;
    visuals.code_bg_color = gray::_50;
    visuals.window_stroke = Stroke::new(1.0, TABLE_BORDER);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, TABLE_BORDER);
    visuals.hyperlink_color = indigo::_600;

    ctx.set_visuals_of(egui::Theme::Light, visuals);
}
