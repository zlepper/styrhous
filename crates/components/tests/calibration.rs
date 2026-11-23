//! The calibration scene: `docs/design-oracles/harbour/src/CAL.html` painted with
//! the design APIs (theme roles, type scale, radius, elevation, icons).
//!
//! It is compared with the Chromium render of the same board by
//! `scripts/design-oracles/compare.sh CAL`, which measures the irreducible
//! rasterizer difference and so sets the pixel-match bar for every oracle.
//! Every coordinate below is copied from `CAL.html`; change both together.

use components::design::theme::Tone;
use components::design::theme::{Theme, ThemeExt};
use components::design::type_scale::{self, TextRole, Weight};
use components::design::{radius, size, stroke};
use components::icons;
use components::test_support::{UiHarnessSnapshot, setup_egui};
use egui::{Color32, CornerRadius, Painter, Rect, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};
use egui_kittest::Harness;

/// Left edge and width of the type-sample line boxes (`.line`).
const LINE_LEFT: f32 = 40.0;
const LINE_WIDTH: f32 = 480.0;

fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect::from_min_size(pos2(x, y), vec2(width, height))
}

/// A `.line` sample: the filled line boxes with the text laid out in them.
fn line(ui: &Ui, painter: &Painter, top: f32, role: TextRole, color: Color32, text: &str) {
    let theme = ui.tokens();
    let galley = role.galley(ui.ctx(), text, color, f32::INFINITY);
    let height = galley.rect.height();
    painter.rect_filled(
        rect(LINE_LEFT, top, LINE_WIDTH, height),
        0,
        theme.surface.panel,
    );
    painter.galley(pos2(LINE_LEFT, top), galley, color);
}

/// Text centred in `area` by egui's own layout, the way a flex box with
/// `align-items: center` and `justify-content: center` centres its line box.
fn centred_text(ui: &mut Ui, area: Rect, role: TextRole, color: Color32, text: &str) {
    let text = role.rich_text(ui.ctx(), text).color(color);
    ui.put(area, egui::Label::new(text).selectable(false));
}

fn type_samples(ui: &Ui, painter: &Painter, theme: &Theme) {
    let primary = theme.text.primary;
    let ui_medium = type_scale::UI_CONTROL;
    let samples: [(f32, TextRole, Color32, &str); 22] = [
        (
            32.0,
            type_scale::DISPLAY,
            primary,
            "Display 28/36 Harbour pods",
        ),
        (
            84.0,
            type_scale::HEADING,
            primary,
            "Heading 20/28 Workloads",
        ),
        (128.0, type_scale::TITLE, primary, "Title 16/24 Pod details"),
        (
            168.0,
            type_scale::BODY,
            primary,
            "Body 14/20 The quick brown fox jumps",
        ),
        (
            204.0,
            type_scale::UI,
            primary,
            "UI 13/20 Restart deployment",
        ),
        (236.0, ui_medium, primary, "UI 13/20 medium Namespaces"),
        (
            268.0,
            type_scale::UI.with_weight(Weight::SemiBold),
            primary,
            "UI 13/20 semibold Apply changes",
        ),
        (
            300.0,
            type_scale::META,
            primary,
            "Meta 12/16 Updated 2 min ago",
        ),
        (
            328.0,
            type_scale::CAPTION,
            theme.text.muted,
            "Caption 11/16 Namespace",
        ),
        (
            356.0,
            type_scale::MONO,
            primary,
            "mono 13/20 kube-system 12.5%",
        ),
        (
            388.0,
            type_scale::MONO_SMALL,
            primary,
            "mono 12/18 nginx:1.27.2-alpine",
        ),
        (
            418.0,
            type_scale::MONO
                .with_weight(Weight::Medium)
                .with_size(10.5, 16.0),
            primary,
            "Plex Mono 10.5/16 medium 1.25 GiB",
        ),
        (
            446.0,
            type_scale::MONO.with_size(11.5, 16.0),
            primary,
            "Plex Mono 11.5/16 250m of 500m",
        ),
        (
            474.0,
            type_scale::MONO.with_size(12.5, 16.0),
            primary,
            "Plex Mono 12.5/16 0.42 cores",
        ),
        (
            502.0,
            ui_medium.with_size(13.5, 20.0),
            primary,
            "Figtree 13.5/20 medium Pods",
        ),
        (
            534.0,
            type_scale::UI
                .with_weight(Weight::SemiBold)
                .with_size(13.5, 20.0),
            primary,
            "Figtree 13.5/20 semibold Pods",
        ),
        (
            566.0,
            type_scale::UI,
            primary,
            "Return ↵ Command ⌘ Delete ⌫",
        ),
        (
            598.0,
            type_scale::MONO.with_size(12.0, 16.0),
            primary,
            "mono ↵ ⌘ ⌫ fallback",
        ),
        (
            626.0,
            type_scale::BODY,
            primary,
            "Line one of a paragraph\nLine two continues here\nLine three ends it.",
        ),
        (
            698.0,
            type_scale::CAPTION,
            primary,
            "Tracked caption · column head",
        ),
        (
            726.0,
            type_scale::DISPLAY.with_size(26.0, 32.0),
            primary,
            "Bricolage 26/32 Harbour",
        ),
        (
            770.0,
            type_scale::CARD_TITLE,
            primary,
            "Bricolage 18/26 Harbour",
        ),
    ];
    for (top, role, color, text) in samples {
        line(ui, painter, top, role, color, text);
    }
}

