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

use crate::PointingHand;

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

/// Returns a chevron-right icon for navigation affordances.
pub fn chevron_right_icon() -> Image<'static> {
    Image::new(include_image!("icons/chevron-right.svg"))
}

/// Returns an up arrow icon for upward navigation.
pub fn arrow_up_icon() -> Image<'static> {
    Image::new(include_image!("icons/arrow-up.svg"))
}

/// Returns a down arrow icon for downward navigation.
pub fn arrow_down_icon() -> Image<'static> {
    Image::new(include_image!("icons/arrow-down.svg"))
}

/// Returns a circular arrow icon for reload actions.
pub fn arrow_path_icon() -> Image<'static> {
    Image::new(include_image!("icons/arrow-path.svg"))
}

/// Returns the settings-home application destination tile.
pub fn settings_destination_application_icon() -> Image<'static> {
    Image::new(include_image!("icons/settings-destination-application.svg"))
}

/// Returns the settings-home cluster-discovery destination tile.
pub fn settings_destination_discovery_icon() -> Image<'static> {
    Image::new(include_image!("icons/settings-destination-discovery.svg"))
}

/// Returns a funnel icon for filtering controls.
pub fn funnel_icon() -> Image<'static> {
    Image::new(include_image!("icons/funnel.svg"))
}

/// Returns a numbered-list icon for line-number visibility controls.
pub fn numbered_list_icon() -> Image<'static> {
    Image::new(include_image!("icons/numbered-list.svg"))
}

/// Returns a calendar icon for timestamp visibility controls.
pub fn calendar_days_icon() -> Image<'static> {
    Image::new(include_image!("icons/calendar-days.svg"))
}

/// Returns a swatch icon for ANSI styling controls.
pub fn swatch_icon() -> Image<'static> {
    Image::new(include_image!("icons/swatch.svg"))
}

/// Returns an X-mark icon for dismissing overlays and panels.
pub fn x_mark_icon() -> Image<'static> {
    Image::new(include_image!("icons/x-mark.svg"))
}

/// Returns a trash icon image for destructive menu actions.
pub fn trash_icon() -> Image<'static> {
    Image::new(include_image!("icons/trash.svg"))
}

/// Returns a plus icon for add controls.
pub fn plus_icon() -> Image<'static> {
    Image::new(include_image!("icons/plus.svg"))
}

/// Returns a cog icon for application settings.
pub fn settings_icon() -> Image<'static> {
    Image::new(include_image!("icons/settings.svg"))
}

/// Returns the bundled Microsoft Azure brand mark for Azure provider surfaces.
pub fn azure_icon() -> Image<'static> {
    Image::new(include_image!("icons/azure.svg"))
}

/// Returns the bundled Tailscale brand mark for Tailscale provider surfaces.
pub fn tailscale_icon() -> Image<'static> {
    Image::new(include_image!("icons/tailscale.svg"))
}

/// Returns the Heroicons document-duplicate image used for copy actions.
pub fn document_duplicate_icon() -> Image<'static> {
    Image::new(include_image!("icons/document-duplicate.svg"))
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
    icon_button(
        ui,
        Image::new(include_image!("icons/trash.svg")),
        size,
        color,
        label,
    )
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
    icon_button(
        ui,
        Image::new(include_image!("icons/pencil.svg")),
        size,
        color,
        label,
    )
}

/// Render a clickable eye icon button using native egui Button.
///
/// The label is exposed to assistive technologies while the button remains icon-only.
pub fn eye_button(ui: &mut Ui, size: f32, color: Color32, label: &str) -> Response {
    icon_button(
        ui,
        Image::new(include_image!("icons/eye.svg")),
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
    decorate_icon_button(ui, response, label)
}

fn decorate_icon_button(ui: &Ui, response: Response, label: &str) -> Response {
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    response
}
