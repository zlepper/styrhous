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
    Color32, Id, Image, Response, RichText, Sense, Stroke, Ui, UiBuilder, Vec2, WidgetText,
    collapsing_header::CollapsingState,
};

use crate::colors::{NAVIGATION_BACKGROUND, WHITE, gray, indigo};
use crate::design::{radius, spacing, typography};

const WIDE_WIDTH: f32 = 292.0;
const NARROW_WIDTH: f32 = 68.0;
// Resource leaves use a slightly denser allocation than their 44px selection
// treatment. This preserves the oracle's vertical rhythm while leaving a clear,
// comfortably sized active target.
const WIDE_ITEM_HEIGHT: f32 = 36.0;
const WIDE_GROUP_HEIGHT: f32 = 44.0;
const NARROW_ITEM_HEIGHT: f32 = 52.0;
const NARROW_AVATAR_SIZE: f32 = 28.0;
const ITEM_PADDING_X: f32 = spacing::SM;
const ICON_SIZE: f32 = 16.0;
const ICON_TEXT_SPACING: f32 = spacing::SM;
const CHEVRON_SIZE: f32 = 16.0;
const CHEVRON_GAP: f32 = 8.0;

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

    fn navigation(selected: bool, hovered: bool, dark: bool) -> Self {
        if dark {
            return match (selected, hovered) {
                (true, _) => Self::new(indigo::_600, WHITE, WHITE),
                (_, true) => Self::new(gray::_800, gray::_100, gray::_300),
                _ => Self::new(Color32::TRANSPARENT, gray::_300, gray::_400),
            };
        }
        match (selected, hovered) {
            (true, _) => Self::new(indigo::_50, indigo::_600, indigo::_600),
            (_, true) => Self::new(gray::_50, gray::_700, gray::_500),
            _ => Self::new(Color32::TRANSPARENT, gray::_700, gray::_500),
        }
    }

    fn child(selected: bool, hovered: bool, dark: bool) -> Self {
        if dark {
            return match (selected, hovered) {
                (true, _) => Self::new(indigo::_600, WHITE, WHITE),
                (_, true) => Self::new(gray::_800, gray::_100, gray::_300),
                _ => Self::new(Color32::TRANSPARENT, gray::_200, gray::_400),
            };
        }
        match (selected, hovered) {
            (true, _) => Self::new(gray::_100, gray::_900, gray::_600),
            (_, true) => Self::new(gray::_50, gray::_700, gray::_500),
            _ => Self::new(Color32::TRANSPARENT, gray::_600, gray::_500),
        }
    }

    fn expandable(hovered: bool, pressed: bool, dark: bool) -> Self {
        if dark {
            return match (pressed, hovered) {
                (true, _) | (_, true) => Self::new(gray::_800, gray::_100, gray::_300),
                _ => Self::new(Color32::TRANSPARENT, gray::_200, gray::_400),
            };
        }
        match (pressed, hovered) {
            (true, _) => Self::new(gray::_100, gray::_700, gray::_500),
            (_, true) => Self::new(gray::_50, gray::_700, gray::_500),
            _ => Self::new(Color32::TRANSPARENT, gray::_700, gray::_500),
        }
    }
}

fn allocate_item(ui: &mut Ui, height: f32) -> (egui::Rect, egui::Rect, Response) {
    let available_width = ui.available_width();
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(available_width, height), Sense::click());
    let inner_rect = rect.shrink2(Vec2::new(ITEM_PADDING_X, 0.0));
    (rect, inner_rect, response)
}

fn draw_background(painter: &egui::Painter, rect: egui::Rect, color: Color32) {
    if color != Color32::TRANSPARENT {
        painter.rect_filled(rect, radius::control(), color);
    }
}

fn render_icon(ui: &mut Ui, rect: egui::Rect, icon: Image<'_>, tint: Color32) {
    let mut icon_ui = ui.new_child(UiBuilder::new().max_rect(rect).layout(
        egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
    ));
    icon_ui.add(icon.fit_to_exact_size(Vec2::splat(ICON_SIZE)).tint(tint));
}

