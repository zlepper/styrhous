//! Tailwind-styled horizontal tabs component for egui
//!
//! A tab component that manages its own selection state and supports optional icons.
//! Each tab can have a content closure that only executes when that tab is selected.
//!
//! # Example
//!
//! ```ignore
//! use egui::{include_image, Image};
//! use components::Tabs;
//!
//! let account_icon = Image::new(include_image!("icons/user.svg"));
//!
//! Tabs::new("settings").show(ui, |tabs| {
//!     tabs.tab("My Account", Some(account_icon), |ui| {
//!         ui.label("Account settings here");
//!     });
//!     tabs.tab("Company", None, |ui| {
//!         ui.label("Company settings here");
//!     });
//! });
//! ```

use egui::{Color32, CursorIcon, Image, Response, ScrollArea, Sense, Ui, Vec2};

use crate::colors::{gray, indigo};

// Constants from Tailwind classes
const TAB_GAP: f32 = 32.0; // space-x-8
const TAB_PADDING_X: f32 = 4.0; // px-1
const TAB_PADDING_Y: f32 = 16.0; // py-4
const ICON_SIZE: f32 = 20.0; // size-5
const ICON_MARGIN_RIGHT: f32 = 8.0; // mr-2
const ICON_MARGIN_LEFT: f32 = -2.0; // -ml-0.5
const UNDERLINE_HEIGHT: f32 = 2.0; // border-b-2
const TEXT_FONT_SIZE: f32 = 14.0; // text-sm
const SEPARATOR_HEIGHT: f32 = 1.0; // border-b

/// Colors for a tab based on its state
struct TabColors {
    text: Color32,
    border: Color32,
    icon: Color32,
}

impl TabColors {
    fn for_state(selected: bool, hovered: bool) -> Self {
        match (selected, hovered) {
            (true, _) => Self {
                text: indigo::_600,
                border: indigo::_500,
                icon: indigo::_500,
            },
            (false, true) => Self {
                text: gray::_700,
                border: gray::_300,
                icon: gray::_500,
            },
            (false, false) => Self {
                text: gray::_500,
                border: Color32::TRANSPARENT,
                icon: gray::_400,
            },
        }
    }
}

/// Stored header info for deferred rendering
struct TabHeader {
    label: String,
    icon: Option<Image<'static>>,
}

/// Builder for creating a tabs component.
///
/// The component manages its own selection state using egui's Id-based storage.
pub struct Tabs {
    id: egui::Id,
}

impl Tabs {
    /// Create a new tabs component with the given id source.
    ///
    /// The id is used to persist the selected tab across frames.
    pub fn new(id_source: impl std::hash::Hash) -> Self {
        Self {
            id: egui::Id::new(id_source),
        }
    }

