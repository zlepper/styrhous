use super::core::*;
use super::*;

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
        add_selectable_accessibility(&response, self.core.ui, text_str, selected);
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
            Image::new(egui::include_image!("../icons/chevron-down.svg"))
        } else {
            Image::new(egui::include_image!("../icons/chevron-right.svg"))
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
        if text_is_truncated {
            response.clone().on_hover_text(text_str);
        }

        if response.clicked() {
            state.toggle(self.core.ui);
        }
        state.store(self.core.ui.ctx());

        let is_open_now = state.is_open();
        add_expandable_accessibility(&response, self.core.ui, text_str, is_open_now);
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
        id_source: impl std::hash::Hash + std::fmt::Debug,
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
            Image::new(egui::include_image!("../icons/chevron-down.svg"))
        } else {
            Image::new(egui::include_image!("../icons/chevron-right.svg"))
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
        if text_is_truncated {
            response.clone().on_hover_text(text_str);
        }

        if response.clicked() {
            state.toggle(self.core.ui);
        }
        state.store(self.core.ui.ctx());

        let is_open_now = state.is_open();
        add_expandable_accessibility(&response, self.core.ui, text_str, is_open_now);
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
        add_selectable_accessibility(&response, self.core.ui, text_str, selected);
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