fn truncate_text(painter: &egui::Painter, text: &str, color: Color32, max_width: f32) -> String {
    let font = typography::body();
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
        typography::body(),
        color,
    );
    rendered_text != text
}

fn render_avatar(
    painter: &egui::Painter,
    center: egui::Pos2,
    initial: &str,
    dark: bool,
    size: f32,
) {
    let (fill, text) = if dark {
        (gray::_700, gray::_100)
    } else {
        (gray::_200, gray::_600)
    };
    painter.circle_filled(center, size / 2.0, fill);
    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        initial,
        typography::body(),
        text,
    );
}

fn add_button_accessibility(response: &Response, ui: &Ui, label: &str) {
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
}

fn icon_rect_wide(inner_rect: egui::Rect, item_height: f32) -> egui::Rect {
    let icon_x = CHEVRON_SIZE + CHEVRON_GAP;
    egui::Rect::from_min_size(
        inner_rect.min + Vec2::new(icon_x, (item_height - ICON_SIZE) / 2.0),
        Vec2::splat(ICON_SIZE),
    )
}

fn icon_rect_centered(inner_rect: egui::Rect) -> egui::Rect {
    egui::Rect::from_center_size(inner_rect.center(), Vec2::splat(ICON_SIZE))
}

fn text_pos_after_icon(inner_rect: egui::Rect, item_height: f32) -> egui::Pos2 {
    let text_x = CHEVRON_SIZE + CHEVRON_GAP + ICON_SIZE + ICON_TEXT_SPACING;
    inner_rect.min + Vec2::new(text_x, (item_height - typography::BODY_SIZE) / 2.0)
}

fn text_pos_after_chevron(inner_rect: egui::Rect, item_height: f32) -> egui::Pos2 {
    let text_x = CHEVRON_SIZE + CHEVRON_GAP + 24.0;
    inner_rect.min + Vec2::new(text_x, (item_height - typography::BODY_SIZE) / 2.0)
}

struct SidebarContentCore<'a> {
    ui: &'a mut Ui,
    id: Id,
    item_height: f32,
    show_text: bool,
    dark: bool,
}

impl<'a> SidebarContentCore<'a> {
    fn wide(ui: &'a mut Ui, dark: bool) -> Self {
        let id = ui.auto_id_with("wide-sidebar");
        Self {
            ui,
            id,
            item_height: WIDE_ITEM_HEIGHT,
            show_text: true,
            dark,
        }
    }

    fn narrow(ui: &'a mut Ui, dark: bool) -> Self {
        let id = ui.auto_id_with("narrow-sidebar");
        Self {
            ui,
            id,
            item_height: NARROW_ITEM_HEIGHT,
            show_text: false,
            dark,
        }
    }

    fn ui_mut(&mut self) -> &mut Ui {
        self.ui
    }

    fn item(&mut self, text: impl Into<WidgetText>, icon: Image<'_>, selected: bool) -> Response {
        let text = text.into();
        let text_str = text.text();

        let (rect, inner_rect, response) = allocate_item(self.ui, self.item_height);
        let colors = ItemColors::navigation(selected, response.hovered(), self.dark);

        let background_rect = if self.show_text {
            inner_rect
        } else {
            inner_rect.shrink2(Vec2::new(0.0, 2.0))
        };
        draw_background(self.ui.painter(), background_rect, colors.background);

        if self.show_text {
            render_icon(
                self.ui,
                icon_rect_wide(inner_rect, self.item_height),
                icon,
                colors.icon,
            );
            let text_is_truncated = render_text(
                self.ui.painter(),
                text_pos_after_icon(inner_rect, self.item_height),
                text_str,
                colors.text,
                inner_rect.right() - text_pos_after_icon(inner_rect, self.item_height).x,
            );
            if text_is_truncated {
                response.clone().on_hover_text(text_str);
            }
        } else {
            // For narrow mode, center icon within full rect (not inner_rect)
            render_icon(self.ui, icon_rect_centered(rect), icon, colors.icon);
            response.clone().on_hover_text(text_str);
        }

        add_button_accessibility(&response, self.ui, text_str);
        response
    }

