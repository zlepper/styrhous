//! Menus (C07): action lists on the popover surface the selects use.
//!
//! Items are 32 points tall (44 with a secondary line); their icons sit in a
//! 16 point column so labels align, and a shortcut hint or a submenu chevron
//! sits at the trailing edge. Groups are split by full-bleed separators and
//! may open with a caption heading; destructive items go last, after a
//! separator. A [`crate::MoreButton`] or a context menu opens a menu; boards
//! and snapshots show one in place with [`preview`].
//!
//! ```ignore
//! MoreButton::new("More actions for coredns").show(ui, |menu| {
//!     if menu.item(MenuItem::new("Edit YAML").icon(Icon::PencilSquare).shortcut("E")).clicked() { … }
//!     menu.submenu(MenuItem::new("Shell").icon(Icon::CommandLine), |menu| { … });
//!     menu.separator();
//!     if menu.item(MenuItem::new("Delete").icon(Icon::Trash).danger()).clicked() { … }
//! });
//! ```

use egui::containers::menu::{MenuState, SubMenu};
use egui::{
    Id, InnerResponse, Popup, Rect, Response, Sense, Ui, Vec2, WidgetInfo, WidgetType, pos2, vec2,
};

use crate::PointingHand;
use crate::design::theme::{self, Theme, ThemeExt, ThemeScope};
use crate::design::type_scale::{self, TextRole};
use crate::design::{radius, size, space, stroke};
use crate::frames::{POPOVER_PADDING, paint_popover_ring, show_anchored_popover};
use crate::icons::Icon;
use crate::interaction::ControlState;
use crate::selection::{CheckState, paint_checkbox};
use crate::tooltip::TooltipExt;

/// A shortcut hint (`.opt .sc`): Plex Mono 11 in the item's 20 point line.
const SHORTCUT_ROLE: TextRole = type_scale::MONO.with_size(11.0, 20.0);
/// The submenu chevron (`.ic.s14`).
const CHEVRON: f32 = size::ICON_SM;

/// One menu item: a label with an optional icon, secondary line, shortcut
/// hint or checkbox.
#[derive(Clone, Copy, Debug)]
#[must_use = "add the item with `Menu::item`"]
pub struct MenuItem<'a> {
    label: &'a str,
    icon: Option<Icon>,
    detail: Option<&'a str>,
    shortcut: Option<&'a str>,
    check: Option<CheckState>,
    danger: bool,
    enabled: bool,
    disabled_reason: Option<&'a str>,
    forced_state: Option<ControlState>,
}

impl<'a> MenuItem<'a> {
    /// An item labelled `label`.
    pub fn new(label: &'a str) -> Self {
        Self {
            label,
            icon: None,
            detail: None,
            shortcut: None,
            check: None,
            danger: false,
            enabled: true,
            disabled_reason: None,
            forced_state: None,
        }
    }

    /// Lead the label with `icon` in the 16 point icon column.
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Add a secondary line under the label (`.opt.two`, 44 points tall).
    pub fn detail(mut self, detail: &'a str) -> Self {
        self.detail = Some(detail);
        self
    }

    /// Show `key` as the item's shortcut hint, display only.
    pub fn shortcut(mut self, key: &'a str) -> Self {
        self.shortcut = Some(key);
        self
    }

    /// Lead the label with a checkbox in `state`. Clicking the item does
    /// not close the menu; the caller toggles the state.
    pub fn checkbox(mut self, state: CheckState) -> Self {
        self.check = Some(state);
        self
    }

    /// A destructive item: `danger-text`, highlighted in `danger-tint`.
    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    /// Enable or disable the item. A disabled item stays readable but
    /// cannot be activated.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// The tooltip of the disabled item, saying why ("Deployment is
    /// paused").
    pub fn disabled_reason(mut self, reason: &'a str) -> Self {
        self.disabled_reason = Some(reason);
        self
    }

    /// Paint the item in `state` regardless of the pointer (boards and
    /// snapshots): [`ControlState::Hovered`] shows the highlight.
    pub fn force_state(mut self, state: ControlState) -> Self {
        self.forced_state = Some(state);
        self
    }

    fn height(&self) -> f32 {
        if self.detail.is_some() {
            size::OPTION_TWO_LINE
        } else {
            size::OPTION
        }
    }
}

impl<'a> From<&'a str> for MenuItem<'a> {
    fn from(label: &'a str) -> Self {
        Self::new(label)
    }
}

impl<'a> From<&'a String> for MenuItem<'a> {
    fn from(label: &'a String) -> Self {
        Self::new(label)
    }
}

/// What an item shows at its trailing edge.
#[derive(Clone, Copy, PartialEq)]
enum Trailing<'a> {
    None,
    Shortcut(&'a str),
    Chevron,
}