    /// Show the tabs component.
    ///
    /// The closure receives a [`TabsContent`] which is used to define tabs.
    /// Each tab's content closure is only executed if that tab is selected.
    pub fn show(self, ui: &mut Ui, add_tabs: impl FnOnce(&mut TabsContent<'_>)) -> TabsResponse {
        let id = self.id;

        // Load persisted selection
        let selected: usize = ui.data(|d| d.get_temp(id)).unwrap_or(0);

        // Calculate header row height based on actual text metrics
        let sample_galley = ui.painter().layout_no_wrap(
            "Xg".to_string(), // Use characters with ascenders/descenders
            egui::FontId::proportional(TEXT_FONT_SIZE),
            Color32::PLACEHOLDER,
        );
        let header_height = TAB_PADDING_Y * 2.0 + sample_galley.size().y.max(ICON_SIZE);

        // Reserve space for headers at the top (will render into this later)
        let header_rect = ui
            .allocate_space(Vec2::new(ui.available_width(), header_height))
            .1;

        // Collect headers and render content for selected tab
        let mut new_selected = selected;
        let mut headers: Vec<TabHeader> = Vec::new();

        {
            let mut tabs_content = TabsContent {
                ui,
                selected,
                current_index: 0,
                headers: &mut headers,
            };

            add_tabs(&mut tabs_content);
        }

        let tab_count = headers.len();

        // Render headers into the reserved space
        {
            let mut header_ui = ui.new_child(egui::UiBuilder::new().max_rect(header_rect));

            ScrollArea::horizontal().id_salt(id.with("header-scroll")).show(&mut header_ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = TAB_GAP;

                    for (index, header) in headers.iter().enumerate() {
                        let is_selected = index == selected;
                        let response =
                            render_tab_header(ui, &header.label, header.icon.as_ref(), is_selected);

                        if response.clicked() {
                            new_selected = index;
                        }
                    }
                });
            });
        }

        // Keyboard navigation (arrow keys)
        if tab_count > 0 {
            if ui.input(|i| i.key_pressed(egui::Key::ArrowLeft)) && new_selected > 0 {
                new_selected -= 1;
            }
            if ui.input(|i| i.key_pressed(egui::Key::ArrowRight)) && new_selected < tab_count - 1 {
                new_selected += 1;
            }
        }

        // Draw separator line at the bottom of the header area
        let separator_rect = egui::Rect::from_min_size(
            header_rect.left_bottom(),
            Vec2::new(header_rect.width(), SEPARATOR_HEIGHT),
        );
        ui.painter().rect_filled(separator_rect, 0.0, gray::_200);

        // Persist selection if changed
        if new_selected != selected {
            ui.data_mut(|d| d.insert_temp(id, new_selected));
        }

        TabsResponse {
            selected: new_selected,
            changed: new_selected != selected,
        }
    }
}

/// Render a single tab header, returning its Response
fn render_tab_header(
    ui: &mut Ui,
    label: &str,
    icon: Option<&Image<'_>>,
    is_selected: bool,
) -> Response {
    // Create galley for measurement (use PLACEHOLDER so fallback_color works when painting)
    let galley = ui.painter().layout_no_wrap(
        label.to_string(),
        egui::FontId::proportional(TEXT_FONT_SIZE),
        Color32::PLACEHOLDER,
    );

    let icon_width = if icon.is_some() {
        ICON_MARGIN_LEFT + ICON_SIZE + ICON_MARGIN_RIGHT
    } else {
        0.0
    };

    // Use galley's actual height for proper vertical centering
    let text_height = galley.size().y;
    let content_height = text_height.max(ICON_SIZE);
    let tab_width = TAB_PADDING_X * 2.0 + icon_width + galley.size().x;
    let tab_height = TAB_PADDING_Y * 2.0 + content_height;

    // Allocate space and get response
    let (rect, response) = ui.allocate_exact_size(Vec2::new(tab_width, tab_height), Sense::click());
    let response = response.on_hover_cursor(CursorIcon::PointingHand);

    // Determine colors based on state
    let colors = TabColors::for_state(is_selected, response.hovered());

    // Calculate content vertical center
    let content_center_y = rect.center().y;

    // Draw icon if present (paint directly, no child UI needed)
    let mut text_x = rect.min.x + TAB_PADDING_X;
    if let Some(icon) = icon {
        let icon_x = text_x + ICON_MARGIN_LEFT;
        let icon_y = content_center_y - ICON_SIZE / 2.0;
        let icon_rect =
            egui::Rect::from_min_size(egui::pos2(icon_x, icon_y), Vec2::splat(ICON_SIZE));

        // Paint icon directly with tint
        icon.clone()
            .fit_to_exact_size(Vec2::splat(ICON_SIZE))
            .tint(colors.icon)
            .paint_at(ui, icon_rect);

        text_x = icon_x + ICON_SIZE + ICON_MARGIN_RIGHT;
    }

    // Draw text using the galley (proper vertical centering using actual text height)
    let text_y = content_center_y - text_height / 2.0;
    ui.painter().galley(
        egui::pos2(text_x, text_y),
        galley,
        colors.text,
    );

    // Draw underline (border)
    if colors.border != Color32::TRANSPARENT {
        let underline_rect = egui::Rect::from_min_size(
            egui::pos2(rect.min.x, rect.max.y - UNDERLINE_HEIGHT),
            Vec2::new(rect.width(), UNDERLINE_HEIGHT),
        );
        ui.painter().rect_filled(underline_rect, 0.0, colors.border);
    }

    // Add accessibility info
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), is_selected, label)
    });

    response
}

