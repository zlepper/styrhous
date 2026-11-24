//! Select and combobox (C05): one trigger and two popover modes.
//!
//! A plain list ([`Combobox::searchable`] off) suits short fixed choices; a
//! searchable list (the default), optionally with multi-select
//! ([`Combobox::multi_select`]), suits namespaces and anything over eight
//! items. The component owns presentation, filtering, keyboard navigation
//! and the selection gestures; callers own their selected values.
//!
//! Keyboard highlight is the neutral `item-highlight` row fill and
//! selection is the trailing accent check (single) or the leading checkbox
//! (multi); a highlighted row is never a solid accent fill.

use crate::tooltip::TooltipExt;
use egui::{
    Align, Id, Key, Modifiers, Popup, PopupCloseBehavior, Rect, Response, Sense, Ui, Vec2,
    WidgetText, pos2, vec2,
};

use crate::PointingHand;
use crate::design::theme::{self, Theme, ThemeExt, ThemeScope, Tone};
use crate::design::type_scale::{self, TextRole};
use crate::design::{radius, size, space, stroke};
use crate::field::{self, FieldSize, FieldState};
use crate::fuzzy::{fuzzy_match_score, normalize_for_search};
use crate::icons::Icon;
use crate::interaction::{ControlState, Interaction, consume_key_with_text};
use crate::keycap::Keycap;
use crate::search::format_count;
use crate::selection::{CheckState, paint_checkbox};

/// The most a popover's list grows before it scrolls (about 8.5 rows, so a
/// cut-off row signals the scroll).
const DROPDOWN_MAX_HEIGHT: f32 = 300.0;
/// The default width of a trigger and its popover.
const DEFAULT_WIDTH: f32 = 256.0;
/// The `+N` chip after a trigger's values (`.sel .more`).
const MORE_ROLE: TextRole = type_scale::MONO
    .with_weight(type_scale::Weight::Medium)
    .with_size(11.0, 16.0);

/// The tone of the status lamp beside an option or the selected value.
fn status_marker_tone(is_active: bool) -> Tone {
    if is_active {
        Tone::Success
    } else {
        Tone::Neutral
    }
}

/// Paint the status lamp beside an option or the selected value, its left
/// edge at `left`, centred on `center_y`.
fn paint_status_marker(ui: &Ui, theme: &Theme, left: f32, center_y: f32, is_active: bool) {
    let lamp = crate::status::lamp_rect(left, center_y);
    crate::status::paint_status_dot(ui.painter(), theme, lamp, status_marker_tone(is_active));
}

/// Internal state persisted across frames via egui memory.
#[derive(Default, Clone)]
struct ComboboxState {
    filter_text: String,
    /// Pre-computed normalized filter chars for efficient matching.
    filter_chars: Vec<char>,
    focused_index: usize,
    /// The row the user explicitly moved the highlight to with the arrow keys.
    /// While set, the highlight follows this row through filtering and
    /// re-ranking; while unset, it stays on the top row.
    pinned_row: Option<PinnedRow>,
    was_open: bool,
    /// The height the rows took last frame, which two-line rows make taller
    /// than the estimate from the row height.
    measured_height: Option<f32>,
}

/// Identity of a row that survives filtering and re-ranking, unlike its index.
#[derive(Clone, PartialEq, Eq)]
enum PinnedRow {
    SelectAll,
    /// The `occurrence`-th (zero-based) item, in display order, whose filter
    /// text is `text`. Items may share filter text, so the text alone would
    /// pin the first duplicate and trap keyboard navigation on it.
    Item {
        text: String,
        occurrence: usize,
    },
}

#[derive(Default)]
struct KeyboardInput {
    enter_pressed: bool,
    /// Space in a multi-select popover: toggle the highlighted row.
    space_pressed: bool,
    close_requested: bool,
    scroll_to_focused: bool,
    /// Net arrow-key movement requested this frame (down is positive).
    move_focus_by: isize,
}

/// The intent produced when a user activates an option.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionAction {
    /// Replace the current selection with this option.
    Replace,
    /// Add this option to, or remove it from, the current selection.
    Toggle,
}

/// Response from rendering a combobox.
pub struct ComboboxResponse {
    /// Response for the combobox trigger.
    pub response: Response,
    /// Whether the optional select-all row was activated.
    pub select_all_clicked: bool,
}

/// Response from a combobox item.
pub struct ItemResponse {
    response: Response,
    keyboard_selected: bool,
    selection_action: Option<SelectionAction>,
}

impl ItemResponse {
    /// Returns true if the item was clicked or selected via Enter.
    pub fn clicked(&self) -> bool {
        self.response.clicked() || self.keyboard_selected
    }