/// Builder passed to a menu's contents callback.
pub struct Menu<'a> {
    ui: &'a mut Ui,
    /// The popup to close once an action runs; `None` in a [`preview`].
    popup_id: Option<Id>,
    /// This menu opened to the left of the item that opened it, so its own
    /// submenus keep cascading left instead of covering its parent.
    opens_left: bool,
}

impl<'a> Menu<'a> {
    fn new(ui: &'a mut Ui, popup_id: Option<Id>) -> Self {
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        Self {
            ui,
            popup_id,
            opens_left: false,
        }
    }

    /// Add `item` and return its response. Clicking an enabled item
    /// without a checkbox closes the menu.
    pub fn item<'i>(&mut self, item: impl Into<MenuItem<'i>>) -> Response {
        let item = item.into();
        let trailing = item.shortcut.map_or(Trailing::None, Trailing::Shortcut);
        let response = paint_item(self.ui, &item, trailing, false, None);
        if response.clicked() && item.check.is_none() {
            self.close();
        }
        response
    }

    /// Add a plain action labelled `label`.
    pub fn action(&mut self, label: impl AsRef<str>) -> Response {
        self.item(MenuItem::new(label.as_ref()))
    }

    /// Add a destructive action. Destructive actions go last, after a
    /// [`Self::separator`].
    pub fn destructive_action(&mut self, label: impl AsRef<str>) -> Response {
        self.item(MenuItem::new(label.as_ref()).danger())
    }

    /// Add a checkbox item (column visibility, filters). Clicking it keeps
    /// the menu open; toggle the state when it is clicked.
    pub fn checkbox(&mut self, label: impl AsRef<str>, state: CheckState) -> Response {
        self.item(MenuItem::new(label.as_ref()).checkbox(state))
    }

    /// Add an item that opens a nested menu to its right on hover (or a
    /// click, Enter or →), marked with a trailing chevron.
    pub fn submenu<'i, R>(
        &mut self,
        item: impl Into<MenuItem<'i>>,
        add_contents: impl FnOnce(&mut Menu<'_>) -> R,
    ) -> InnerResponse<Option<R>> {
        let item = item.into();
        let Some(popup_id) = self.popup_id else {
            // A preview has no menu state to open a submenu from.
            let response = paint_item(self.ui, &item, Trailing::Chevron, false, None);
            return InnerResponse::new(None, response);
        };
        let id = self.ui.next_auto_id();
        let open = MenuState::from_ui(self.ui, |state, _| {
            state.open_item == Some(SubMenu::id_from_widget_id(id))
        });
        let response = paint_item(self.ui, &item, Trailing::Chevron, open, Some(true));
        let theme = self.ui.tokens();
        // `SubMenu` frames its popup from this ui's style: the popover.
        crate::frames::submenu_frame_style(self.ui.style_mut(), &theme);
        // egui opens a submenu to the right whenever it fits on screen. Under a
        // menu that already opened leftwards that would cover the parent, so
        // anchor it as if it reached the screen's edge: it flips left (S07).
        let mut anchor = response.clone();
        if self.opens_left {
            anchor.interact_rect.max.x = self.ui.ctx().content_rect().right();
        }
        let item_left = response.rect.left();
        let inner = SubMenu::new().show(self.ui, &anchor, |ui| {
            let opens_left = ui.max_rect().right() <= item_left;
            theme::scope(ui, ThemeScope::Popover, |ui| {
                constrain_width(ui);
                let mut menu = Menu::new(ui, Some(popup_id));
                menu.opens_left = opens_left;
                add_contents(&mut menu)
            })
        });
        if let Some(inner) = &inner {
            paint_popover_ring(self.ui.ctx(), &theme, &inner.response);
            label_menu(&inner.response, item.label);
        }
        InnerResponse::new(inner.map(|inner| inner.inner), response)
    }

    /// Add a caption heading over the next group (`.mlabel`).
    pub fn heading(&mut self, text: &str) {
        let theme = self.ui.tokens();
        let role = type_scale::CAPTION;
        let content = role.galley(self.ui.ctx(), text, theme.text.muted, f32::INFINITY);
        let width = row_width(self.ui, content.rect.width() + 2.0 * size::OPTION_PADDING_X);
        let (rect, _) = self
            .ui
            .allocate_exact_size(vec2(width, size::MENU_HEADING), Sense::hover());
        let galley = role.galley(
            self.ui.ctx(),
            text,
            theme.text.muted,
            width - 2.0 * size::OPTION_PADDING_X,
        );
        let pos = pos2(rect.left() + size::OPTION_PADDING_X, rect.top() + space::S8);
        let label = Rect::from_min_size(pos, galley.rect.size());
        let response =
            type_scale::interact_text(self.ui, label, self.ui.id().with(("heading", text)), text);
        type_scale::paint_text(self.ui, &response, pos, galley, theme.text.muted);
    }

    /// Add a full-bleed divider between groups of items (`.msep`).
    pub fn separator(&mut self) {
        separator(self.ui);
    }

    /// Close the whole menu (for example after an action in a submenu).
    pub fn close(&self) {
        if let Some(popup_id) = self.popup_id {
            Popup::close_id(self.ui.ctx(), popup_id);
        }
    }
}

