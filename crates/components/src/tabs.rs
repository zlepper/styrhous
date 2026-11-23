//! Tabs (`.tabs`, C13): a [`size::TABS`] row of [`size::TAB`] pill tabs,
//! [`TAB_GAP`] apart, in 13/20 medium `text-secondary`. The selected tab is
//! a `pill-on-bg` pill with a `pill-on-text` label; a hovered tab fills with
//! `control-fill`. A tab may carry a neutral count badge.
//!
//! The component remembers its selected tab under its id. Each tab's content
//! closure runs only while that tab is selected. ←/→ move between tabs while
//! one has keyboard focus.
//!
//! ```ignore
//! Tabs::new("pod").show(ui, |tabs| {
//!     tabs.tab("Overview", None, |ui| { /* … */ });
//!     tabs.tab_with_count("Events", 4, |ui| { /* … */ });
//! });
//! ```

use egui::{Align, Color32, Image, Layout, Rect, Response, Sense, Ui, Vec2, pos2, vec2};

use crate::PointingHand;
use crate::badge::CountBadge;
use crate::design::theme::{Theme, ThemeExt, Tone};
use crate::design::{size, space, type_scale};

/// Gap between tabs.
const TAB_GAP: f32 = space::S4;
/// Gap between a tab's icon, label and count.
const TAB_CONTENT_GAP: f32 = space::S6;

/// The fill and content colour of a tab.
fn tab_colors(theme: &Theme, selected: bool, hovered: bool) -> (Color32, Color32) {
    match (selected, hovered) {
        (true, false) => (theme.components.pill_on_bg, theme.components.pill_on_text),
        (true, true) => (
            theme.components.pill_on_bg_hover,
            theme.components.pill_on_text,
        ),
        (false, true) => (theme.control.fill, theme.text.primary),
        (false, false) => (Color32::TRANSPARENT, theme.text.secondary),
    }
}

/// Stored header info for deferred rendering
struct TabHeader {
    label: String,
    icon: Option<Image<'static>>,
    count: Option<usize>,
}

/// Builder for creating a tabs component.
///
/// The component manages its own selection state using egui's Id-based storage.
pub struct Tabs {
    id: egui::Id,
    forced_hover: Option<usize>,
}

impl Tabs {
    /// Create a new tabs component with the given id source.
    ///
    /// The id is used to persist the selected tab across frames.
    pub fn new(id_source: impl std::hash::Hash + std::fmt::Debug) -> Self {
        Self {
            id: egui::Id::new(id_source),
            forced_hover: None,
        }
    }

    /// Paint the tab at `index` hovered (design boards and snapshots).
    pub fn force_hover(mut self, index: usize) -> Self {
        self.forced_hover = Some(index);
        self
    }

    fn tab_id(&self, index: usize) -> egui::Id {
        self.id.with(("tab", index))
    }

    /// Show the tabs component.
    ///
    /// The closure receives a [`TabsContent`] which is used to define tabs.
    /// Each tab's content closure is only executed if that tab is selected.
    pub fn show(self, ui: &mut Ui, add_tabs: impl FnOnce(&mut TabsContent<'_>)) -> TabsResponse {
        let id = self.id;
        let selected: usize = ui.data(|d| d.get_temp(id)).unwrap_or(0);

        // Reserve the header row; the headers are painted after the tabs
        // are known.
        let header_rect = ui
            .allocate_space(Vec2::new(ui.available_width(), size::TABS))
            .1;

        let mut headers: Vec<TabHeader> = Vec::new();
        add_tabs(&mut TabsContent {
            ui,
            selected,
            current_index: 0,
            headers: &mut headers,
        });

        let mut new_selected = selected;
        let mut focus = None;
        let mut header_ui = ui.new_child(egui::UiBuilder::new().max_rect(header_rect));
        crate::scroll::horizontal()
            .id_salt(id.with("header-scroll"))
            .show(&mut header_ui, |ui| {
                ui.set_min_height(size::TABS);
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = TAB_GAP;
                    for (index, header) in headers.iter().enumerate() {
                        let hovered = self.forced_hover == Some(index);
                        let response =
                            render_tab(ui, self.tab_id(index), header, index == selected, hovered);
                        if response.clicked() {
                            new_selected = index;
                        }
                        if response.has_focus() {
                            // The tabs move focus themselves; keep egui from also
                            // moving it with the arrow keys.
                            let filter = egui::EventFilter {
                                horizontal_arrows: true,
                                ..Default::default()
                            };
                            ui.memory_mut(|memory| {
                                memory.set_focus_lock_filter(response.id, filter);
                            });
                            let step = ui.input_mut(|input| {
                                if input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft) {
                                    -1
                                } else if input
                                    .consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight)
                                {
                                    1
                                } else {
                                    0
                                }
                            });
                            let target = index as isize + step;
                            if step != 0 && (0..headers.len() as isize).contains(&target) {
                                new_selected = target as usize;
                                focus = Some(new_selected);
                            }
                        }
                    }
                });
            });
        if let Some(index) = focus {
            ui.memory_mut(|memory| memory.request_focus(self.tab_id(index)));
        }

        if new_selected != selected {
            ui.data_mut(|d| d.insert_temp(id, new_selected));
        }
        TabsResponse {
            selected: new_selected,
            changed: new_selected != selected,
        }
    }
}