    /// Returns the selection intent, if the item was activated.
    ///
    /// Plain clicks and keyboard activation replace the selection.
    /// Ctrl-click (or Command-click on macOS), a click on a multi-select
    /// row's checkbox and Space on the highlighted multi-select row toggle
    /// this item within the selection.
    pub fn selection_action(&self) -> Option<SelectionAction> {
        self.selection_action
    }

    /// Returns the underlying egui response.
    pub fn response(&self) -> &Response {
        &self.response
    }
}

/// What a row shows.
struct RowContent<'a> {
    label: &'a str,
    detail: Option<&'a str>,
    /// Single select: the trailing check. Multi-select: the checkbox.
    check: CheckState,
    status: Option<bool>,
    close_on_replace: bool,
}

/// Context passed to the render callback for each filtered option.
pub struct ComboboxUi<'a> {
    ui: &'a mut Ui,
    focused_index: usize,
    current_index: usize,
    popup_id: Id,
    enter_pressed: bool,
    space_pressed: bool,
    scroll_to_focused: bool,
    multi_select: bool,
}

impl ComboboxUi<'_> {
    /// Render a styled option row.
    pub fn item(&mut self, label: impl Into<WidgetText>, is_selected: bool) -> ItemResponse {
        self.item_with_status(label, is_selected, None)
    }

    /// Render an option row with an optional status marker.
    ///
    /// `Some(true)` paints a green marker, `Some(false)` a neutral marker, and
    /// `None` omits the marker entirely.
    pub fn item_with_status(
        &mut self,
        label: impl Into<WidgetText>,
        is_selected: bool,
        status: Option<bool>,
    ) -> ItemResponse {
        let label = label.into();
        self.row(RowContent {
            label: label.text(),
            detail: None,
            check: is_selected.into(),
            status,
            close_on_replace: true,
        })
    }

    /// Render an option with a secondary detail line (`.opt.two`): an
    /// alias or why it matched the search. An empty `detail` makes it a
    /// one-line option.
    pub fn item_with_status_detail(
        &mut self,
        label: impl Into<WidgetText>,
        detail: impl AsRef<str>,
        is_selected: bool,
        status: Option<bool>,
    ) -> ItemResponse {
        let label = label.into();
        self.row(RowContent {
            label: label.text(),
            detail: Some(detail.as_ref()).filter(|detail| !detail.is_empty()),
            check: is_selected.into(),
            status,
            close_on_replace: true,
        })
    }

    fn select_all_item(&mut self, label: &str, state: CheckState) -> ItemResponse {
        self.row(RowContent {
            label,
            detail: None,
            check: state,
            status: None,
            close_on_replace: false,
        })
    }

    fn row(&mut self, content: RowContent<'_>) -> ItemResponse {
        let is_focused = self.current_index == self.focused_index;
        self.current_index += 1;

        let height = if content.detail.is_some() {
            size::OPTION_TWO_LINE
        } else {
            size::OPTION
        };
        let (rect, response) = self
            .ui
            .allocate_exact_size(vec2(self.ui.available_width(), height), Sense::click());
        let response = response.with_pointing_hand();

        if is_focused && self.scroll_to_focused {
            response.scroll_to_me(Some(Align::Center));
        }

        let checkbox_rect = Rect::from_min_size(
            pos2(
                rect.left() + size::OPTION_PADDING_X,
                rect.center().y - size::CHECKBOX / 2.0,
            ),
            Vec2::splat(size::CHECKBOX),
        );
        // In a multi-select popover Enter closes it ("done"); the toggles
        // have already applied.
        let keyboard_selected = is_focused && self.enter_pressed && !self.multi_select;
        let keyboard_toggled = is_focused && self.space_pressed;
        let clicked = response.clicked() || keyboard_selected || keyboard_toggled;
        let selection_action = clicked.then(|| {
            let toggle_click = !keyboard_selected
                && (keyboard_toggled
                    || self
                        .ui
                        .input(|input| input.modifiers.ctrl || input.modifiers.command)
                    || self.multi_select
                        && response
                            .interact_pointer_pos()
                            .is_some_and(|pos| checkbox_rect.expand(space::S4).contains(pos)));
            if toggle_click {
                SelectionAction::Toggle
            } else {
                SelectionAction::Replace
            }
        });

        if content.close_on_replace && matches!(selection_action, Some(SelectionAction::Replace)) {
            Popup::close_id(self.ui.ctx(), self.popup_id);
        }

        let mut is_truncated = false;
        if self.ui.is_rect_visible(rect) {
            let theme = self.ui.tokens();
            let background = if is_focused {
                Some(theme.item.highlight)
            } else if response.hovered() {
                Some(theme.surface.raised_hover)
            } else {
                None
            };
            if let Some(background) = background {
                self.ui
                    .painter()
                    .rect_filled(rect, radius::OPTION, background);
            }

            let mut left = rect.left() + size::OPTION_PADDING_X;
            if self.multi_select {
                paint_checkbox(self.ui, checkbox_rect, content.check, false);
                left = checkbox_rect.right() + space::S8;
            }
            if let Some(is_active) = content.status {
                paint_status_marker(self.ui, &theme, left, rect.center().y, is_active);
                left += size::STATUS_DOT + space::S8;
            }
            let trailing_check = !self.multi_select && content.check == CheckState::On;
            let right = rect.right()
                - size::OPTION_PADDING_X
                - if trailing_check {
                    size::ICON + space::S8
                } else {
                    0.0
                };
            let width = right - left;
            let role = type_scale::UI;
            let label = field::text_galley(self.ui, role, content.label, width, false);
            let detail = content
                .detail
                .map(|detail| field::text_galley(self.ui, type_scale::META, detail, width, false));
            is_truncated = label.elided || detail.as_ref().is_some_and(|detail| detail.elided);
            let text_height =
                label.rect.height() + detail.as_ref().map_or(0.0, |detail| detail.rect.height());
            let top = rect.center().y - text_height / 2.0;
            let label_height = label.rect.height();
            type_scale::paint_galley(
                self.ui.painter(),
                pos2(left, top),
                label,
                theme.text.primary,
            );
            if let Some(detail) = detail {
                type_scale::paint_galley(
                    self.ui.painter(),
                    pos2(left, top + label_height),
                    detail,
                    theme.text.muted,
                );
            }
            if trailing_check {
                let check = Rect::from_min_size(
                    pos2(right + space::S8, rect.center().y - size::ICON / 2.0),
                    Vec2::splat(size::ICON),
                );
                Icon::Check.paint(self.ui, check, theme.accent.text);
            }
        }

        if is_truncated && response.hovered() {
            response.show_tooltip(crate::tooltip::TooltipContent::Text(content.label));
        }
        let is_enabled = self.ui.is_enabled();
        let accessibility_label = content
            .detail
            .filter(|detail| *detail != content.label)
            .map(|detail| format!("{} ({detail})", content.label))
            .unwrap_or_else(|| content.label.to_owned());
        content
            .check
            .describe(&response, is_enabled, &accessibility_label);

        ItemResponse {
            response,
            keyboard_selected: keyboard_selected || keyboard_toggled,
            selection_action,
        }
    }

    /// Close the dropdown programmatically.
    pub fn close(&self) {
        Popup::close_id(self.ui.ctx(), self.popup_id);
    }
}

