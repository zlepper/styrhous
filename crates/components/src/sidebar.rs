//! Tailwind-styled sidebar components for egui
//!
//! Provides two sidebar variants:
//! - [`WideSidebar`] - Full-featured navigation with icons, text, expandable sections
//! - [`NarrowSidebar`] - Compact icon-only navigation
//!
//! # Example: Wide Sidebar
//! ```ignore
//! use egui::{include_image, Image};
//!
//! let home_icon = Image::new(include_image!("icons/home.svg"));
//! let folder_icon = Image::new(include_image!("icons/folder.svg"));
//!
//! WideSidebar::new().show(ui, |sidebar| {
//!     sidebar.item("Dashboard", home_icon, true);
//!     sidebar.section_header("Your teams");
//!     sidebar.expandable("Projects", folder_icon, false, |sidebar| {
//!         sidebar.child_item("Alpha", false);
//!         sidebar.child_item("Beta", true);
//!     });
//! });
//! ```
//!
//! # Example: Narrow Sidebar
//! ```ignore
//! NarrowSidebar::new().show(ui, |sidebar| {
//!     sidebar.item("Dashboard", home_icon, true);
//!     sidebar.item("Settings", settings_icon, false);
//! });
//! ```

use egui::{
    Color32, Id, Image, Response, RichText, Sense, Ui, UiBuilder, Vec2, WidgetText,
    collapsing_header::CollapsingState,
};

use crate::PointingHand;
use crate::design::theme::{self, Theme, ThemeExt, ThemeScope};
use crate::design::{radius, space, stroke, type_scale};

const WIDE_WIDTH: f32 = 292.0;
const NARROW_WIDTH: f32 = 68.0;
// Resource leaves use a slightly denser allocation than their 44px selection
// treatment. This preserves the oracle's vertical rhythm while leaving a clear,
// comfortably sized active target.
const WIDE_ITEM_HEIGHT: f32 = 36.0;
const WIDE_GROUP_HEIGHT: f32 = 44.0;
const NARROW_ITEM_HEIGHT: f32 = 52.0;
const NARROW_AVATAR_SIZE: f32 = 28.0;
const ITEM_PADDING_X: f32 = space::S8;
const ICON_SIZE: f32 = 16.0;
const ICON_TEXT_SPACING: f32 = space::S8;
const CHEVRON_SIZE: f32 = 16.0;
const CHEVRON_GAP: f32 = 8.0;
/// Distance from an expandable item's padded edge to its disclosure chevron.
const CHEVRON_INSET: f32 = 12.0;

struct ItemColors {
    background: Color32,
    text: Color32,
    icon: Color32,
}

impl ItemColors {
    const fn new(background: Color32, text: Color32, icon: Color32) -> Self {
        Self {
            background,
            text,
            icon,
        }
    }

    /// The selected treatment shared by every item kind (`.nv.is-on`).
    fn selected(theme: &Theme) -> Self {
        Self::new(
            theme.nav.item_selected,
            theme.text.primary,
            theme.nav.accent,
        )
    }

    fn navigation(theme: &Theme, selected: bool, hovered: bool) -> Self {
        match (selected, hovered) {
            (true, _) => Self::selected(theme),
            (_, true) => Self::new(theme.nav.item_hover, theme.nav.text, theme.nav.text),
            _ => Self::new(Color32::TRANSPARENT, theme.nav.text, theme.nav.text_muted),
        }
    }

    fn child(theme: &Theme, selected: bool, hovered: bool) -> Self {
        match (selected, hovered) {
            (true, _) => Self::selected(theme),
            (_, true) => Self::new(theme.nav.item_hover, theme.nav.text, theme.nav.text_muted),
            _ => Self::new(
                Color32::TRANSPARENT,
                theme.nav.text_secondary,
                theme.nav.text_muted,
            ),
        }
    }

    fn expandable(theme: &Theme, hovered: bool, pressed: bool) -> Self {
        match (pressed, hovered) {
            (true, _) => Self::new(theme.nav.item_active, theme.nav.text, theme.nav.text_muted),
            (_, true) => Self::new(theme.nav.item_hover, theme.nav.text, theme.nav.text_muted),
            _ => Self::new(
                Color32::TRANSPARENT,
                theme.nav.text_secondary,
                theme.nav.text_muted,
            ),
        }
    }
}