/// A full-bleed divider between groups of popover rows (`.msep`): 4 points
/// of space, a hairline across the whole popover, 4 points of space.
pub(crate) fn separator(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(
        vec2(row_width(ui, 0.0), size::OPTION_SEPARATOR),
        Sense::hover(),
    );
    let theme = ui.tokens();
    ui.painter().hline(
        (rect.left() - POPOVER_PADDING)..=(rect.right() + POPOVER_PADDING),
        rect.center().y,
        stroke::divider(&theme),
    );
}

/// The width of a menu row whose content needs `content` points. A popup
/// first lays its contents out in a sizing pass, where every row asks for
/// its content width; afterwards the menu is as wide as its widest row and
/// every row fills it.
fn row_width(ui: &Ui, content: f32) -> f32 {
    if ui.is_sizing_pass() {
        content
    } else {
        ui.available_width()
    }
}

/// Keep a menu's contents between the design's minimum and maximum width.
fn constrain_width(ui: &mut Ui) {
    let padding = 2.0 * POPOVER_PADDING;
    ui.set_min_width(size::MENU_MIN_WIDTH - padding);
    if ui.is_sizing_pass() {
        // Later passes keep the width the sizing pass settled on.
        ui.set_max_width(size::MENU_MAX_WIDTH - padding);
    }
}

fn label_menu(response: &Response, label: &str) {
    response.ctx.accesskit_node_builder(response.id, |node| {
        node.set_role(egui::accesskit::Role::Menu);
        node.set_label(format!("{label} menu"));
    });
}

/// Show a menu `width` points wide in place, as the design boards draw an
/// open menu. Items are not interactive beyond hover.
pub fn preview<R>(
    ui: &mut Ui,
    width: f32,
    label: &str,
    add_contents: impl FnOnce(&mut Menu<'_>) -> R,
) -> InnerResponse<R> {
    let theme = ui.tokens();
    let inner = show_anchored_popover(ui, &theme, |ui| {
        ui.set_width(width - 2.0 * POPOVER_PADDING);
        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
            theme::scope(ui, ThemeScope::Popover, |ui| {
                add_contents(&mut Menu::new(ui, None))
            })
        })
        .inner
    });
    label_menu(&inner.response, label);
    inner
}