/// Marker type for comboboxes without a filter function.
pub struct NoFilter;

/// Wrapper type indicating that a filter function is configured.
pub struct WithFilter<F>(F);

/// What the closed trigger leads its value with.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Lead {
    /// A status lamp: lit (success) or neutral.
    Status(bool),
    /// An icon ("All namespaces" shows a globe), `text-muted` (C05) or in
    /// the value's colour (the C16 scope pill).
    Icon { icon: Icon, muted: bool },
}

/// The select-all row of a multi-select popover.
#[derive(Clone, Debug)]
struct SelectAll {
    label: String,
    state: CheckState,
}

/// A popover shown in the flow of the layout instead of a popup, for design
/// boards and snapshots.
#[derive(Clone, Debug)]
struct Preview {
    query: String,
    highlight: Option<usize>,
    gap: f32,
    trigger: bool,
}

/// A select trigger with its popover (C05 `Select` / `Combobox`).
#[must_use = "call `show_items` to add the combobox"]
pub struct Combobox<Filter> {
    id_salt: Id,
    label: Option<WidgetText>,
    accessibility_label: Option<String>,
    placeholder: Option<String>,
    search_placeholder: Option<String>,
    search_accessibility_label: Option<String>,
    selected_text: Option<String>,
    lead: Option<Lead>,
    more: usize,
    width: Option<f32>,
    popup_width: Option<f32>,
    size: FieldSize,
    select_all: Option<SelectAll>,
    searchable: bool,
    multi_select: bool,
    noun: String,
    enabled: bool,
    disabled_reason: Option<String>,
    forced_state: Option<ControlState>,
    preview: Option<Preview>,
    read_only: bool,
    filter: Filter,
}

struct TriggerInput<'a> {
    width: f32,
    is_open: bool,
    placeholder: Option<&'a str>,
    selected_text: Option<&'a str>,
    lead: Option<Lead>,
    more: usize,
    size: FieldSize,
    enabled: bool,
    /// The trigger opens its popover (not for a read-only value).
    interactive: bool,
    forced_state: Option<ControlState>,
}

mod widget;

#[cfg(test)]
mod tests;
