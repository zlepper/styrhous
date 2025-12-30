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

use egui::{include_image, Color32, Image, Response, Ui, Vec2};

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