/// Show the contents of a menu popup opened by `popup` (anchored to a
/// trigger or the pointer), labelled `label` for assistive technologies.
pub(crate) fn show_popup<R>(
    popup: Popup<'_>,
    theme: &Theme,
    label: &str,
    add_contents: impl FnOnce(&mut Menu<'_>) -> R,
) -> Option<InnerResponse<R>> {
    let popup_id = popup.get_id();
    let inner = popup
        .width(size::MENU_MIN_WIDTH)
        .frame(egui::Frame::NONE)
        .show(|ui| {
            show_anchored_popover(ui, theme, |ui| {
                theme::scope(ui, ThemeScope::Popover, |ui| {
                    constrain_width(ui);
                    add_contents(&mut Menu::new(ui, Some(popup_id)))
                })
            })
            .inner
        });
    if let Some(inner) = &inner {
        label_menu(&inner.response, label);
    }
    inner
}

/// The colours of an item: its label, its icon and its highlight fill.
fn item_colors(
    theme: &Theme,
    item: &MenuItem<'_>,
) -> (egui::Color32, egui::Color32, egui::Color32) {
    if item.danger {
        (theme.danger.text, theme.danger.text, theme.danger.tint)
    } else if !item.enabled {
        (theme.text.disabled, theme.text.muted, theme.item.highlight)
    } else {
        (theme.text.primary, theme.text.muted, theme.item.highlight)
    }
}

/// Paint one item row (`.opt`) and return its response. `open` keeps a
/// submenu item highlighted; `expanded` exposes its expanded state.
fn paint_item(
    ui: &mut Ui,
    item: &MenuItem<'_>,
    trailing: Trailing<'_>,
    open: bool,
    expanded: Option<bool>,
) -> Response {
    let theme = ui.tokens();
    let role = type_scale::UI;
    let (label_color, icon_color, highlight_fill) = item_colors(&theme, item);
    let ctx = ui.ctx().clone();

    let leading = if item.icon.is_some() || item.check.is_some() {
        size::ICON + space::S8
    } else {
        0.0
    };
    let trailing_galley = match trailing {
        Trailing::Shortcut(key) => {
            Some(SHORTCUT_ROLE.galley(&ctx, key, theme.text.muted, f32::INFINITY))
        }
        _ => None,
    };
    let trailing_width = match (&trailing_galley, trailing) {
        (Some(galley), _) => galley.rect.width() + space::S8,
        (None, Trailing::Chevron) => CHEVRON + space::S8,
        _ => 0.0,
    };
    let label_width = role
        .galley(&ctx, item.label, label_color, f32::INFINITY)
        .rect
        .width()
        .max(item.detail.map_or(0.0, |detail| {
            type_scale::META
                .galley(&ctx, detail, theme.text.muted, f32::INFINITY)
                .rect
                .width()
        }));
    let desired_width = 2.0 * size::OPTION_PADDING_X + leading + label_width + trailing_width;
    let width = row_width(
        ui,
        desired_width.min(size::MENU_MAX_WIDTH - 2.0 * POPOVER_PADDING),
    );

    let sense = if item.enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(vec2(width, item.height()), sense);
    let response = if item.enabled {
        response.with_pointing_hand()
    } else {
        match item.disabled_reason {
            Some(reason) => response.tooltip(reason),
            None => response,
        }
    };
    // A clipped label shows its whole text in a truncation tooltip (S07).
    let mut label_clipped = false;
    let highlighted = match item.forced_state {
        Some(state) => state != ControlState::Rest,
        None => item.enabled && (response.hovered() || response.has_focus()) || open,
    };

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        if highlighted {
            painter.rect_filled(rect, radius::OPTION, highlight_fill);
        }
        let center_y = rect.center().y;
        let mut left = rect.left() + size::OPTION_PADDING_X;
        let leading_rect = Rect::from_min_size(
            pos2(left, center_y - size::ICON / 2.0),
            Vec2::splat(size::ICON),
        );
        if let Some(state) = item.check {
            paint_checkbox(ui, leading_rect, state, false);
        } else if let Some(icon) = item.icon {
            icon.paint(ui, leading_rect, icon_color);
        }
        left += leading;

        let right = rect.right() - size::OPTION_PADDING_X;
        match (trailing_galley, trailing) {
            (Some(galley), _) => {
                let pos = pos2(
                    right - galley.rect.width(),
                    center_y - galley.rect.height() / 2.0,
                );
                type_scale::paint_galley(ui.painter(), pos, galley, theme.text.muted);
            }
            (None, Trailing::Chevron) => {
                let chevron = Rect::from_min_size(
                    pos2(right - CHEVRON, center_y - CHEVRON / 2.0),
                    Vec2::splat(CHEVRON),
                );
                Icon::ChevronRight.paint(ui, chevron, theme.text.muted);
            }
            _ => {}
        }

        let text_width = (right - trailing_width - left).max(0.0);
        let label = role.galley_elided(ui.ctx(), item.label, label_color, text_width, 1);
        label_clipped = label.elided;
        let detail = item.detail.map(|detail| {
            type_scale::META.galley_elided(ui.ctx(), detail, theme.text.muted, text_width, 1)
        });
        let text_height =
            label.rect.height() + detail.as_ref().map_or(0.0, |detail| detail.rect.height());
        let top = center_y - text_height / 2.0;
        let label_height = label.rect.height();
        type_scale::expose_text_runs(ui, &response, pos2(left, top), &label);
        type_scale::paint_galley(ui.painter(), pos2(left, top), label, label_color);
        if let Some(detail) = detail {
            type_scale::paint_galley(
                ui.painter(),
                pos2(left, top + label_height),
                detail,
                theme.text.muted,
            );
        }
    }

    let response = if label_clipped {
        response.tooltip(item.label)
    } else {
        response
    };
    let enabled = item.enabled && ui.is_enabled();
    let name = match item.detail {
        Some(detail) => format!("{} ({detail})", item.label),
        None => item.label.to_owned(),
    };
    match item.check {
        Some(state) => state.describe(&response, enabled, &name),
        None => response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, enabled, &name)),
    }
    ui.ctx().accesskit_node_builder(response.id, |node| {
        if item.check.is_none() {
            node.set_role(egui::accesskit::Role::MenuItem);
        }
        if let Some(expanded) = expanded {
            node.set_expanded(expanded && open);
        }
    });
    response
}

#[cfg(test)]
mod tests;