/// Content builder passed to the tabs closure.
///
/// Use this to define individual tabs with their labels, icons, and content.
pub struct TabsContent<'a> {
    ui: &'a mut Ui,
    selected: usize,
    current_index: usize,
    headers: &'a mut Vec<TabHeader>,
}

impl<'a> TabsContent<'a> {
    /// Add a tab with a label, optional icon, and content closure.
    ///
    /// The content closure is only executed if this tab is selected.
    /// Icons must have `'static` lifetime (e.g., from `include_image!`).
    pub fn tab(
        &mut self,
        label: impl Into<String>,
        icon: Option<Image<'static>>,
        content: impl FnOnce(&mut Ui),
    ) {
        let label = label.into();
        let is_selected = self.current_index == self.selected;

        // Store header info for later rendering (take ownership, no clone)
        self.headers.push(TabHeader { label, icon });

        // Render content immediately if this tab is selected
        if is_selected {
            content(self.ui);
        }

        self.current_index += 1;
    }
}

/// Response from showing a tabs component.
pub struct TabsResponse {
    /// The index of the currently selected tab.
    pub selected: usize,
    /// Whether the selection changed this frame.
    pub changed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;

    #[test]
    fn test_tabs_no_icons() {
        let mut harness = Harness::new_ui(|ui| {
            Tabs::new("test_tabs").show(ui, |tabs| {
                tabs.tab("Tab One", None, |ui| {
                    ui.label("Content for tab one");
                });
                tabs.tab("Tab Two", None, |ui| {
                    ui.label("Content for tab two");
                });
                tabs.tab("Tab Three", None, |ui| {
                    ui.label("Content for tab three");
                });
            });
        });

        harness.run();
        harness.snapshot("tabs/no_icons");
    }

    #[test]
    fn test_tabs_with_icons() {
        let mut harness = Harness::new_ui(|ui| {
            let home_icon = Image::new(egui::include_image!("icons/home.svg"));
            let users_icon = Image::new(egui::include_image!("icons/users.svg"));
            let folder_icon = Image::new(egui::include_image!("icons/folder.svg"));

            Tabs::new("test_tabs_icons").show(ui, |tabs| {
                tabs.tab("Home", Some(home_icon), |ui| {
                    ui.label("Home content");
                });
                tabs.tab("Users", Some(users_icon), |ui| {
                    ui.label("Users content");
                });
                tabs.tab("Projects", Some(folder_icon), |ui| {
                    ui.label("Projects content");
                });
            });
        });

        egui_extras::install_image_loaders(&harness.ctx);
        harness.run();
        harness.snapshot("tabs/with_icons");
    }

    #[test]
    fn test_tabs_selection_change() {
        use egui_kittest::kittest::Queryable;

        let mut harness = Harness::new_ui(|ui| {
            let response = Tabs::new("test_selection").show(ui, |tabs| {
                tabs.tab("First", None, |ui| {
                    ui.label("First content");
                });
                tabs.tab("Second", None, |ui| {
                    ui.label("Second content");
                });
            });

            // Show which tab is selected
            ui.label(format!("Selected: {}", response.selected));
        });

        harness.run();
        harness.snapshot("tabs/selection_initial");

        // Click on "Second" tab
        let second_tab = harness.get_by_label("Second");
        second_tab.click();
        harness.run();

        harness.snapshot("tabs/selection_changed");
    }
}
