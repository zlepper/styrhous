use super::core::*;
use super::*;

/// A compact icon-only sidebar
pub struct NarrowSidebar {
    width: Option<f32>,
    dark: bool,
    background: Option<Color32>,
}

impl Default for NarrowSidebar {
    fn default() -> Self {
        Self::new()
    }
}

impl NarrowSidebar {
    /// Create a new narrow sidebar with default settings
    pub fn new() -> Self {
        Self {
            width: None,
            dark: false,
            background: None,
        }
    }

    /// Override the default width (72px)
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// Use the dark navigation treatment intended for application shells.
    pub fn dark(mut self) -> Self {
        self.dark = true;
        self
    }

    /// Use a distinct dark surface while preserving dark navigation affordances.
    pub fn dark_background(mut self, background: Color32) -> Self {
        self.dark = true;
        self.background = Some(background);
        self
    }

    /// Show the sidebar with the given content builder
    pub fn show<R>(
        self,
        ui: &mut Ui,
        add_contents: impl FnOnce(&mut NarrowSidebarContent<'_>) -> R,
    ) -> R {
        render_sidebar(
            ui,
            self.width,
            NARROW_WIDTH,
            self.background.unwrap_or(if self.dark {
                NAVIGATION_BACKGROUND
            } else {
                WHITE
            }),
            9.0,
            |child_ui| show_narrow_content(child_ui, self.dark, add_contents),
        )
    }

    /// Show the sidebar with a footer anchored below its scrollable navigation items.
    pub fn show_with_footer<R>(
        self,
        ui: &mut Ui,
        add_contents: impl FnOnce(&mut NarrowSidebarContent<'_>) -> R,
        add_footer: impl FnOnce(&mut NarrowSidebarContent<'_>),
    ) -> R {
        render_sidebar_with_footer(
            ui,
            SidebarLayout {
                width: self.width,
                default_width: NARROW_WIDTH,
                background: self.background.unwrap_or(if self.dark {
                    NAVIGATION_BACKGROUND
                } else {
                    WHITE
                }),
                top_padding: 9.0,
            },
            NARROW_ITEM_HEIGHT + spacing::SM,
            |child_ui| show_narrow_content(child_ui, self.dark, add_contents),
            |footer_ui| {
                footer_ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                    ui.add_space(spacing::SM);
                    show_narrow_content(ui, self.dark, add_footer)
                });
            },
        )
    }
}

fn show_narrow_content<R>(
    ui: &mut Ui,
    dark: bool,
    add_contents: impl FnOnce(&mut NarrowSidebarContent<'_>) -> R,
) -> R {
    ui.spacing_mut().item_spacing.y = 0.0;
    let mut content = NarrowSidebarContent {
        core: SidebarContentCore::narrow(ui, dark),
    };
    add_contents(&mut content)
}

/// Content builder for narrow sidebar
pub struct NarrowSidebarContent<'a> {
    core: SidebarContentCore<'a>,
}

impl<'a> NarrowSidebarContent<'a> {
    /// Get mutable access to the underlying Ui
    pub fn ui_mut(&mut self) -> &mut Ui {
        self.core.ui_mut()
    }

    /// Add a navigation item (icon only, text used for accessibility)
    pub fn item(
        &mut self,
        text: impl Into<WidgetText>,
        icon: Image<'_>,
        selected: bool,
    ) -> Response {
        self.core.item(text, icon, selected)
    }

    /// Add a navigation item with caller-provided tooltip text.
    pub fn item_with_tooltip(
        &mut self,
        text: impl Into<WidgetText>,
        icon: Image<'_>,
        tooltip: &str,
        selected: bool,
    ) -> Response {
        self.core
            .item_with_tooltip(text, icon, selected, Some(tooltip))
    }

    /// Add a button item with caller-provided tooltip text.
    pub fn button_with_tooltip(
        &mut self,
        text: impl Into<WidgetText>,
        icon: Image<'_>,
        tooltip: &str,
    ) -> Response {
        self.core.button_with_tooltip(text, icon, tooltip)
    }

    /// Add an avatar item (initial only, text used for accessibility)
    pub fn avatar_item(
        &mut self,
        text: impl Into<WidgetText>,
        initial: &str,
        selected: bool,
    ) -> Response {
        self.core.avatar_item(text, initial, selected)
    }

    /// Add an avatar item with caller-provided tooltip text.
    pub fn avatar_item_with_tooltip(
        &mut self,
        text: impl Into<WidgetText>,
        initial: &str,
        tooltip: &str,
        selected: bool,
    ) -> Response {
        self.core
            .avatar_item_with_tooltip(text, initial, selected, Some(tooltip))
    }

    /// Add a visual separator line
    pub fn separator(&mut self) {
        self.core.separator();
    }
}