/// The width of a tab's content and its count badge, if any.
fn tab_content(
    ui: &Ui,
    header: &TabHeader,
) -> (std::sync::Arc<egui::Galley>, Option<CountBadge>, f32) {
    let galley =
        type_scale::UI_CONTROL.galley(ui.ctx(), &header.label, Color32::PLACEHOLDER, f32::INFINITY);
    let mut width = galley.rect.width();
    if header.icon.is_some() {
        width += size::ICON + TAB_CONTENT_GAP;
    }
    let count = header
        .count
        .map(|count| CountBadge::new(count).tone(Tone::Neutral));
    if let Some(count) = &count {
        width += TAB_CONTENT_GAP + count.size(ui).x;
    }
    (galley, count, width)
}

/// Render a single tab header, returning its response.
fn render_tab(
    ui: &mut Ui,
    id: egui::Id,
    header: &TabHeader,
    is_selected: bool,
    forced_hover: bool,
) -> Response {
    let (galley, count, content_width) = tab_content(ui, header);
    let (rect, _) = ui.allocate_exact_size(
        vec2(content_width + 2.0 * size::TAB_PADDING_X, size::TAB),
        Sense::hover(),
    );
    let response = ui.interact(rect, id, Sense::click()).with_pointing_hand();
    let name = match header.count {
        Some(count) => format!("{} {count}", header.label),
        None => header.label.clone(),
    };
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::Button,
            ui.is_enabled(),
            is_selected,
            &name,
        )
    });
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let theme = ui.tokens();
    let hovered = forced_hover || response.hovered();
    let (fill, color) = tab_colors(&theme, is_selected, hovered);
    ui.painter()
        .rect_filled(rect, crate::design::radius::FULL, fill);
    let center_y = rect.center().y;
    let mut x = rect.left() + size::TAB_PADDING_X;
    if let Some(icon) = &header.icon {
        let icon_rect = Rect::from_min_size(
            pos2(x, center_y - size::ICON / 2.0),
            Vec2::splat(size::ICON),
        );
        icon.clone()
            .fit_to_exact_size(Vec2::splat(size::ICON))
            .tint(color)
            .paint_at(ui, icon_rect);
        x += size::ICON + TAB_CONTENT_GAP;
    }
    let text_width = galley.rect.width();
    let pos = pos2(x, center_y - galley.rect.height() / 2.0);
    type_scale::paint_text_as(
        ui,
        &response,
        pos,
        galley,
        color,
        egui::accesskit::Role::Button,
    );
    x += text_width + TAB_CONTENT_GAP;
    if let Some(mut count) = count {
        if is_selected {
            count = count.on_pill();
        }
        let size = count.size(ui);
        let count_rect = Rect::from_min_size(pos2(x, center_y - size.y / 2.0), size);
        let mut count_ui = ui.new_child(egui::UiBuilder::new().max_rect(count_rect));
        count.show(&mut count_ui);
    }
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

impl TabsContent<'_> {
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
        self.add(label.into(), icon, None, content);
    }

    /// Add a tab with a neutral count badge after its label ("Events 4").
    pub fn tab_with_count(
        &mut self,
        label: impl Into<String>,
        count: usize,
        content: impl FnOnce(&mut Ui),
    ) {
        self.add(label.into(), None, Some(count), content);
    }

    fn add(
        &mut self,
        label: String,
        icon: Option<Image<'static>>,
        count: Option<usize>,
        content: impl FnOnce(&mut Ui),
    ) {
        let is_selected = self.current_index == self.selected;
        self.headers.push(TabHeader { label, icon, count });
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
    use crate::test_support::UiHarnessSnapshot;
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

        crate::test_support::setup_egui(&mut harness);
        harness.run();
        harness.ui_harness("tabs/test_tabs_no_icons/no_icons");
    }

    #[test]
    fn test_tabs_with_icons() {
        let mut harness = Harness::new_ui(|ui| {
            let home_icon = Image::new(egui::include_image!("../assets/icons/home.svg"));
            let users_icon = Image::new(egui::include_image!("../assets/icons/users.svg"));
            let folder_icon = Image::new(egui::include_image!("../assets/icons/folder.svg"));

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

        crate::test_support::setup_egui(&mut harness);
        harness.run();
        harness.ui_harness("tabs/test_tabs_with_icons/with_icons");
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

        crate::test_support::setup_egui(&mut harness);
        harness.run();
        harness.ui_harness("tabs/test_tabs_selection_change/selection_initial");

        // Click on "Second" tab
        let second_tab = harness.get_by_label("Second");
        second_tab.click();
        harness.run();

        harness.ui_harness("tabs/test_tabs_selection_change/selection_changed");
    }

    #[test]
    fn arrow_keys_move_between_tabs_only_while_one_has_focus() {
        use egui::Key;
        use egui_kittest::kittest::Queryable;

        let mut harness = Harness::new_ui_state(
            |ui, selected: &mut usize| {
                *selected = Tabs::new("keyboard")
                    .show(ui, |tabs| {
                        tabs.tab("First", None, |_| {});
                        tabs.tab_with_count("Events", 4, |_| {});
                        tabs.tab("Logs", None, |_| {});
                    })
                    .selected;
            },
            0,
        );
        crate::test_support::setup_egui(&mut harness);
        harness.run();
        harness.key_press(Key::ArrowRight);
        harness.run();
        assert_eq!(*harness.state(), 0, "no tab has focus");

        harness.get_by_label("First").focus();
        harness.run();
        harness.key_press(Key::ArrowRight);
        harness.run();
        assert_eq!(*harness.state(), 1);
        assert!(harness.get_by_label("Events 4").is_focused());
        harness.key_press(Key::ArrowRight);
        harness.run();
        assert_eq!(*harness.state(), 2, "one tab per press");
        assert!(harness.get_by_label("Logs").is_focused());
        harness.key_press(Key::ArrowLeft);
        harness.run();
        assert_eq!(*harness.state(), 1);
        assert!(harness.get_by_label("Events 4").is_focused());
    }
}