/// Secondary text such as section headers and loading notes.
fn muted_text_color(ui: &Ui) -> Color32 {
    ui.tokens().nav.text_muted
}

/// The tint of disclosure chevrons.
fn chevron_tint(ui: &Ui) -> Color32 {
    ui.tokens().nav.text_muted
}

fn allocate_item(ui: &mut Ui, height: f32) -> (egui::Rect, egui::Rect, Response) {
    let available_width = ui.available_width();
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(available_width, height), Sense::click());
    let response = response.with_pointing_hand();
    let inner_rect = rect.shrink2(Vec2::new(ITEM_PADDING_X, 0.0));
    (rect, inner_rect, response)
}

fn draw_background(painter: &egui::Painter, rect: egui::Rect, color: Color32) {
    if color != Color32::TRANSPARENT {
        painter.rect_filled(rect, egui::CornerRadius::same(radius::KEY), color);
    }
}

fn render_icon(ui: &mut Ui, rect: egui::Rect, icon: Image<'_>, tint: Color32, size: f32) {
    let mut icon_ui = ui.new_child(UiBuilder::new().max_rect(rect).layout(
        egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
    ));
    icon_ui.add(icon.fit_to_exact_size(Vec2::splat(size)).tint(tint));
}

fn truncate_text(painter: &egui::Painter, text: &str, color: Color32, max_width: f32) -> String {
    let font = type_scale::BODY.font_id(painter.ctx());
    if painter
        .layout_no_wrap(text.to_owned(), font.clone(), color)
        .size()
        .x
        <= max_width
    {
        return text.to_owned();
    }

    let ellipsis = "…";
    let mut truncated = String::new();
    for character in text.chars() {
        let candidate = format!("{truncated}{character}{ellipsis}");
        if painter
            .layout_no_wrap(candidate, font.clone(), color)
            .size()
            .x
            > max_width
        {
            break;
        }
        truncated.push(character);
    }
    format!("{truncated}{ellipsis}")
}

fn render_text(
    painter: &egui::Painter,
    pos: egui::Pos2,
    text: &str,
    color: Color32,
    max_width: f32,
) -> bool {
    let rendered_text = truncate_text(painter, text, color, max_width.max(0.0));
    painter.text(
        pos,
        egui::Align2::LEFT_TOP,
        &rendered_text,
        type_scale::BODY.font_id(painter.ctx()),
        color,
    );
    rendered_text != text
}

fn add_selectable_accessibility(response: &Response, ui: &Ui, label: &str, selected: bool) {
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            selected,
            label,
        )
    });
}

fn add_expandable_accessibility(response: &Response, ui: &Ui, label: &str, open: bool) {
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::CollapsingHeader,
            ui.is_enabled(),
            open,
            label,
        )
    });
}

fn icon_rect_wide(inner_rect: egui::Rect, item_height: f32, size: f32) -> egui::Rect {
    let icon_x = CHEVRON_SIZE + CHEVRON_GAP;
    egui::Rect::from_min_size(
        inner_rect.min + Vec2::new(icon_x, (item_height - size) / 2.0),
        Vec2::splat(size),
    )
}

fn icon_rect_centered(inner_rect: egui::Rect, size: f32) -> egui::Rect {
    egui::Rect::from_center_size(inner_rect.center(), Vec2::splat(size))
}

fn text_pos_after_icon(inner_rect: egui::Rect, item_height: f32) -> egui::Pos2 {
    let text_x = CHEVRON_SIZE + CHEVRON_GAP + ICON_SIZE + ICON_TEXT_SPACING;
    inner_rect.min + Vec2::new(text_x, (item_height - type_scale::BODY.size()) / 2.0)
}

fn text_pos_after_chevron(inner_rect: egui::Rect, item_height: f32) -> egui::Pos2 {
    let text_x = CHEVRON_SIZE + CHEVRON_GAP + 24.0;
    inner_rect.min + Vec2::new(text_x, (item_height - type_scale::BODY.size()) / 2.0)
}

mod core;
mod narrow;
mod wide;

pub use narrow::{NarrowSidebar, NarrowSidebarContent};
pub use wide::{ExpandableResponse, WideSidebar, WideSidebarContent};

#[cfg(test)]
mod tests;
