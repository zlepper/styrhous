//! Tailwind-styled sidebar component for egui
//!
//! Provides a navigation sidebar with support for:
//! - Wide mode (icons + text) and Narrow mode (icons only)
//! - Expandable/collapsible sections
//! - Selection state highlighting
//! - Accessible buttons for screen readers
//!
//! # Example
//! ```ignore
//! use egui::{include_image, Image};
//!
//! let home_icon = Image::new(include_image!("icons/home.svg"));
//! let folder_icon = Image::new(include_image!("icons/folder.svg"));
//!
//! Sidebar::new()
//!     .mode(SidebarMode::Wide)
//!     .show(ui, |sidebar| {
//!         sidebar.item("Dashboard", home_icon, true);
//!         sidebar.item("Team", home_icon.clone(), false);
//!         sidebar.section_header("Your teams");
//!         sidebar.expandable("Projects", folder_icon, false, |sidebar| {
//!             sidebar.child_item("Alpha", false);
//!             sidebar.child_item("Beta", true);
//!         });
//!     });
//! ```
//!
//! ## Handling Selection
//!
//! Items return a `Response` for detecting clicks:
//! ```ignore
//! if sidebar.item("Dashboard", home_icon, selected == "Dashboard").clicked() {
//!     selected = "Dashboard";
//! }
//! ```