fn pills(ui: &mut Ui, painter: &Painter, theme: &Theme) {
    let button = type_scale::UI.with_weight(Weight::SemiBold);
    let small_button = type_scale::META.with_weight(Weight::SemiBold);
    let mut pill = |area: Rect, fill: Color32, role: TextRole, color: Color32, text: &str| {
        painter.rect_filled(area, radius::FULL, fill);
        centred_text(ui, area, role, color, text);
    };
    pill(
        rect(40.0, 816.0, 120.0, size::CONTROL_MD),
        theme.components.btn_secondary_bg,
        button,
        theme.text.primary,
        "Restart",
    );
    pill(
        rect(176.0, 816.0, 120.0, size::CONTROL_MD),
        theme.accent.fill,
        button,
        theme.text.on_fill,
        "Apply changes",
    );
    pill(
        rect(312.0, 818.0, 96.0, size::CONTROL_SM),
        theme.components.btn_secondary_bg,
        small_button,
        theme.text.primary,
        "Cancel",
    );

    // `.kbd`: content width plus padding, at least as wide as it is tall.
    let key = type_scale::KEY;
    let text_width = key
        .galley(ui.ctx(), "⌘K", theme.text.secondary, f32::INFINITY)
        .rect
        .width();
    let width = (text_width + 2.0 * size::KEY_PADDING_X).max(size::KEY_HEIGHT);
    let area = rect(424.0, 823.0, width, size::KEY_HEIGHT);
    painter.rect_filled(area, radius::KEY, theme.components.key_bg);
    centred_text(ui, area, key, theme.text.secondary, "⌘K");
}

fn radius_scale(painter: &Painter, theme: &Theme) {
    let radii = [
        radius::KEY,
        radius::MENU_ITEM,
        radius::ROW,
        radius::POPOVER,
        radius::CARD,
        radius::OVERLAY,
        radius::FULL,
    ];
    for (index, corner) in radii.into_iter().enumerate() {
        let left = 560.0 + 136.0 * index as f32;
        painter.rect_filled(
            rect(left, 40.0, 120.0, 64.0),
            corner,
            theme.control.fill_strong,
        );
    }
}

fn strokes(painter: &Painter, theme: &Theme) {
    let pixels_per_point = painter.pixels_per_point();
    let hairline = stroke::hairline;
    // CSS borders snap to whole device pixels; the inset ring (a box-shadow) does not.
    let border = |color| stroke::control_border(color, pixels_per_point);
    let border_width = border(theme.control.border).width;
    let ring = |color| Stroke::new(size::CONTROL_STROKE, color);
    painter.rect_stroke(
        rect(560.0, 136.0, 120.0, 64.0),
        radius::ROW,
        hairline(theme.divider.strong),
        StrokeKind::Inside,
    );
    painter.rect_stroke(
        rect(696.0, 136.0, 120.0, 64.0),
        radius::ROW,
        border(theme.control.border),
        StrokeKind::Inside,
    );
    let selected = rect(832.0, 136.0, 120.0, 64.0);
    painter.rect_filled(selected, radius::CARD, theme.item.selected);
    painter.rect_stroke(
        selected,
        radius::CARD,
        ring(theme.accent.fill),
        StrokeKind::Inside,
    );

    let checkbox = Vec2::splat(size::CHECKBOX);
    painter.rect_stroke(
        Rect::from_min_size(pos2(984.0, 160.0), checkbox),
        radius::CHECKBOX,
        border(theme.control.border),
        StrokeKind::Inside,
    );
    painter.rect_filled(
        Rect::from_min_size(pos2(1016.0, 160.0), checkbox),
        radius::CHECKBOX,
        theme.accent.fill,
    );
    let radio = |left: f32| Rect::from_min_size(pos2(left, 160.0), checkbox);
    // `border-radius: 50%` on the border box: an inside stroke of a full pill.
    painter.rect_stroke(
        radio(1048.0),
        radius::FULL,
        border(theme.control.border),
        StrokeKind::Inside,
    );
    painter.circle_filled(
        radio(1080.0).center(),
        size::CHECKBOX / 2.0,
        theme.accent.fill,
    );
    painter.circle_filled(
        radio(1080.0).center(),
        size::RADIO_DOT / 2.0,
        theme.text.on_fill,
    );

    painter.rect_filled(
        rect(1120.0, 152.0, 200.0, size::HAIRLINE),
        0,
        theme.divider.base,
    );
    painter.rect_filled(
        rect(1120.0, 176.0, 200.0, border_width),
        0,
        theme.control.border,
    );
    painter.rect_filled(
        rect(1344.0, 136.0, size::HAIRLINE, 64.0),
        0,
        theme.divider.strong,
    );
}