    fn avatar_item(
        &mut self,
        text: impl Into<WidgetText>,
        initial: &str,
        selected: bool,
    ) -> Response {
        self.avatar_item_with_tooltip(text, initial, selected, None)
    }

    fn avatar_item_with_tooltip(
        &mut self,
        text: impl Into<WidgetText>,
        initial: &str,
        selected: bool,
        tooltip: Option<&str>,
    ) -> Response {
        let text = text.into();
        let text_str = text.text();
        let tooltip = tooltip.unwrap_or(text_str);

        let (rect, inner_rect, response) = allocate_item(self.ui, self.item_height);
        let colors = ItemColors::navigation(selected, response.hovered(), self.dark);

        let background_rect = if self.show_text {
            inner_rect
        } else {
            inner_rect.shrink2(Vec2::new(0.0, 2.0))
        };
        draw_background(self.ui.painter(), background_rect, colors.background);

        if self.show_text {
            let avatar_x = CHEVRON_SIZE + CHEVRON_GAP;
            let avatar_center =
                inner_rect.min + Vec2::new(avatar_x + ICON_SIZE / 2.0, self.item_height / 2.0);
            render_avatar(
                self.ui.painter(),
                avatar_center,
                initial,
                self.dark,
                ICON_SIZE,
            );
            let text_is_truncated = render_text(
                self.ui.painter(),
                text_pos_after_icon(inner_rect, self.item_height),
                text_str,
                colors.text,
                inner_rect.right() - text_pos_after_icon(inner_rect, self.item_height).x,
            );
            if text_is_truncated {
                response.clone().on_hover_text(tooltip);
            }
        } else {
            // For narrow mode, center avatar within full rect (not inner_rect)
            render_avatar(
                self.ui.painter(),
                rect.center(),
                initial,
                self.dark,
                NARROW_AVATAR_SIZE,
            );
            response.clone().on_hover_text(tooltip);
        }

        add_button_accessibility(&response, self.ui, text_str);
        response
    }

    fn separator(&mut self) {
        self.ui.add_space(8.0);
        let available_width = self.ui.available_width();
        let rect = self
            .ui
            .allocate_exact_size(Vec2::new(available_width, 1.0), Sense::hover())
            .0;
        let line_rect = rect.shrink2(Vec2::new(ITEM_PADDING_X, 0.0));
        self.ui.painter().line_segment(
            [line_rect.left_center(), line_rect.right_center()],
            Stroke::new(1.0, if self.dark { gray::_800 } else { gray::_200 }),
        );
        self.ui.add_space(8.0);
    }
}