use egui::{
    collapsing_header::CollapsingState, Color32, CornerRadius, Image, Response, RichText, Sense,
    Stroke, Ui, UiBuilder, Vec2, WidgetText,
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

/// Response from an expandable sidebar section
pub struct ExpandableResponse<R> {
    /// Response from clicking the header
    pub header: Response,
    /// Result from the children closure, or None if collapsed
    pub children: Option<R>,
    /// Whether the section is currently expanded
    pub is_open: bool,
}

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

        // Both modes use light theme
        let background = WHITE;

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
    /// * `icon` - Icon image to display
    /// * `selected` - Whether this item is currently selected
    ///
    /// # Returns
    /// Response indicating if the item was clicked
    pub fn item(&mut self, text: impl Into<WidgetText>, icon: Image<'_>, selected: bool) -> Response {
        let text = text.into();
        let item_height = match self.mode {
            SidebarMode::Wide => WIDE_ITEM_HEIGHT,
            SidebarMode::Narrow => NARROW_ITEM_HEIGHT,
        };

        let available_width = self.ui.available_width();
        let sized_icon = icon.fit_to_exact_size(Vec2::splat(ICON_SIZE));

        // Allocate the full row as clickable
        let (rect, response) = self.ui.allocate_exact_size(
            Vec2::new(available_width, item_height),
            Sense::click(),
        );

        let inner_rect = rect.shrink2(Vec2::new(ITEM_PADDING_X, 0.0));

        // Determine colors based on state (same colors for both modes - light theme)
        let (bg_color, text_color, icon_color) = if selected {
            (indigo::_50, indigo::_600, indigo::_600)
        } else if response.hovered() {
            (gray::_50, gray::_700, gray::_500)
        } else {
            (Color32::TRANSPARENT, gray::_700, gray::_500)
        };

        // Draw background
        if bg_color != Color32::TRANSPARENT {
            self.ui.painter().rect_filled(
                inner_rect,
                CornerRadius::same(ITEM_CORNER_RADIUS),
                bg_color,
            );
        }

        match self.mode {
            SidebarMode::Wide => {
                // Reserve chevron space for alignment with expandable items
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
                icon_ui.add(sized_icon.tint(icon_color));

                // Text
                let text_x = icon_x + ICON_SIZE + ICON_TEXT_SPACING;
                let text_pos = inner_rect.min + Vec2::new(text_x, (item_height - 14.0) / 2.0);
                self.ui.painter().text(
                    text_pos,
                    egui::Align2::LEFT_TOP,
                    text.text(),
                    egui::FontId::proportional(14.0),
                    text_color,
                );
            }
            SidebarMode::Narrow => {
                // Center icon only
                let icon_rect = egui::Rect::from_center_size(inner_rect.center(), Vec2::splat(ICON_SIZE));
                let mut icon_ui = self.ui.new_child(
                    UiBuilder::new()
                        .max_rect(icon_rect)
                        .layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight)),
                );
                icon_ui.add(sized_icon.tint(icon_color));
            }
        }

        // Add accessibility info
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, self.ui.is_enabled(), text.text()));

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
    /// In narrow mode, returns a dummy response (expandable sections are hidden).
    ///
    /// # Arguments
    /// * `text` - The section header text
    /// * `icon` - Icon image to display
    /// * `default_open` - Whether the section starts expanded
    /// * `add_children` - Closure to add child items
    pub fn expandable<R>(
        &mut self,
        text: impl Into<WidgetText>,
        icon: Image<'_>,
        default_open: bool,
        add_children: impl FnOnce(&mut SidebarContent<'_>) -> R,
    ) -> ExpandableResponse<R> {
        if self.mode == SidebarMode::Narrow {
            // Hidden in narrow mode - return dummy response
            let dummy = self.ui.allocate_response(Vec2::ZERO, Sense::click());
            return ExpandableResponse {
                header: dummy,
                children: None,
                is_open: false,
            };
        }

        let text = text.into();
        let id = self.ui.make_persistent_id(text.text());
        let mut state = CollapsingState::load_with_default_open(self.ui.ctx(), id, default_open);
        let is_open = state.is_open();

        let item_height = WIDE_ITEM_HEIGHT;
        let available_width = self.ui.available_width();

        // Get the appropriate chevron image
        let chevron = if is_open {
            Image::new(egui::include_image!("icons/chevron-down.svg"))
        } else {
            Image::new(egui::include_image!("icons/chevron-right.svg"))
        };
        let chevron = chevron.fit_to_exact_size(Vec2::splat(CHEVRON_SIZE)).tint(gray::_400);
        let sized_icon = icon.fit_to_exact_size(Vec2::splat(ICON_SIZE));

        // Create the entire row as a single clickable button with chevron + icon + text
        // Use a horizontal layout inside an allocate_ui_with_layout for proper sizing
        let (rect, response) = self.ui.allocate_exact_size(
            Vec2::new(available_width, item_height),
            Sense::click(),
        );

        // Draw hover/press background
        if response.hovered() || response.is_pointer_button_down_on() {
            let inner_rect = rect.shrink2(Vec2::new(ITEM_PADDING_X, 0.0));
            self.ui.painter().rect_filled(
                inner_rect,
                CornerRadius::same(ITEM_CORNER_RADIUS),
                if response.is_pointer_button_down_on() { gray::_100 } else { gray::_50 },
            );
        }

        // Draw content manually
        let inner_rect = rect.shrink2(Vec2::new(ITEM_PADDING_X, 0.0));

        // Chevron
        let chevron_rect = egui::Rect::from_min_size(
            inner_rect.min + Vec2::new(0.0, (item_height - CHEVRON_SIZE) / 2.0),
            Vec2::splat(CHEVRON_SIZE),
        );
        let mut chevron_ui = self.ui.new_child(
            UiBuilder::new()
                .max_rect(chevron_rect)
                .layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight)),
        );
        chevron_ui.add(chevron);

        // Icon (tinted for visibility)
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
        icon_ui.add(sized_icon.tint(gray::_500));

        // Text
        let text_x = icon_x + ICON_SIZE + ICON_TEXT_SPACING;
        let text_pos = inner_rect.min + Vec2::new(text_x, (item_height - 14.0) / 2.0);
        self.ui.painter().text(
            text_pos,
            egui::Align2::LEFT_TOP,
            text.text(),
            egui::FontId::proportional(14.0),
            gray::_700,
        );

        // Add accessibility info
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, self.ui.is_enabled(), text.text()));

        // Handle click to toggle
        if response.clicked() {
            state.toggle(self.ui);
        }

        // Store state to persist across frames (critical for egui immediate mode!)
        state.store(self.ui.ctx());

        // Re-check state after potential toggle for children rendering
        let is_open_now = state.is_open();

        // Draw children if open
        let children = if is_open_now {
            self.ui.add_space(2.0);
            let result = add_children(self);
            self.ui.add_space(2.0);
            Some(result)
        } else {
            None
        };

        ExpandableResponse {
            header: response,
            children,
            is_open: is_open_now,
        }
    }

    /// Add a child item (indented, no icon)
    ///
    /// Used inside `expandable()` sections.
    pub fn child_item(&mut self, text: impl Into<WidgetText>, selected: bool) -> Response {
        let text = text.into();
        let item_height = WIDE_ITEM_HEIGHT;
        let available_width = self.ui.available_width();

        // Allocate the full row as clickable
        let (rect, response) = self.ui.allocate_exact_size(
            Vec2::new(available_width, item_height),
            Sense::click(),
        );

        // Text position aligned with parent item text (after chevron + icon space)
        let text_x = ITEM_PADDING_X + CHEVRON_SIZE + CHEVRON_GAP + ICON_SIZE + ICON_TEXT_SPACING;

        // Background rect starts slightly before text for visual grouping
        let bg_indent = text_x - 8.0;
        let bg_rect = egui::Rect::from_min_max(
            rect.min + Vec2::new(bg_indent, 0.0),
            rect.max - Vec2::new(ITEM_PADDING_X, 0.0),
        );

        // Determine colors
        let (bg_color, text_color) = if selected {
            (gray::_100, gray::_900)
        } else if response.hovered() {
            (gray::_50, gray::_700)
        } else {
            (Color32::TRANSPARENT, gray::_600)
        };

        // Draw background
        if bg_color != Color32::TRANSPARENT {
            self.ui.painter().rect_filled(
                bg_rect,
                CornerRadius::same(ITEM_CORNER_RADIUS),
                bg_color,
            );
        }

        // Text
        let text_pos = rect.min + Vec2::new(text_x, (item_height - 14.0) / 2.0);
        self.ui.painter().text(
            text_pos,
            egui::Align2::LEFT_TOP,
            text.text(),
            egui::FontId::proportional(14.0),
            text_color,
        );

        // Add accessibility info
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, self.ui.is_enabled(), text.text()));

        response
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

    /// Add an avatar item with a circular initial badge
    ///
    /// This is a specialized item that shows a circular avatar with an initial letter
    /// instead of an icon image.
    pub fn avatar_item(&mut self, text: impl Into<WidgetText>, initial: &str, selected: bool) -> Response {
        let text = text.into();
        let item_height = match self.mode {
            SidebarMode::Wide => WIDE_ITEM_HEIGHT,
            SidebarMode::Narrow => NARROW_ITEM_HEIGHT,
        };

        let available_width = self.ui.available_width();

        // Allocate the full row as clickable
        let (rect, response) = self.ui.allocate_exact_size(
            Vec2::new(available_width, item_height),
            Sense::click(),
        );

        let inner_rect = rect.shrink2(Vec2::new(ITEM_PADDING_X, 0.0));

        // Determine colors based on state (same light theme for both modes)
        let (bg_color, text_color) = if selected {
            (indigo::_50, indigo::_600)
        } else if response.hovered() {
            (gray::_50, gray::_700)
        } else {
            (Color32::TRANSPARENT, gray::_700)
        };

        // Draw background
        if bg_color != Color32::TRANSPARENT {
            self.ui.painter().rect_filled(
                inner_rect,
                CornerRadius::same(ITEM_CORNER_RADIUS),
                bg_color,
            );
        }

        match self.mode {
            SidebarMode::Wide => {
                // Reserve chevron space for alignment
                let avatar_x = CHEVRON_SIZE + CHEVRON_GAP;
                let avatar_center = inner_rect.min + Vec2::new(avatar_x + ICON_SIZE / 2.0, item_height / 2.0);

                // Draw avatar circle
                self.ui.painter().circle_filled(avatar_center, ICON_SIZE / 2.0, gray::_200);
                self.ui.painter().text(
                    avatar_center,
                    egui::Align2::CENTER_CENTER,
                    initial,
                    egui::FontId::proportional(12.0),
                    gray::_600,
                );

                // Text
                let text_x = avatar_x + ICON_SIZE + ICON_TEXT_SPACING;
                let text_pos = inner_rect.min + Vec2::new(text_x, (item_height - 14.0) / 2.0);
                self.ui.painter().text(
                    text_pos,
                    egui::Align2::LEFT_TOP,
                    text.text(),
                    egui::FontId::proportional(14.0),
                    text_color,
                );
            }
            SidebarMode::Narrow => {
                // Center avatar only
                let avatar_center = inner_rect.center();
                self.ui.painter().circle_filled(avatar_center, ICON_SIZE / 2.0, gray::_200);
                self.ui.painter().text(
                    avatar_center,
                    egui::Align2::CENTER_CENTER,
                    initial,
                    egui::FontId::proportional(12.0),
                    gray::_600,
                );
            }
        }

        // Add accessibility info
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, self.ui.is_enabled(), text.text()));

        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::kittest::Queryable;
    use egui_kittest::Harness;

    /// Create a test harness with image loaders installed
    fn create_harness<'a>(app: impl FnMut(&mut Ui) + 'a) -> Harness<'a> {
        let mut harness = Harness::new_ui(app);
        egui_extras::install_image_loaders(&harness.ctx);
        harness.run();
        harness
    }

    // Icon helpers for testing
    fn home_icon() -> Image<'static> {
        Image::new(egui::include_image!("icons/home.svg"))
    }
    fn users_icon() -> Image<'static> {
        Image::new(egui::include_image!("icons/users.svg"))
    }
    fn folder_icon() -> Image<'static> {
        Image::new(egui::include_image!("icons/folder.svg"))
    }
    fn calendar_icon() -> Image<'static> {
        Image::new(egui::include_image!("icons/calendar.svg"))
    }
    fn document_icon() -> Image<'static> {
        Image::new(egui::include_image!("icons/document.svg"))
    }
    fn chart_icon() -> Image<'static> {
        Image::new(egui::include_image!("icons/chart-bar.svg"))
    }

    #[test]
    fn test_sidebar_wide_mode() {
        let mut harness = create_harness(|ui| {
            Sidebar::new()
                .mode(SidebarMode::Wide)
                .show(ui, |sidebar| {
                    sidebar.item("Dashboard", home_icon(), true);
                    sidebar.item("Team", users_icon(), false);
                    sidebar.item("Projects", folder_icon(), false);
                    sidebar.item("Calendar", calendar_icon(), false);
                    sidebar.item("Documents", document_icon(), false);
                    sidebar.item("Reports", chart_icon(), false);

                    sidebar.section_header("Your teams");

                    sidebar.avatar_item("Heroicons", "H", false);
                    sidebar.avatar_item("Tailwind Labs", "T", false);
                    sidebar.avatar_item("Workcation", "W", false);
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
                    sidebar.item("Dashboard", home_icon(), true);
                    sidebar.item("Team", users_icon(), false);
                    sidebar.item("Projects", folder_icon(), false);
                    sidebar.item("Calendar", calendar_icon(), false);
                    sidebar.item("Documents", document_icon(), false);
                    sidebar.item("Reports", chart_icon(), false);
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
                    sidebar.item("Dashboard", home_icon(), true);

                    sidebar.expandable("Teams", users_icon(), true, |sidebar| {
                        sidebar.child_item("Engineering", false);
                        sidebar.child_item("Human Resources", false);
                        sidebar.child_item("Customer Success", false);
                    });

                    sidebar.expandable("Projects", folder_icon(), false, |sidebar| {
                        sidebar.child_item("Alpha", false);
                        sidebar.child_item("Beta", false);
                    });

                    sidebar.item("Calendar", calendar_icon(), false);
                    sidebar.item("Documents", document_icon(), false);
                    sidebar.item("Reports", chart_icon(), false);
                });
        });

        harness.snapshot("sidebar_expandable");
    }

    #[test]
    fn test_sidebar_expandable_toggle() {
        let mut harness = Harness::new_ui(|ui| {
            Sidebar::new()
                .mode(SidebarMode::Wide)
                .show(ui, |sidebar| {
                    sidebar.item("Dashboard", home_icon(), true);
                    sidebar.expandable("Teams", users_icon(), false, |sidebar| {
                        sidebar.child_item("Engineering", false);
                        sidebar.child_item("Design", false);
                    });
                });
        });
        egui_extras::install_image_loaders(&harness.ctx);
        harness.run();

        // Snapshot: collapsed (no children visible)
        harness.snapshot("expandable_toggle_collapsed");

        // Click the "Teams" row - get its rect from the accessibility label
        let teams_node = harness.get_by_label("Teams");
        let center = teams_node.rect().center();

        // Frame 1: Press down
        harness.input_mut().events.push(egui::Event::PointerMoved(center));
        harness.input_mut().events.push(egui::Event::PointerButton {
            pos: center,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        });
        harness.run();

        // Frame 2: Release - this triggers clicked()
        harness.input_mut().events.push(egui::Event::PointerButton {
            pos: center,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::default(),
        });
        harness.run();

        // Snapshot: expanded (children now visible)
        harness.snapshot("expandable_toggle_expanded");
    }
}