fn elevation(painter: &Painter, theme: &Theme) {
    let raised = rect(600.0, 264.0, 240.0, 140.0);
    theme.shadow.raised.paint(painter, raised, radius::POPOVER);
    painter.rect_filled(raised, radius::POPOVER, theme.surface.raised);

    let overlay = rect(920.0, 264.0, 280.0, 160.0);
    theme
        .shadow
        .overlay
        .paint(painter, overlay, radius::OVERLAY);
    painter.rect_filled(overlay, radius::OVERLAY, theme.surface.raised);

    let tooltip = rect(1280.0, 264.0, 200.0, 48.0);
    theme
        .shadow
        .tooltip
        .paint(painter, tooltip, radius::MENU_ITEM);
    painter.rect_filled(tooltip, radius::MENU_ITEM, theme.components.tooltip_bg);
}

fn icon_rows(ui: &Ui, painter: &Painter, theme: &Theme) {
    let glyphs = || {
        [
            icons::magnifying_glass_icon(),
            icons::settings_icon(),
            icons::x_mark_icon(),
            icons::chevron_down_icon(),
            icons::check_icon(),
            icons::cube_icon(),
        ]
    };
    // `.icons`: a 20 point high flex row with 24 point gaps, items centred.
    for (top, icon_size) in [(600.0, size::ICON), (640.0, size::ICON_LG)] {
        for (index, glyph) in glyphs().into_iter().enumerate() {
            let min = pos2(
                560.0 + (icon_size + 24.0) * index as f32,
                top + (size::ICON_LG - icon_size) / 2.0,
            );
            glyph
                .tint(theme.text.secondary)
                .paint_at(ui, Rect::from_min_size(min, Vec2::splat(icon_size)));
        }
    }

    let tile = |left: f32, glyph, tone| {
        let area = Rect::from_min_size(pos2(left, 688.0), Vec2::splat(size::TILE));
        icons::paint_tile(ui, painter, area, glyph, tone, radius::TILE);
    };
    tile(560.0, icons::circle_stack_icon(), Tone::Accent);
    tile(612.0, icons::globe_alt_icon(), Tone::Accent);
    tile(664.0, icons::settings_icon(), Tone::Neutral);
}

fn calibration_scene(ui: &mut Ui) {
    let theme = ui.tokens();
    let painter = ui.ctx().layer_painter(egui::LayerId::background());
    painter.rect_filled(
        ui.ctx().content_rect(),
        CornerRadius::ZERO,
        theme.surface.canvas,
    );
    type_samples(ui, &painter, &theme);
    pills(ui, &painter, &theme);
    radius_scale(&painter, &theme);
    strokes(&painter, &theme);
    elevation(&painter, &theme);
    icon_rows(ui, &painter, &theme);
}

#[test]
fn calibration_board_matches_the_design_apis() {
    let mut harness = Harness::builder().build_ui(calibration_scene);
    setup_egui(&mut harness);
    harness.run();
    harness.ui_harness("calibration/harbour_calibration");
}

/// The calibration scene on a 2× display: borders snap to whole device
/// pixels and text baselines round in device pixels, as Chromium does at a
/// device scale factor of 2.
#[test]
fn calibration_board_at_2x() {
    let mut harness = Harness::builder()
        .with_pixels_per_point(2.0)
        .build_ui(calibration_scene);
    setup_egui(&mut harness);
    harness.run();
    assert_eq!(
        harness
            .ctx
            .global_style()
            .visuals
            .widgets
            .inactive
            .bg_stroke
            .width,
        1.5,
        "the 1.5 px control border paints 3 device pixels wide"
    );
    harness.ui_harness("calibration/harbour_calibration_2x");
}