/// Shared sidebar shell - draws background and creates child UI
fn render_sidebar<R>(
    ui: &mut Ui,
    width: Option<f32>,
    default_width: f32,
    background: Color32,
    top_padding: f32,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> R {
    let rect = ui.available_rect_before_wrap();
    // Use specified width, or available width capped at default (handles being inside a SidePanel)
    let width = width.unwrap_or_else(|| rect.width().min(default_width));
    let sidebar_rect = egui::Rect::from_min_size(rect.min, Vec2::new(width, rect.height()));
    ui.painter().rect_filled(sidebar_rect, 0.0, background);

    let mut child_ui = ui.new_child(
        UiBuilder::new()
            .max_rect(sidebar_rect)
            .layout(egui::Layout::top_down(egui::Align::LEFT)),
    );

    let scroll_id = ui.auto_id_with("sidebar-scroll");
    egui::ScrollArea::vertical()
        .id_salt(scroll_id)
        .auto_shrink(false)
        .show(&mut child_ui, |ui| {
            ui.add_space(top_padding);
            add_contents(ui)
        })
        .inner
}

/// Response from an expandable sidebar section
pub struct ExpandableResponse<R> {
    /// Response from clicking the header
    pub header: Response,
    /// Result from the children closure, or None if collapsed
    pub children: Option<R>,
    /// Whether the section is currently expanded
    pub is_open: bool,
}

/// A full-featured sidebar with icons, text, and expandable sections
pub struct WideSidebar {
    width: Option<f32>,
    dark: bool,
}

impl Default for WideSidebar {
    fn default() -> Self {
        Self::new()
    }
}

impl WideSidebar {
    /// Create a new wide sidebar with default settings
    pub fn new() -> Self {
        Self {
            width: None,
            dark: false,
        }
    }

    /// Override the default width (256px)
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// Use the dark navigation treatment intended for application shells.
    pub fn dark(mut self) -> Self {
        self.dark = true;
        self
    }

    /// Show the sidebar with the given content builder
    pub fn show<R>(
        self,
        ui: &mut Ui,
        add_contents: impl FnOnce(&mut WideSidebarContent<'_>) -> R,
    ) -> R {
        render_sidebar(
            ui,
            self.width,
            WIDE_WIDTH,
            if self.dark {
                NAVIGATION_BACKGROUND
            } else {
                WHITE
            },
            6.0,
            |child_ui| {
                let mut content = WideSidebarContent {
                    core: SidebarContentCore::wide(child_ui, self.dark),
                };
                add_contents(&mut content)
            },
        )
    }
}

/// Content builder for wide sidebar
pub struct WideSidebarContent<'a> {
    core: SidebarContentCore<'a>,
}

impl<'a> WideSidebarContent<'a> {
    /// Get mutable access to the underlying Ui
    pub fn ui_mut(&mut self) -> &mut Ui {
        self.core.ui_mut()
    }

    /// Add a navigation item with icon and text
    pub fn item(
        &mut self,
        text: impl Into<WidgetText>,
        icon: Image<'_>,
        selected: bool,
    ) -> Response {
        self.core.item(text, icon, selected)
    }

    /// Add an avatar item with a circular initial badge
    pub fn avatar_item(
        &mut self,
        text: impl Into<WidgetText>,
        initial: &str,
        selected: bool,
    ) -> Response {
        self.core.avatar_item(text, initial, selected)
    }

    /// Add a full-width primary navigation item without an icon or disclosure control.
    pub fn primary_text_item(&mut self, text: impl Into<WidgetText>, selected: bool) -> Response {
        let text = text.into();
        let text_str = text.text();
        let (rect, inner_rect, response) = allocate_item(self.core.ui, WIDE_ITEM_HEIGHT);
        let colors = ItemColors::navigation(selected, response.hovered(), self.core.dark);
        draw_background(self.core.ui.painter(), inner_rect, colors.background);

        let text_pos = inner_rect.min
            + Vec2::new(
                spacing::XL,
                (WIDE_ITEM_HEIGHT - typography::BODY_SIZE) / 2.0,
            );
        let text_is_truncated = render_text(
            self.core.ui.painter(),
            text_pos,
            text_str,
            colors.text,
            rect.right() - ITEM_PADDING_X - text_pos.x,
        );
        add_button_accessibility(&response, self.core.ui, text_str);
        if text_is_truncated {
            response.clone().on_hover_text(text_str);
        }

        response
    }

    /// Add a section header label
    pub fn section_header(&mut self, text: &str) {
        self.core.ui.add_space(12.0);
        let padding_x = ITEM_PADDING_X + 4.0;
        self.core.ui.horizontal(|ui| {
            ui.add_space(padding_x);
            ui.label(
                RichText::new(text.to_uppercase())
                    .font(typography::metadata())
                    .color(if self.core.dark {
                        gray::_400
                    } else {
                        gray::_500
                    }),
            );
        });
        self.core.ui.add_space(2.0);
    }

    /// Add an expandable section with child items
    pub fn expandable<R>(
        &mut self,
        text: impl Into<WidgetText>,
        icon: Image<'_>,
        default_open: bool,
        add_children: impl FnOnce(&mut WideSidebarContent<'_>) -> R,
    ) -> ExpandableResponse<R> {
        let text = text.into();
        let text_str = text.text();

        let id = self.core.id.with(text_str);
        let mut state =
            CollapsingState::load_with_default_open(self.core.ui.ctx(), id, default_open);
        let is_open = state.is_open();

        // Chevron
        let chevron = if is_open {
            Image::new(egui::include_image!("icons/chevron-down.svg"))
        } else {
            Image::new(egui::include_image!("icons/chevron-right.svg"))
        };
        let chevron =
            chevron
                .fit_to_exact_size(Vec2::splat(CHEVRON_SIZE))
                .tint(if self.core.dark {
                    gray::_500
                } else {
                    gray::_400
                });

        let (rect, inner_rect, response) = allocate_item(self.core.ui, WIDE_GROUP_HEIGHT);
        let colors = ItemColors::expandable(
            response.hovered(),
            response.is_pointer_button_down_on(),
            self.core.dark,
        );

        draw_background(self.core.ui.painter(), rect, colors.background);

        // Chevron
        let chevron_rect = egui::Rect::from_min_size(
            inner_rect.min + Vec2::new(8.0, (WIDE_GROUP_HEIGHT - CHEVRON_SIZE) / 2.0),
            Vec2::splat(CHEVRON_SIZE),
        );
        let mut chevron_ui =
            self.core
                .ui
                .new_child(UiBuilder::new().max_rect(chevron_rect).layout(
                    egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                ));
        chevron_ui.add(chevron);

        // Icon
        render_icon(
            self.core.ui,
            icon_rect_wide(inner_rect, WIDE_ITEM_HEIGHT),
            icon,
            colors.icon,
        );

        // Text
        let text_is_truncated = render_text(
            self.core.ui.painter(),
            text_pos_after_icon(inner_rect, WIDE_GROUP_HEIGHT),
            text_str,
            colors.text,
            inner_rect.right() - text_pos_after_icon(inner_rect, WIDE_GROUP_HEIGHT).x,
        );
        add_button_accessibility(&response, self.core.ui, text_str);
        if text_is_truncated {
            response.clone().on_hover_text(text_str);
        }

        if response.clicked() {
            state.toggle(self.core.ui);
        }
        state.store(self.core.ui.ctx());

        let is_open_now = state.is_open();
        let children = if is_open_now {
            self.core.ui.add_space(2.0);
            let result = add_children(self);
            self.core.ui.add_space(2.0);
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

    /// Add an expandable resource group without an icon.
    ///
    /// This compact treatment keeps the disclosure chevron as the only hierarchy
    /// cue, leaving resource names as far left as possible.
    pub fn expandable_text<R>(
        &mut self,
        text: impl Into<WidgetText>,
        default_open: bool,
        add_children: impl FnOnce(&mut WideSidebarContent<'_>) -> R,
    ) -> ExpandableResponse<R> {
        let text = text.into();
        let id = self.core.id.with(text.text());
        self.expandable_text_with_layout(id, text, default_open, 0.0, add_children)
    }

    /// Add an indented expandable resource group with a caller-provided stable ID.
    ///
    /// This is useful for nested groups that share a display label, such as the
    /// `Other` subgroup below several API groups.
    pub fn nested_expandable_text<R>(
        &mut self,
        id_source: impl std::hash::Hash,
        text: impl Into<WidgetText>,
        default_open: bool,
        add_children: impl FnOnce(&mut WideSidebarContent<'_>) -> R,
    ) -> ExpandableResponse<R> {
        let id = self.core.id.with(id_source);
        self.expandable_text_with_layout(id, text.into(), default_open, 20.0, add_children)
    }

    fn expandable_text_with_layout<R>(
        &mut self,
        id: Id,
        text: WidgetText,
        default_open: bool,
        indent: f32,
        add_children: impl FnOnce(&mut WideSidebarContent<'_>) -> R,
    ) -> ExpandableResponse<R> {
        let text_str = text.text();

        let mut state =
            CollapsingState::load_with_default_open(self.core.ui.ctx(), id, default_open);
        let is_open = state.is_open();
        let chevron = if is_open {
            Image::new(egui::include_image!("icons/chevron-down.svg"))
        } else {
            Image::new(egui::include_image!("icons/chevron-right.svg"))
        }
        .fit_to_exact_size(Vec2::splat(CHEVRON_SIZE))
        .tint(if self.core.dark {
            gray::_500
        } else {
            gray::_400
        });

        let group_height = WIDE_GROUP_HEIGHT;
        let (rect, inner_rect, response) = allocate_item(self.core.ui, group_height);
        let colors = ItemColors::expandable(
            response.hovered(),
            response.is_pointer_button_down_on(),
            self.core.dark,
        );
        draw_background(self.core.ui.painter(), rect, colors.background);

        let chevron_rect = egui::Rect::from_min_size(
            inner_rect.min + Vec2::new(12.0 + indent, (group_height - CHEVRON_SIZE) / 2.0),
            Vec2::splat(CHEVRON_SIZE),
        );
        let mut chevron_ui =
            self.core
                .ui
                .new_child(UiBuilder::new().max_rect(chevron_rect).layout(
                    egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                ));
        chevron_ui.add(chevron);

        let text_pos = text_pos_after_chevron(inner_rect, group_height) + Vec2::new(indent, 0.0);
        let text_is_truncated = render_text(
            self.core.ui.painter(),
            text_pos,
            text_str,
            colors.text,
            inner_rect.right() - text_pos.x,
        );
        add_button_accessibility(&response, self.core.ui, text_str);
        if text_is_truncated {
            response.clone().on_hover_text(text_str);
        }

        if response.clicked() {
            state.toggle(self.core.ui);
        }
        state.store(self.core.ui.ctx());

        let is_open_now = state.is_open();
        let children = if is_open_now {
            self.core.ui.add_space(6.0);
            let result = add_children(self);
            self.core.ui.add_space(2.0);
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
    pub fn child_item(&mut self, text: impl Into<WidgetText>, selected: bool) -> Response {
        self.child_item_with_indent(text.into(), selected, 0.0)
    }

    /// Add a leaf item nested below a child expandable group.
    pub fn nested_child_item(&mut self, text: impl Into<WidgetText>, selected: bool) -> Response {
        self.child_item_with_indent(text.into(), selected, 40.0)
    }

    fn child_item_with_indent(
        &mut self,
        text: WidgetText,
        selected: bool,
        indent: f32,
    ) -> Response {
        let text_str = text.text();

        let (rect, _, response) = allocate_item(self.core.ui, WIDE_ITEM_HEIGHT);
        let colors = ItemColors::child(selected, response.hovered(), self.core.dark);

        // Child text aligns with the parent disclosure control. The absence of a
        // chevron communicates that it is a leaf without spending width on a gutter.
        let text_x = ITEM_PADDING_X + CHEVRON_SIZE + CHEVRON_GAP + indent;
        let bg_indent = ITEM_PADDING_X + 8.0 + indent;
        let bg_rect = egui::Rect::from_min_max(
            rect.min + Vec2::new(bg_indent, 0.0),
            rect.max - Vec2::new(44.0, if selected { -3.8 } else { 0.0 }),
        );

        draw_background(self.core.ui.painter(), bg_rect, colors.background);

        let text_pos =
            rect.min + Vec2::new(text_x, (WIDE_ITEM_HEIGHT - typography::BODY_SIZE) / 2.0);
        let text_is_truncated = render_text(
            self.core.ui.painter(),
            text_pos,
            text_str,
            colors.text,
            rect.right() - ITEM_PADDING_X - text_pos.x,
        );
        add_button_accessibility(&response, self.core.ui, text_str);
        if text_is_truncated {
            response.clone().on_hover_text(text_str);
        }

        response
    }

    /// Add a visual separator line
    pub fn separator(&mut self) {
        self.core.separator();
    }
}

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
            |child_ui| {
                child_ui.spacing_mut().item_spacing.y = 0.0;
                let mut content = NarrowSidebarContent {
                    core: SidebarContentCore::narrow(child_ui, self.dark),
                };
                add_contents(&mut content)
            },
        )
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn create_harness<'a>(app: impl FnMut(&mut Ui) + 'a) -> Harness<'a> {
        let mut harness = Harness::new_ui(app);
        crate::test_support::setup_egui(&mut harness);
        harness.run();
        harness
    }

    macro_rules! test_icon {
        ($name:ident, $path:literal) => {
            fn $name() -> Image<'static> {
                Image::new(egui::include_image!($path))
            }
        };
    }

    test_icon!(home_icon, "icons/home.svg");
    test_icon!(users_icon, "icons/users.svg");
    test_icon!(folder_icon, "icons/folder.svg");
    test_icon!(calendar_icon, "icons/calendar.svg");
    test_icon!(document_icon, "icons/document.svg");
    test_icon!(chart_icon, "icons/chart-bar.svg");

    #[test]
    fn test_sidebar_wide_mode() {
        let mut harness = create_harness(|ui| {
            WideSidebar::new().show(ui, |sidebar| {
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

        harness.snapshot("sidebars/wide");
    }

    #[test]
    fn test_sidebar_narrow_mode() {
        let mut harness = create_harness(|ui| {
            NarrowSidebar::new().show(ui, |sidebar| {
                sidebar.item("Dashboard", home_icon(), true);
                sidebar.item("Team", users_icon(), false);
                sidebar.item("Projects", folder_icon(), false);
                sidebar.item("Calendar", calendar_icon(), false);
                sidebar.item("Documents", document_icon(), false);
                sidebar.item("Reports", chart_icon(), false);
            });
        });

        harness.snapshot("sidebars/narrow");
    }

    #[test]
    fn test_sidebar_narrow_avatars() {
        let mut harness = create_harness(|ui| {
            NarrowSidebar::new().show(ui, |sidebar| {
                sidebar.avatar_item("Production", "P", true);
                sidebar.avatar_item("Development", "D", false);
                sidebar.avatar_item("Staging", "S", false);
            });
        });

        harness.snapshot("sidebars/narrow_avatars");
    }

    #[test]
    fn test_sidebar_dark_mode() {
        let mut harness = create_harness(|ui| {
            WideSidebar::new().dark().show(ui, |sidebar| {
                sidebar.section_header("Resources");
                sidebar.expandable("core", folder_icon(), true, |sidebar| {
                    sidebar.child_item("pods", true);
                    sidebar.child_item("services", false);
                });
                sidebar.expandable("apps", folder_icon(), false, |_sidebar| {});
            });
        });

        harness.snapshot("sidebars/dark");
    }

    #[test]
    fn test_sidebar_primary_text_item() {
        let mut harness = create_harness(|ui| {
            WideSidebar::new().dark().show(ui, |sidebar| {
                sidebar.primary_text_item("nodes", true);
                sidebar.primary_text_item("namespaces", false);
            });
        });

        harness.snapshot("sidebars/primary_text_item");
    }

    #[test]
    fn test_sidebar_expandable_sections() {
        let mut harness = create_harness(|ui| {
            WideSidebar::new().show(ui, |sidebar| {
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

        harness.snapshot("sidebars/expandable");
    }

    #[test]
    fn test_sidebar_expandable_toggle() {
        let mut harness = Harness::new_ui(|ui| {
            WideSidebar::new().show(ui, |sidebar| {
                sidebar.item("Dashboard", home_icon(), true);
                sidebar.expandable("Teams", users_icon(), false, |sidebar| {
                    sidebar.child_item("Engineering", false);
                    sidebar.child_item("Design", false);
                });
            });
        });
        crate::test_support::setup_egui(&mut harness);
        harness.run();

        harness.snapshot("sidebars/expandable_toggle_collapsed");

        let teams_node = harness.get_by_label("Teams");
        let center = teams_node.rect().center();

        harness
            .input_mut()
            .events
            .push(egui::Event::PointerMoved(center));
        harness.input_mut().events.push(egui::Event::PointerButton {
            pos: center,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        });
        harness.run();

        harness.input_mut().events.push(egui::Event::PointerButton {
            pos: center,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::default(),
        });
        harness.run();

        harness.snapshot("sidebars/expandable_toggle_expanded");
    }

    #[test]
    fn test_sidebar_parent_hover_is_a_full_rounded_row() {
        let mut harness = create_harness(|ui| {
            WideSidebar::new().dark().show(ui, |sidebar| {
                sidebar.expandable_text("Apps & Containers", true, |sidebar| {
                    sidebar.child_item("pods", false);
                    sidebar.child_item("deployments", false);
                });
            });
        });

        harness.get_by_label("Apps & Containers").hover();
        harness.run();

        harness.snapshot("sidebars/resource_parent_hover");
    }

    #[test]
    fn test_sidebar_open_resource_parent_keeps_the_closed_row_height() {
        let heights = Rc::new(RefCell::new((0.0, 0.0)));
        let heights_for_ui = heights.clone();
        let _harness = create_harness(move |ui| {
            WideSidebar::new().dark().show(ui, |sidebar| {
                let open = sidebar.expandable_text("Open resources", true, |_sidebar| {});
                heights_for_ui.borrow_mut().0 = open.header.rect.height();

                let closed = sidebar.expandable_text("Closed resources", false, |_sidebar| {});
                heights_for_ui.borrow_mut().1 = closed.header.rect.height();
            });
        });

        let (open_height, closed_height) = *heights.borrow();
        assert_eq!(open_height, closed_height);
        assert_eq!(open_height, WIDE_GROUP_HEIGHT);
    }

    #[test]
    fn test_sidebar_full_text_tooltips_only_appear_when_truncated() {
        let mut visible_label = create_harness(|ui| {
            WideSidebar::new().dark().show(ui, |sidebar| {
                sidebar.child_item("pods", false);
            });
        });
        visible_label.get_by_label("pods").hover();
        visible_label.run();
        visible_label.snapshot("sidebars/tooltip_visible_label");

        let mut truncated_label = create_harness(|ui| {
            WideSidebar::new().width(160.0).dark().show(ui, |sidebar| {
                sidebar.child_item("very-long-resource-name-that-needs-truncation", false);
            });
        });
        truncated_label
            .get_by_label("very-long-resource-name-that-needs-truncation")
            .hover();
        truncated_label.run();
        truncated_label.snapshot("sidebars/tooltip_truncated_label");
    }

    #[test]
    fn test_sidebar_child_item_click() {
        let clicked: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        let clicked_clone = clicked.clone();

        let mut harness = Harness::new_ui(move |ui| {
            WideSidebar::new().show(ui, |sidebar| {
                sidebar.item("Dashboard", home_icon(), true);
                sidebar.expandable("Teams", users_icon(), true, |sidebar| {
                    if sidebar.child_item("Engineering", false).clicked() {
                        *clicked_clone.borrow_mut() = Some("Engineering".to_string());
                    }
                    if sidebar.child_item("Design", false).clicked() {
                        *clicked_clone.borrow_mut() = Some("Design".to_string());
                    }
                });
            });
        });
        crate::test_support::setup_egui(&mut harness);
        harness.run();

        // Click on Engineering using accessibility
        harness.get_by_label("Engineering").click();
        harness.run();

        assert_eq!(
            *clicked.borrow(),
            Some("Engineering".to_string()),
            "Engineering should be clicked via accessibility"
        );
    }
}
