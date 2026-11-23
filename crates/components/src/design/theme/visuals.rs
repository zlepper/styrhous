//! Mapping theme roles onto egui's own [`Visuals`] and text styles, so stock
//! egui widgets (labels, scroll bars, text edits, windows) match the theme.

use egui::style::{WidgetVisuals, Widgets};
use egui::{Color32, Context, CornerRadius, Stroke, TextStyle, Visuals};

use super::Theme;
use crate::design::{radius, size, stroke, type_scale};

/// Opacity of disabled controls (S00: "45% opacity of rest").
const DISABLED_ALPHA: f32 = 0.45;

/// Coverage gamma for glyph rasterization (`alpha = coverage ^ gamma`).
///
/// Measured on the calibration board (`docs/design-oracles/harbour/src/CAL.html`):
/// with unhinted outlines (the oracles render with `--font-render-hinting=none`)
/// this matches Chromium's text weight within 2 %; egui's dark-mode default
/// renders 10–30 % heavier and raw coverage 8 % lighter.
const TEXT_GAMMA: f32 = 0.85;

/// egui [`Visuals`] drawn from `theme`'s roles.
pub fn visuals(theme: &Theme, pixels_per_point: f32) -> Visuals {
    let base = if theme.is_dark() {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    let raised_ring = theme.shadow.raised.ring.unwrap_or(Stroke::NONE);

    let mut visuals = Visuals {
        dark_mode: theme.is_dark(),
        override_text_color: Some(theme.text.primary),
        weak_text_color: Some(theme.text.muted),
        widgets: widgets(theme, pixels_per_point),
        hyperlink_color: theme.accent.text,
        warn_fg_color: theme.warning.text,
        error_fg_color: theme.danger.text,
        faint_bg_color: theme.surface.panel,
        extreme_bg_color: theme.components.field_bg,
        text_edit_bg_color: Some(theme.components.field_bg),
        code_bg_color: theme.code.surface,
        panel_fill: theme.surface.canvas,
        window_fill: theme.surface.raised,
        window_stroke: raised_ring,
        window_shadow: theme.shadow.overlay.shadow,
        window_corner_radius: CornerRadius::same(radius::OVERLAY),
        popup_shadow: theme.shadow.raised.shadow,
        menu_corner_radius: CornerRadius::same(radius::POPOVER),
        disabled_alpha: DISABLED_ALPHA,
        text_options: egui::epaint::text::TextOptions {
            font_hinting: false,
            color_transfer_function: egui::epaint::FontColorTransferFunction::Gamma(TEXT_GAMMA),
            ..base.text_options
        },
        ..base
    };
    visuals.selection.bg_fill = theme.accent.tint;
    visuals.selection.stroke = Stroke::new(1.0, theme.accent.text);
    visuals.text_cursor.stroke.color = theme.code.caret;
    visuals
}

/// Widget visuals per interaction state.
///
/// Buttons fill with `weak_bg_fill`; checkboxes and radios fill with
/// `bg_fill` and outline with `bg_stroke` (C06: a transparent 16 point box
/// with a 1.5 px `control-border`, `control-border-hover` plus a
/// `control-fill` wash on hover). Text edits with egui's default frame and
/// framed stock buttons pick up the same border; the components draw their
/// own frames.
fn widgets(theme: &Theme, pixels_per_point: f32) -> Widgets {
    let border = |color| stroke::control_border(color, pixels_per_point);
    let control = |weak_fill: Color32, fill: Color32, stroke: Stroke| WidgetVisuals {
        bg_fill: fill,
        weak_bg_fill: weak_fill,
        bg_stroke: stroke,
        corner_radius: CornerRadius::same(radius::FULL),
        fg_stroke: Stroke::new(1.0, theme.text.primary),
        expansion: 0.0,
    };
    let hover_border = border(theme.control.border_hover);
    Widgets {
        noninteractive: WidgetVisuals {
            bg_stroke: stroke::divider(theme),
            corner_radius: CornerRadius::same(radius::CARD),
            ..control(theme.surface.panel, theme.surface.panel, Stroke::NONE)
        },
        inactive: control(
            theme.control.fill,
            Color32::TRANSPARENT,
            border(theme.control.border),
        ),
        hovered: control(theme.control.fill_hover, theme.control.fill, hover_border),
        active: control(
            theme.control.fill_active,
            theme.control.fill_hover,
            hover_border,
        ),
        open: control(
            theme.control.fill_active,
            theme.control.fill_hover,
            hover_border,
        ),
    }
}

/// Apply `theme` to egui: select its dark or light mode, replace that mode's
/// visuals with [`visuals`], set the text styles from the type scale and give
/// stock buttons the design's small control padding.
///
/// Text styles use the Harbour font families; install them with
/// [`crate::design::fonts::install`] (as [`super::configure_default`] does).
/// Prefer [`super::switch`], which also makes `theme` the theme components
/// read; this only updates egui's own style.
pub(crate) fn apply_theme(ctx: &Context, theme: &Theme) {
    let mode = if theme.is_dark() {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    };
    ctx.set_theme(mode);
    let pixels_per_point = ctx.pixels_per_point();
    ctx.style_mut_of(mode, |style| {
        style.visuals = visuals(theme, pixels_per_point);
        style.spacing.button_padding = size::BUTTON_PADDING_SM;
        crate::tooltip::configure_style(style);
        for (text_style, role) in [
            (TextStyle::Small, type_scale::META),
            (TextStyle::Body, type_scale::UI),
            (TextStyle::Button, type_scale::UI),
            (TextStyle::Heading, type_scale::HEADING),
            (TextStyle::Monospace, type_scale::MONO),
        ] {
            style
                .text_styles
                .insert(text_style, role.registered_font_id());
        }
    });
    super::context::record_applied_pixels_per_point(ctx, pixels_per_point);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design::theme::{Theme, ThemeExt, switch};
    use std::sync::Arc;

    #[test]
    fn apply_theme_switches_egui_to_the_themes_dark_visuals() {
        let ctx = Context::default();
        let theme = Theme::default();
        apply_theme(&ctx, &theme);

        assert_eq!(ctx.theme(), egui::Theme::Dark);
        let style = ctx.global_style();
        let visuals = &style.visuals;
        assert!(visuals.dark_mode);
        assert_eq!(visuals.panel_fill, theme.surface.canvas);
        assert_eq!(visuals.window_fill, theme.surface.raised);
        assert_eq!(visuals.extreme_bg_color, theme.components.field_bg);
        assert_eq!(visuals.code_bg_color, theme.code.surface);
        assert_eq!(visuals.override_text_color, Some(theme.text.primary));
        assert_eq!(visuals.hyperlink_color, theme.accent.text);
        assert_eq!(visuals.selection.bg_fill, theme.accent.tint);
        // Buttons fill with `weak_bg_fill`.
        assert_eq!(visuals.widgets.inactive.weak_bg_fill, theme.control.fill);
        assert_eq!(
            visuals.widgets.hovered.weak_bg_fill,
            theme.control.fill_hover
        );
        assert_eq!(
            visuals.widgets.active.weak_bg_fill,
            theme.control.fill_active
        );
        // Checkboxes and radios (C06): transparent with a control border,
        // a control-fill wash and the hover border on hover.
        let inactive = visuals.widgets.inactive;
        assert_eq!(inactive.bg_fill, Color32::TRANSPARENT);
        assert_eq!(inactive.bg_stroke.color, theme.control.border);
        assert_eq!(inactive.bg_stroke.width, 1.0, "1.5 px CSS border at 1x");
        assert_eq!(visuals.widgets.hovered.bg_fill, theme.control.fill);
        assert_eq!(
            visuals.widgets.hovered.bg_stroke.color,
            theme.control.border_hover
        );
        assert!(!visuals.text_options.font_hinting);
        assert_eq!(
            visuals.widgets.noninteractive.bg_stroke.color,
            theme.divider.base
        );
        assert_eq!(visuals.popup_shadow, theme.shadow.raised.shadow);
        assert_eq!(visuals.window_shadow, theme.shadow.overlay.shadow);
        assert_eq!(
            visuals.window_stroke.color,
            crate::design::palette::WHITE_A04
        );

        assert_eq!(
            style.text_styles[&TextStyle::Body].size,
            type_scale::UI.size()
        );
        assert_eq!(
            style.text_styles[&TextStyle::Heading].size,
            type_scale::HEADING.size()
        );
        assert_eq!(
            style.text_styles[&TextStyle::Monospace].family,
            egui::FontFamily::Name(type_scale::MONO.family_name().into())
        );
    }

    #[test]
    fn switch_applies_visuals_and_installs_the_theme_together() {
        let ctx = Context::default();
        switch(&ctx, Arc::new(Theme::default()));

        let switched = Arc::new(
            Theme::from_file_str(
                r##"
                name = "Darker canvas"
                [palette]
                sand-950 = "#000000"
                "##,
            )
            .unwrap(),
        );
        switch(&ctx, Arc::clone(&switched));

        assert_eq!(ctx.global_style().visuals.panel_fill, Color32::BLACK);
        assert_eq!(ctx.tokens().surface.canvas, Color32::BLACK);
        assert_eq!(ctx.tokens().name(), "Darker canvas");
    }

    #[test]
    fn stock_buttons_get_the_small_control_padding() {
        let ctx = Context::default();
        switch(&ctx, Arc::new(Theme::default()));
        assert_eq!(
            ctx.global_style().spacing.button_padding,
            size::BUTTON_PADDING_SM
        );
    }

    /// Runs one pass at `pixels_per_point` and returns the inactive border width
    /// the pass rendered with.
    fn border_width_in_pass(ctx: &Context, pixels_per_point: f32) -> (f32, f32) {
        let mut input = egui::RawInput::default();
        input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .native_pixels_per_point = Some(pixels_per_point);
        let mut seen = (0.0, 0.0);
        let mut output = ctx.run_ui(input, |ui| {
            seen = (
                ui.visuals().widgets.inactive.bg_stroke.width,
                ui.ctx()
                    .global_style()
                    .visuals
                    .widgets
                    .inactive
                    .bg_stroke
                    .width,
            );
        });
        output.textures_delta.clear();
        seen
    }

    #[test]
    fn a_new_display_scale_rebuilds_the_scale_dependent_visuals() {
        let ctx = Context::default();
        switch(&ctx, Arc::new(Theme::default()));

        // CSS snaps the 1.5 px control border to whole device pixels.
        assert_eq!(border_width_in_pass(&ctx, 1.0), (1.0, 1.0));
        // The pass that first sees the new scale already renders with it,
        // in the root `Ui` as well as the global style.
        assert_eq!(border_width_in_pass(&ctx, 2.0), (1.5, 1.5));
        assert_eq!(border_width_in_pass(&ctx, 2.0), (1.5, 1.5));
        assert_eq!(border_width_in_pass(&ctx, 1.0), (1.0, 1.0));
    }

    #[test]
    fn the_scale_sync_waits_for_a_theme() {
        let ctx = Context::default();
        let mut output = ctx.run_ui(Default::default(), |ui| {
            assert!(!crate::design::theme::sync_pixels_per_point(ui.ctx()));
        });
        output.textures_delta.clear();
    }
}
