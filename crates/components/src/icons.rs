//! SVG icons for the components library.
//!
//! The outline icons are the design's own set — Heroicons outline, stroke
//! width 1.5 — extracted byte for byte from the Harbour stylesheet by
//! `scripts/design-oracles/extract-icons.mjs` into `assets/icons/`, so egui
//! and the design oracles rasterize identical paths. They are drawn in white
//! and tinted with a theme role. A few app-only icons (Heroicons the design
//! does not use, and provider brand marks) live next to them.
//!
//! Icons render through the egui_extras SVG loader, which rasterizes at the
//! exact pixel size the image is shown at. The consuming application must
//! install the image loaders once at startup:
//! ```ignore
//! egui_extras::install_image_loaders(&cc.egui_ctx);
//! ```
//!
//! # Usage
//!
//! ```ignore
//! icons::chevron_right(ui, 16.0, ui.tokens().text.muted);
//! ```

use egui::{Color32, CornerRadius, Image, Painter, Rect, Response, Ui, Vec2, include_image};

use crate::PointingHand;
use crate::design::theme::{Theme, Tone};

macro_rules! icons {
    ($($(#[$doc:meta])* $name:ident => $file:literal;)+) => {
        $(
            $(#[$doc])*
            pub fn $name() -> Image<'static> {
                Image::new(include_image!(concat!("../assets/icons/", $file)))
            }
        )+
    };
}

icons! {
    /// Home.
    home_icon => "home.svg";
    /// Users.
    users_icon => "users.svg";
    /// Folder.
    folder_icon => "folder.svg";
    /// Calendar.
    calendar_icon => "calendar.svg";
    /// Document.
    document_icon => "document.svg";
    /// Code bracket, for literal values.
    code_bracket_icon => "code-bracket.svg";
    /// Key, for secret-backed values.
    key_icon => "key.svg";
    /// Document text, for Kubernetes field references.
    document_text_icon => "document-text.svg";
    /// Question mark, for unknown or unspecified values.
    question_mark_circle_icon => "question-mark-circle.svg";
    /// Bar chart.
    chart_bar_icon => "chart-bar.svg";
    /// Horizontal ellipsis, for compact action menus.
    ellipsis_horizontal_icon => "ellipsis-horizontal.svg";
    /// Left arrow, for backwards navigation.
    arrow_left_icon => "arrow-left.svg";
    /// Right arrow, for forwards navigation.
    arrow_right_icon => "arrow-right.svg";
    /// Up arrow.
    arrow_up_icon => "arrow-up.svg";
    /// Down arrow.
    arrow_down_icon => "arrow-down.svg";
    /// Circular arrow, for reload actions.
    arrow_path_icon => "arrow-path.svg";
    /// Chevron right, for navigation affordances.
    chevron_right_icon => "chevron-right.svg";
    /// Chevron down.
    chevron_down_icon => "chevron-down.svg";
    /// Funnel, for filtering controls.
    funnel_icon => "funnel.svg";
    /// Numbered list, for line-number visibility controls.
    numbered_list_icon => "numbered-list.svg";
    /// Calendar days, for timestamp visibility controls.
    calendar_days_icon => "calendar-days.svg";
    /// Swatch, for ANSI styling controls.
    swatch_icon => "swatch.svg";
    /// X mark, for dismissing overlays and panels.
    x_mark_icon => "x-mark.svg";
    /// Trash, for destructive actions.
    trash_icon => "trash.svg";
    /// Pencil, for edit actions.
    pencil_icon => "pencil.svg";
    /// Plus, for add controls.
    plus_icon => "plus.svg";
    /// Cog, for application settings.
    settings_icon => "cog-6-tooth.svg";
    /// Bell, for the session operation history.
    bell_icon => "bell.svg";
    /// Document duplicate, for copy actions.
    document_duplicate_icon => "document-duplicate.svg";
    /// Magnifying glass.
    magnifying_glass_icon => "magnifying-glass.svg";
    /// Check mark.
    check_icon => "check.svg";
    /// Cube, a Kubernetes resource.
    cube_icon => "cube.svg";
    /// Globe.
    globe_alt_icon => "globe-alt.svg";
    /// Circle stack: contexts and clusters.
    circle_stack_icon => "circle-stack.svg";
    /// Horizontal adjustments: presentation settings.
    adjustments_horizontal_icon => "adjustments-horizontal.svg";
    /// Share: cluster discovery.
    share_icon => "share.svg";
    /// Wrench and screwdriver: application preferences.
    wrench_screwdriver_icon => "wrench-screwdriver.svg";
    /// User circle: license and account.
    user_circle_icon => "user-circle.svg";
    /// The bundled Microsoft Azure brand mark for Azure provider surfaces.
    azure_icon => "azure.svg";
    /// The bundled Tailscale brand mark for Tailscale provider surfaces.
    tailscale_icon => "tailscale.svg";
}

/// Show `image` at `size` points, tinted `color`.
fn show(ui: &mut Ui, image: Image<'static>, size: f32, color: Color32) -> Response {
    ui.add(image.fit_to_exact_size(Vec2::splat(size)).tint(color))
}

/// Render a chevron-right icon `size` points square, tinted `color`.
pub fn chevron_right(ui: &mut Ui, size: f32, color: Color32) -> Response {
    show(ui, chevron_right_icon(), size, color)
}

/// Render a chevron-down icon `size` points square, tinted `color`.
pub fn chevron_down(ui: &mut Ui, size: f32, color: Color32) -> Response {
    show(ui, chevron_down_icon(), size, color)
}

/// Render a chevron-up icon `size` points square, tinted `color`.
pub fn chevron_up(ui: &mut Ui, size: f32, color: Color32) -> Response {
    show(
        ui,
        Image::new(include_image!("../assets/icons/chevron-up.svg")),
        size,
        color,
    )
}

/// Render a bars-3 (hamburger menu) icon `size` points square, tinted `color`.
pub fn bars_3(ui: &mut Ui, size: f32, color: Color32) -> Response {
    show(
        ui,
        Image::new(include_image!("../assets/icons/bars-3.svg")),
        size,
        color,
    )
}

/// Render a trash (delete) icon (non-interactive). For a clickable version,
/// use [`trash_button`].
pub fn trash(ui: &mut Ui, size: f32, color: Color32) -> Response {
    show(ui, trash_icon(), size, color)
}

/// Render a pencil (edit) icon (non-interactive). For a clickable version,
/// use [`pencil_button`].
pub fn pencil(ui: &mut Ui, size: f32, color: Color32) -> Response {
    show(ui, pencil_icon(), size, color)
}

/// A clickable, frameless trash (delete) icon button labelled `label` for
/// assistive technologies.
pub fn trash_button(ui: &mut Ui, size: f32, color: Color32, label: &str) -> Response {
    icon_button(ui, trash_icon(), size, color, label)
}

/// A clickable, frameless pencil (edit) icon button labelled `label` for
/// assistive technologies.
pub fn pencil_button(ui: &mut Ui, size: f32, color: Color32, label: &str) -> Response {
    icon_button(ui, pencil_icon(), size, color, label)
}

/// A clickable, frameless eye icon button labelled `label` for assistive
/// technologies.
pub fn eye_button(ui: &mut Ui, size: f32, color: Color32, label: &str) -> Response {
    icon_button(
        ui,
        Image::new(include_image!("../assets/icons/eye.svg")),
        size,
        color,
        label,
    )
}

fn icon_button(
    ui: &mut Ui,
    image: Image<'static>,
    size: f32,
    color: Color32,
    label: &str,
) -> Response {
    let image = image.fit_to_exact_size(Vec2::splat(size)).tint(color);
    let response = ui
        .add(egui::Button::image(image).frame(false))
        .with_pointing_hand();
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    response
}

/// The fill and glyph colours of an icon tile in `tone` (the design's
/// `.tile` variants): accent and info tiles are tinted with a coloured glyph
/// (`.tile`), neutral tiles use the strong control fill with a secondary
/// glyph (`.tile.gray`), and status tiles are solid with a dark glyph
/// (`.tile.ok`, `.tile.warn`, `.tile.bad`).
pub fn tile_colors(theme: &Theme, tone: Tone) -> (Color32, Color32) {
    let colors = theme.tone(tone);
    match tone {
        Tone::Accent | Tone::Info => (colors.tint, colors.text),
        Tone::Neutral => (theme.control.fill_strong, theme.text.secondary),
        Tone::Success | Tone::Warning | Tone::Danger => (colors.fill, theme.text.on_fill),
    }
}

/// Glyph size relative to the tile: a 20 point glyph on a 36 point tile.
const TILE_GLYPH_RATIO: f32 = 20.0 / 36.0;

/// Paint an icon tile into `rect`: a rounded square in `tone` with `glyph`
/// centred on it, tinted to match.
pub fn paint_tile(
    ui: &Ui,
    painter: &Painter,
    rect: Rect,
    glyph: Image<'static>,
    tone: Tone,
    corner_radius: impl Into<CornerRadius>,
) {
    let (fill, glyph_color) = tile_colors(&crate::design::theme::ThemeExt::tokens(ui), tone);
    painter.rect_filled(rect, corner_radius, fill);
    let glyph_size = (rect.height() * TILE_GLYPH_RATIO).round();
    let glyph_rect = Rect::from_center_size(rect.center(), Vec2::splat(glyph_size));
    glyph.tint(glyph_color).paint_at(ui, glyph_rect);
}

/// Paints a check mark `width` points wide centred on `center`, as drawn for selected
/// options and completed steps.
pub fn paint_check_mark(
    painter: &egui::Painter,
    center: egui::Pos2,
    width: f32,
    stroke: egui::Stroke,
) {
    let scale = width / 13.0;
    let [start, corner, end] = [
        Vec2::new(-6.0, 0.0),
        Vec2::new(-1.5, 4.5),
        Vec2::new(7.0, -5.0),
    ]
    .map(|offset| center + offset * scale);
    painter.line_segment([start, corner], stroke);
    painter.line_segment([corner, end], stroke);
}
