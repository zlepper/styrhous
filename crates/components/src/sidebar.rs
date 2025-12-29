//! Tailwind-styled sidebar component for egui
//!
//! Provides a navigation sidebar with support for:
//! - Wide mode (icons + text) and Narrow mode (icons only)
//! - Expandable/collapsible sections
//! - Selection state highlighting
//!
//! # Example
//! ```ignore
//! Sidebar::new()
//!     .mode(SidebarMode::Wide)
//!     .show(ui, |sidebar| {
//!         sidebar.item("Dashboard", |ui| { ui.label("🏠"); }, true);
//!         sidebar.item("Team", |ui| { ui.label("👥"); }, false);
//!         sidebar.section_header("Your teams");
//!         sidebar.expandable("Projects", |ui| { ui.label("📁"); }, false, |sidebar| {
//!             sidebar.child_item("Alpha", false);
//!             sidebar.child_item("Beta", true);
//!         });
//!     });
//! ```

use egui::{
    collapsing_header::CollapsingState, Color32, CornerRadius, Response, RichText, Sense, Stroke,
    Ui, UiBuilder, Vec2,
};

use crate::colors::{gray, indigo, WHITE};

/// Sidebar display mode
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SidebarMode {
    /// Wide mode: shows icons and text labels
    #[default]
    Wide,
    /// Narrow mode: shows only icons, dark background
    Narrow,
}

/// Default width for wide mode
const WIDE_WIDTH: f32 = 256.0;
/// Default width for narrow mode
const NARROW_WIDTH: f32 = 72.0;
/// Item height in wide mode
const WIDE_ITEM_HEIGHT: f32 = 40.0;
/// Item height in narrow mode
const NARROW_ITEM_HEIGHT: f32 = 48.0;
/// Horizontal padding for items
const ITEM_PADDING_X: f32 = 12.0;
/// Corner radius for item backgrounds
const ITEM_CORNER_RADIUS: u8 = 6;
/// Icon size
const ICON_SIZE: f32 = 24.0;
/// Spacing between icon and text
const ICON_TEXT_SPACING: f32 = 12.0;
/// Chevron icon size (Tailwind size-5 = 20px)
const CHEVRON_SIZE: f32 = 20.0;
/// Gap between chevron and next element (Tailwind gap-x-3 = 12px)
const CHEVRON_GAP: f32 = 12.0;

/// A Tailwind-styled sidebar builder for egui
pub struct Sidebar {
    mode: SidebarMode,
    width: Option<f32>,
}

impl Default for Sidebar {
    fn default() -> Self {
        Self::new()
    }
}

impl Sidebar {
    /// Create a new sidebar with default settings
    pub fn new() -> Self {
        Self {
            mode: SidebarMode::Wide,
            width: None,
        }
    }

    /// Set the display mode (Wide or Narrow)
    pub fn mode(mut self, mode: SidebarMode) -> Self {
        self.mode = mode;
        self
    }

    /// Override the default width
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// Show the sidebar with the given content builder
    pub fn show<R>(self, ui: &mut Ui, add_contents: impl FnOnce(&mut SidebarContent<'_>) -> R) -> R {
        let width = self.width.unwrap_or(match self.mode {
            SidebarMode::Wide => WIDE_WIDTH,
            SidebarMode::Narrow => NARROW_WIDTH,
        });

        let background = match self.mode {
            SidebarMode::Wide => WHITE,
            SidebarMode::Narrow => gray::_900,
        };

        // Draw background
        let rect = ui.available_rect_before_wrap();
        let sidebar_rect = egui::Rect::from_min_size(rect.min, Vec2::new(width, rect.height()));
        ui.painter().rect_filled(sidebar_rect, 0.0, background);

        // Create a child UI with constrained width
        let mut child_ui = ui.new_child(
            UiBuilder::new()
                .max_rect(sidebar_rect)
                .layout(egui::Layout::top_down(egui::Align::LEFT)),
        );

        // Add padding
        child_ui.add_space(8.0);

        let mut content = SidebarContent {
            ui: &mut child_ui,
            mode: self.mode,
        };

        add_contents(&mut content)
    }
}

/// Context for building sidebar content
///
/// This is passed to the closure in `Sidebar::show()` and provides
/// methods for adding navigation items, sections, and expandable groups.
pub struct SidebarContent<'a> {
    ui: &'a mut Ui,
    mode: SidebarMode,
}

impl<'a> SidebarContent<'a> {
    /// Get the current sidebar mode
    pub fn mode(&self) -> SidebarMode {
        self.mode
    }

    /// Get mutable access to the underlying Ui
    pub fn ui_mut(&mut self) -> &mut Ui {
        self.ui
    }

