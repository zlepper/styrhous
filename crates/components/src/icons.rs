//! SVG icons for the components library
//!
//! Uses Heroicons (https://heroicons.com/) for consistent iconography.
//! Icons are embedded as SVG and rendered via egui_extras image loaders.
//!
//! # Setup
//!
//! The consuming application must install the image loaders once at startup:
//! ```ignore
//! egui_extras::install_image_loaders(&cc.egui_ctx);
//! ```
//!
//! # Usage
//!
//! ```ignore
//! icons::chevron_right(ui, 16.0, gray::_400);
//! ```

use egui::{Color32, Image, Response, Ui, Vec2, include_image};

/// Render a chevron-right icon
///
/// # Arguments
/// * `ui` - The UI to render into
/// * `size` - Icon size in pixels
/// * `color` - Icon color (applied as tint)
pub fn chevron_right(ui: &mut Ui, size: f32, color: Color32) -> Response {
    let image = Image::new(include_image!("icons/chevron-right.svg"))
        .fit_to_exact_size(Vec2::splat(size))
        .tint(color);
    ui.add(image)
}

/// Render a chevron-down icon
///
/// # Arguments
/// * `ui` - The UI to render into
/// * `size` - Icon size in pixels
/// * `color` - Icon color (applied as tint)
pub fn chevron_down(ui: &mut Ui, size: f32, color: Color32) -> Response {
    let image = Image::new(include_image!("icons/chevron-down.svg"))
        .fit_to_exact_size(Vec2::splat(size))
        .tint(color);
    ui.add(image)
}

/// Render a chevron-up icon
///
/// # Arguments
/// * `ui` - The UI to render into
/// * `size` - Icon size in pixels
/// * `color` - Icon color (applied as tint)
pub fn chevron_up(ui: &mut Ui, size: f32, color: Color32) -> Response {
    let image = Image::new(include_image!("icons/chevron-up.svg"))
        .fit_to_exact_size(Vec2::splat(size))
        .tint(color);
    ui.add(image)
}

/// Render a bars-3 (hamburger menu) icon
///
/// # Arguments
/// * `ui` - The UI to render into
/// * `size` - Icon size in pixels
/// * `color` - Icon color (applied as tint)
pub fn bars_3(ui: &mut Ui, size: f32, color: Color32) -> Response {
    let image = Image::new(include_image!("icons/bars-3.svg"))
        .fit_to_exact_size(Vec2::splat(size))
        .tint(color);
    ui.add(image)
}

// Icon factory functions - return unsized Image for use in components

/// Returns a home icon image
pub fn home_icon() -> Image<'static> {
    Image::new(include_image!("icons/home.svg"))
}

/// Returns a users icon image
pub fn users_icon() -> Image<'static> {
    Image::new(include_image!("icons/users.svg"))
}

/// Returns a folder icon image
pub fn folder_icon() -> Image<'static> {
    Image::new(include_image!("icons/folder.svg"))
}

/// Returns a calendar icon image
pub fn calendar_icon() -> Image<'static> {
    Image::new(include_image!("icons/calendar.svg"))
}

/// Returns a document icon image
pub fn document_icon() -> Image<'static> {
    Image::new(include_image!("icons/document.svg"))
}

/// Returns a chart bar icon image
pub fn chart_bar_icon() -> Image<'static> {
    Image::new(include_image!("icons/chart-bar.svg"))
}

/// Returns a horizontal ellipsis icon image for compact action menus.
pub fn ellipsis_horizontal_icon() -> Image<'static> {
    Image::new(include_image!("icons/ellipsis-horizontal.svg"))
}

/// Returns a left arrow icon for backwards navigation.
pub fn arrow_left_icon() -> Image<'static> {
    Image::new(include_image!("icons/arrow-left.svg"))
}

/// Returns a right arrow icon for forwards navigation.
pub fn arrow_right_icon() -> Image<'static> {
    Image::new(include_image!("icons/arrow-right.svg"))
}

/// Returns an X-mark icon for dismissing overlays and panels.
pub fn x_mark_icon() -> Image<'static> {
    Image::new(include_image!("icons/x-mark.svg"))
}

/// Returns a trash icon image for destructive menu actions.
pub fn trash_icon() -> Image<'static> {
    Image::new(include_image!("icons/trash.svg"))
}

/// Render a trash (delete) icon (non-interactive)
///
/// For a clickable button version, use `trash_button()` instead.
///
/// # Arguments
/// * `ui` - The UI to render into
/// * `size` - Icon size in pixels
/// * `color` - Icon color (applied as tint)
pub fn trash(ui: &mut Ui, size: f32, color: Color32) -> Response {
    let image = Image::new(include_image!("icons/trash.svg"))
        .fit_to_exact_size(Vec2::splat(size))
        .tint(color);
    ui.add(image)
}

/// Render a pencil (edit) icon (non-interactive)
///
/// For a clickable button version, use `pencil_button()` instead.
///
/// # Arguments
/// * `ui` - The UI to render into
/// * `size` - Icon size in pixels
/// * `color` - Icon color (applied as tint)
pub fn pencil(ui: &mut Ui, size: f32, color: Color32) -> Response {
    let image = Image::new(include_image!("icons/pencil.svg"))
        .fit_to_exact_size(Vec2::splat(size))
        .tint(color);
    ui.add(image)
}

/// Render a clickable trash (delete) icon button using native egui Button
///
/// Uses Button::image_and_text() with zero-width space for proper accessibility.
/// The label appears in screen readers and kittest but not visually.
///
/// # Arguments
/// * `ui` - The UI to render into
/// * `size` - Icon size in pixels
/// * `color` - Icon color (applied as tint)
/// * `label` - Accessibility label for the button (e.g., "Delete my-resource")
pub fn trash_button(ui: &mut Ui, size: f32, color: Color32, label: &str) -> Response {
    let image = Image::new(include_image!("icons/trash.svg"))
        .fit_to_exact_size(Vec2::splat(size))
        .tint(color);
    let response = ui.add(egui::Button::image(image).frame(false));
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    response
}

/// Render a clickable pencil (edit) icon button using native egui Button
///
/// Uses Button::image_and_text() with zero-width space for proper accessibility.
/// The label appears in screen readers and kittest but not visually.
///
/// # Arguments
/// * `ui` - The UI to render into
/// * `size` - Icon size in pixels
/// * `color` - Icon color (applied as tint)
/// * `label` - Accessibility label for the button (e.g., "Edit my-resource")
pub fn pencil_button(ui: &mut Ui, size: f32, color: Color32, label: &str) -> Response {
    let image = Image::new(include_image!("icons/pencil.svg"))
        .fit_to_exact_size(Vec2::splat(size))
        .tint(color);
    let response = ui.add(egui::Button::image(image).frame(false));
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    response
}

/// Render a clickable eye icon button using native egui Button.
///
/// The label is exposed to assistive technologies while the button remains icon-only.
pub fn eye_button(ui: &mut Ui, size: f32, color: Color32, label: &str) -> Response {
    let image = Image::new(include_image!("icons/eye.svg"))
        .fit_to_exact_size(Vec2::splat(size))
        .tint(color);
    let response = ui.add(egui::Button::image(image).frame(false));
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    response
}