    /// Add a navigation item with icon and text
    ///
    /// In narrow mode, only the icon is shown.
    ///
    /// # Arguments
    /// * `text` - The label text (shown in wide mode only)
    /// * `icon` - Closure to render the icon
    /// * `selected` - Whether this item is currently selected
    ///
    /// # Returns
    /// Response indicating if the item was clicked
    pub fn item(
        &mut self,
        text: &str,
        icon: impl FnOnce(&mut Ui),
        selected: bool,
    ) -> Response {
        let item_height = match self.mode {
            SidebarMode::Wide => WIDE_ITEM_HEIGHT,
            SidebarMode::Narrow => NARROW_ITEM_HEIGHT,
        };

        let (bg_color, text_color, icon_color) = self.item_colors(selected, false);

        let available_width = self.ui.available_width();

        // Allocate space and get response
        let (rect, response) = self.ui.allocate_exact_size(
            Vec2::new(available_width, item_height),
            Sense::click(),
        );

        // Calculate inner rect with padding
        let inner_rect = rect.shrink2(Vec2::new(ITEM_PADDING_X, 0.0));

        // Determine colors based on interaction state
        let (bg, text_col, icon_col) = if response.hovered() && !selected {
            self.item_colors(selected, true)
        } else {
            (bg_color, text_color, icon_color)
        };

        // Draw background if selected or hovered
        if selected || response.hovered() {
            self.ui.painter().rect_filled(
                inner_rect,
                CornerRadius::same(ITEM_CORNER_RADIUS),
                bg,
            );
        }

        match self.mode {
            SidebarMode::Wide => {
                // Icon position - aligned with expandable items (after chevron space)
                let icon_x = CHEVRON_SIZE + CHEVRON_GAP;
                let icon_rect = egui::Rect::from_min_size(
                    inner_rect.min + Vec2::new(icon_x, (item_height - ICON_SIZE) / 2.0),
                    Vec2::splat(ICON_SIZE),
                );

                let mut icon_ui = self.ui.new_child(
                    UiBuilder::new()
                        .max_rect(icon_rect)
                        .layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight)),
                );
                icon_ui.visuals_mut().override_text_color = Some(icon_col);
                icon(&mut icon_ui);

                // Text label - after icon with gap
                let text_x = icon_x + ICON_SIZE + ICON_TEXT_SPACING;
                let text_pos = inner_rect.min + Vec2::new(text_x, (item_height - 14.0) / 2.0);
                self.ui.painter().text(
                    text_pos,
                    egui::Align2::LEFT_TOP,
                    text,
                    egui::FontId::proportional(14.0),
                    text_col,
                );
            }
            SidebarMode::Narrow => {
                // Center icon only
                let icon_rect = egui::Rect::from_center_size(
                    inner_rect.center(),
                    Vec2::splat(ICON_SIZE),
                );

                let mut icon_ui = self.ui.new_child(
                    UiBuilder::new()
                        .max_rect(icon_rect)
                        .layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight)),
                );
                icon_ui.visuals_mut().override_text_color = Some(icon_col);
                icon(&mut icon_ui);
            }
        }

        response
    }

    /// Add a section header label (only visible in wide mode)
    ///
    /// Section headers are uppercase, smaller text used to group navigation items.
    pub fn section_header(&mut self, text: &str) {
        if self.mode == SidebarMode::Narrow {
            return; // Hidden in narrow mode
        }

        self.ui.add_space(16.0);

        let padding = Vec2::new(ITEM_PADDING_X + 8.0, 4.0);
        self.ui.horizontal(|ui| {
            ui.add_space(padding.x);
            ui.label(
                RichText::new(text.to_uppercase())
                    .size(11.0)
                    .color(gray::_500),
            );
        });

        self.ui.add_space(4.0);
    }

    /// Add an expandable section with child items
    ///
    /// In narrow mode, expandable sections are hidden entirely.
    ///
    /// # Arguments
    /// * `text` - The section header text
    /// * `icon` - Closure to render the icon
    /// * `default_open` - Whether the section starts expanded
    /// * `add_children` - Closure to add child items
    pub fn expandable<R>(
        &mut self,
        text: &str,
        icon: impl FnOnce(&mut Ui),
        default_open: bool,
        add_children: impl FnOnce(&mut SidebarContent<'_>) -> R,
    ) -> Option<R> {
        if self.mode == SidebarMode::Narrow {
            return None; // Hidden in narrow mode
        }

        let id = self.ui.make_persistent_id(text);
        let mut state = CollapsingState::load_with_default_open(self.ui.ctx(), id, default_open);
        let is_open = state.is_open();

        // Draw the header
        let item_height = WIDE_ITEM_HEIGHT;
        let available_width = self.ui.available_width();

        let (rect, response) = self.ui.allocate_exact_size(
            Vec2::new(available_width, item_height),
            Sense::click(),
        );

        let inner_rect = rect.shrink2(Vec2::new(ITEM_PADDING_X, 0.0));

        // Hover background
        if response.hovered() {
            self.ui.painter().rect_filled(
                inner_rect,
                CornerRadius::same(ITEM_CORNER_RADIUS),
                gray::_50,
            );
        }

        // Chevron - positioned at start of inner rect
        let chevron_rect = egui::Rect::from_min_size(
            inner_rect.min + Vec2::new(0.0, (item_height - CHEVRON_SIZE) / 2.0),
            Vec2::splat(CHEVRON_SIZE),
        );
        let mut chevron_ui = self.ui.new_child(
            UiBuilder::new()
                .max_rect(chevron_rect)
                .layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight)),
        );
        if is_open {
            crate::icons::chevron_down(&mut chevron_ui, CHEVRON_SIZE, gray::_400);
        } else {
            crate::icons::chevron_right(&mut chevron_ui, CHEVRON_SIZE, gray::_400);
        }

        // Icon - positioned after chevron with gap
        let icon_x = CHEVRON_SIZE + CHEVRON_GAP;
        let icon_rect = egui::Rect::from_min_size(
            inner_rect.min + Vec2::new(icon_x, (item_height - ICON_SIZE) / 2.0),
            Vec2::splat(ICON_SIZE),
        );
        let mut icon_ui = self.ui.new_child(
            UiBuilder::new()
                .max_rect(icon_rect)
                .layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight)),
        );
        icon_ui.visuals_mut().override_text_color = Some(gray::_700);
        icon(&mut icon_ui);

        // Text - positioned after icon with gap
        let text_x = icon_x + ICON_SIZE + ICON_TEXT_SPACING;
        let text_pos = inner_rect.min + Vec2::new(text_x, (item_height - 14.0) / 2.0);
        self.ui.painter().text(
            text_pos,
            egui::Align2::LEFT_TOP,
            text,
            egui::FontId::proportional(14.0),
            gray::_700,
        );

        // Handle click to toggle
        if response.clicked() {
            state.toggle(self.ui);
        }

        // Draw children if open
        if is_open {
            // Indent children
            self.ui.add_space(2.0);
            let result = add_children(self);
            self.ui.add_space(2.0);
            Some(result)
        } else {
            None
        }
    }

    /// Add a child item (indented, no icon)
    ///
    /// Used inside `expandable()` sections.
    pub fn child_item(&mut self, text: &str, selected: bool) -> Response {
        let item_height = WIDE_ITEM_HEIGHT;
        let available_width = self.ui.available_width();

        let (rect, response) = self.ui.allocate_exact_size(
            Vec2::new(available_width, item_height),
            Sense::click(),
        );

        let inner_rect = rect.shrink2(Vec2::new(ITEM_PADDING_X, 0.0));

        // Text position aligned with parent item text
        let text_x = CHEVRON_SIZE + CHEVRON_GAP + ICON_SIZE + ICON_TEXT_SPACING;

        // Background rect starts slightly before text
        let bg_indent = text_x - 8.0;
        let indented_rect = egui::Rect::from_min_max(
            inner_rect.min + Vec2::new(bg_indent, 0.0),
            inner_rect.max,
        );

        // Colors
        let (bg, text_color) = if selected {
            (gray::_100, gray::_900)
        } else if response.hovered() {
            (gray::_50, gray::_700)
        } else {
            (Color32::TRANSPARENT, gray::_600)
        };

        // Background
        if selected || response.hovered() {
            self.ui.painter().rect_filled(
                indented_rect,
                CornerRadius::same(ITEM_CORNER_RADIUS),
                bg,
            );
        }

        // Text - aligned with parent item text
        let text_pos = inner_rect.min + Vec2::new(text_x, (item_height - 14.0) / 2.0);
        self.ui.painter().text(
            text_pos,
            egui::Align2::LEFT_TOP,
            text,
            egui::FontId::proportional(14.0),
            text_color,
        );

        response
    }

    /// Render a circular avatar with an initial letter
    ///
    /// Returns a closure suitable for use as an icon parameter.
    pub fn avatar(initial: &str) -> impl FnOnce(&mut Ui) + '_ {
        move |ui: &mut Ui| {
            let size = Vec2::splat(ICON_SIZE);
            let (rect, _) = ui.allocate_exact_size(size, Sense::hover());

            // Draw circular background
            ui.painter().circle_filled(rect.center(), ICON_SIZE / 2.0, gray::_200);

            // Draw initial
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                initial,
                egui::FontId::proportional(12.0),
                gray::_600,
            );
        }
    }

    /// Add a visual separator line
    pub fn separator(&mut self) {
        self.ui.add_space(8.0);
        let available_width = self.ui.available_width();
        let rect = self.ui.allocate_exact_size(Vec2::new(available_width, 1.0), Sense::hover()).0;
        let line_rect = rect.shrink2(Vec2::new(ITEM_PADDING_X, 0.0));
        self.ui.painter().line_segment(
            [line_rect.left_center(), line_rect.right_center()],
            Stroke::new(1.0, gray::_200),
        );
        self.ui.add_space(8.0);
    }

    /// Get colors for an item based on selection and hover state
    fn item_colors(&self, selected: bool, hovered: bool) -> (Color32, Color32, Color32) {
        match self.mode {
            SidebarMode::Wide => {
                if selected {
                    (indigo::_50, indigo::_600, indigo::_600)
                } else if hovered {
                    (gray::_50, gray::_700, gray::_500)
                } else {
                    (Color32::TRANSPARENT, gray::_700, gray::_500)
                }
            }
            SidebarMode::Narrow => {
                if selected {
                    (gray::_800, WHITE, WHITE)
                } else if hovered {
                    (gray::_800, gray::_400, gray::_400)
                } else {
                    (Color32::TRANSPARENT, gray::_400, gray::_400)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;

    /// Create a test harness with image loaders installed
    fn create_harness<'a>(app: impl FnMut(&mut Ui) + 'a) -> Harness<'a> {
        let mut harness = Harness::new_ui(app);
        egui_extras::install_image_loaders(&harness.ctx);
        harness.run();
        harness
    }

    fn home_icon(ui: &mut Ui) {
        ui.label("🏠");
    }

    fn team_icon(ui: &mut Ui) {
        ui.label("👥");
    }

    fn folder_icon(ui: &mut Ui) {
        ui.label("📁");
    }

    fn calendar_icon(ui: &mut Ui) {
        ui.label("📅");
    }

    fn document_icon(ui: &mut Ui) {
        ui.label("📄");
    }

    fn chart_icon(ui: &mut Ui) {
        ui.label("📊");
    }

    #[test]
    fn test_sidebar_wide_mode() {
        let mut harness = create_harness(|ui| {
            Sidebar::new()
                .mode(SidebarMode::Wide)
                .show(ui, |sidebar| {
                    sidebar.item("Dashboard", home_icon, true);
                    sidebar.item("Team", team_icon, false);
                    sidebar.item("Projects", folder_icon, false);
                    sidebar.item("Calendar", calendar_icon, false);
                    sidebar.item("Documents", document_icon, false);
                    sidebar.item("Reports", chart_icon, false);

                    sidebar.section_header("Your teams");

                    sidebar.item("Heroicons", |ui| { SidebarContent::avatar("H")(ui); }, false);
                    sidebar.item("Tailwind Labs", |ui| { SidebarContent::avatar("T")(ui); }, false);
                    sidebar.item("Workcation", |ui| { SidebarContent::avatar("W")(ui); }, false);
                });
        });

        harness.snapshot("sidebar_wide");
    }

    #[test]
    fn test_sidebar_narrow_mode() {
        let mut harness = create_harness(|ui| {
            Sidebar::new()
                .mode(SidebarMode::Narrow)
                .show(ui, |sidebar| {
                    sidebar.item("Dashboard", home_icon, true);
                    sidebar.item("Team", team_icon, false);
                    sidebar.item("Projects", folder_icon, false);
                    sidebar.item("Calendar", calendar_icon, false);
                    sidebar.item("Documents", document_icon, false);
                    sidebar.item("Reports", chart_icon, false);
                });
        });

        harness.snapshot("sidebar_narrow");
    }

    #[test]
    fn test_sidebar_expandable_sections() {
        let mut harness = create_harness(|ui| {
            Sidebar::new()
                .mode(SidebarMode::Wide)
                .show(ui, |sidebar| {
                    sidebar.item("Dashboard", home_icon, true);

                    sidebar.expandable("Teams", team_icon, true, |sidebar| {
                        sidebar.child_item("Engineering", false);
                        sidebar.child_item("Human Resources", false);
                        sidebar.child_item("Customer Success", false);
                    });

                    sidebar.expandable("Projects", folder_icon, false, |sidebar| {
                        sidebar.child_item("Alpha", false);
                        sidebar.child_item("Beta", false);
                    });

                    sidebar.item("Calendar", calendar_icon, false);
                    sidebar.item("Documents", document_icon, false);
                    sidebar.item("Reports", chart_icon, false);
                });
        });

        harness.snapshot("sidebar_expandable");
    }
}
