//! Tailwind-styled searchable combobox component for egui.
//!
//! The component owns presentation, filtering, keyboard navigation, and the
//! selection gestures. Callers continue to own their selected values.

const ITEM_HEIGHT: f32 = 32.0;
const MULTILINE_ITEM_HEIGHT: f32 = 52.0;
const INPUT_HEIGHT: f32 = 36.0;
const ICON_SIZE: f32 = 16.0;
const DROPDOWN_MAX_HEIGHT: f32 = 300.0;
const DEFAULT_WIDTH: f32 = 256.0;
const ITEM_PADDING_X: f32 = crate::design::spacing::MD;
const INPUT_PADDING_X: f32 = crate::design::spacing::MD;
const ICON_AREA_WIDTH: f32 = 36.0;
const COMPACT_INPUT_HEIGHT: f32 = 32.0;

use egui::{
    Align, Align2, Color32, FontId, Id, Key, Modifiers, Popup, PopupCloseBehavior, Rect, Response,
    Sense, Stroke, StrokeKind, TextEdit, Ui, Vec2, WidgetText,
};
use std::sync::Arc;

use crate::PointingHand;
use crate::colors::{SUCCESS, WHITE, gray, indigo};
use crate::design::{radius, typography};
use crate::fuzzy::{fuzzy_match_score, normalize_for_search};
use crate::icons;

fn layout_truncated_text(
    ui: &Ui,
    text: &str,
    font_id: FontId,
    color: Color32,
    max_width: f32,
) -> Arc<egui::text::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font_id, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(max_width.max(0.0));
    ui.fonts_mut(|fonts| fonts.layout_job(job))
}

/// Internal state persisted across frames via egui memory.
#[derive(Default, Clone)]
struct ComboboxState {
    filter_text: String,
    /// Pre-computed normalized filter chars for efficient matching.
    filter_chars: Vec<char>,
    focused_index: usize,
    was_open: bool,
}

#[derive(Default)]
struct KeyboardInput {
    enter_pressed: bool,
    close_requested: bool,
    scroll_to_focused: bool,
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
    /// Whether the optional Select all row was activated.
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
    /// Plain clicks and keyboard activation replace the selection. Ctrl-click
    /// (or Command-click on macOS) toggles this item within the selection.
    pub fn selection_action(&self) -> Option<SelectionAction> {
        self.selection_action
    }

    /// Returns the underlying egui response.
    pub fn response(&self) -> &Response {
        &self.response
    }
}

/// Context passed to the render callback for each filtered option.
pub struct ComboboxUi<'a> {
    ui: &'a mut Ui,
    focused_index: usize,
    current_index: usize,
    popup_id: Id,
    enter_pressed: bool,
    scroll_to_focused: bool,
    item_height: f32,
}

impl<'a> ComboboxUi<'a> {
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
        self.item_internal(label, None, is_selected, status, true)
    }

    /// Render an option with a secondary detail line.
    pub fn item_with_status_detail(
        &mut self,
        label: impl Into<WidgetText>,
        detail: impl AsRef<str>,
        is_selected: bool,
        status: Option<bool>,
    ) -> ItemResponse {
        self.item_internal(label, Some(detail.as_ref()), is_selected, status, true)
    }

    fn select_all_item(&mut self, all_selected: bool) -> ItemResponse {
        self.item_internal("Select all", None, all_selected, None, false)
    }

    fn item_internal(
        &mut self,
        label: impl Into<WidgetText>,
        detail: Option<&str>,
        is_selected: bool,
        status: Option<bool>,
        close_on_replace: bool,
    ) -> ItemResponse {
        let label = label.into();
        let label_text = label.text().to_owned();
        let is_focused = self.current_index == self.focused_index;
        self.current_index += 1;

        let available_width = self.ui.available_width();
        let (rect, response) = self.ui.allocate_exact_size(
            Vec2::new(
                available_width,
                if detail.is_some() {
                    MULTILINE_ITEM_HEIGHT
                } else {
                    self.item_height
                },
            ),
            Sense::click(),
        );
        let response = response.with_pointing_hand();

        if is_focused && self.scroll_to_focused {
            response.scroll_to_me(Some(Align::Center));
        }

        let keyboard_selected = is_focused && self.enter_pressed;
        let clicked = response.clicked() || keyboard_selected;
        let selection_action = clicked.then(|| {
            if !keyboard_selected
                && self
                    .ui
                    .input(|input| input.modifiers.ctrl || input.modifiers.command)
            {
                SelectionAction::Toggle
            } else {
                SelectionAction::Replace
            }
        });

        if close_on_replace && matches!(selection_action, Some(SelectionAction::Replace)) {
            Popup::close_id(self.ui.ctx(), self.popup_id);
        }

        let mut is_truncated = false;
        if self.ui.is_rect_visible(rect) {
            let (background, text_color) = if is_focused {
                (indigo::_600, WHITE)
            } else if response.hovered() {
                (gray::_50, gray::_900)
            } else {
                (Color32::TRANSPARENT, gray::_900)
            };
            if background != Color32::TRANSPARENT {
                self.ui.painter().rect_filled(rect, 0.0, background);
            }

            if let Some(is_active) = status {
                let marker_color = if is_focused {
                    WHITE
                } else if is_active {
                    SUCCESS
                } else {
                    gray::_200
                };
                self.ui.painter().circle_filled(
                    rect.left_center() + Vec2::new(ITEM_PADDING_X + 5.0, 0.0),
                    5.0,
                    marker_color,
                );
            }

            let text_offset = ITEM_PADDING_X + if status.is_some() { 20.0 } else { 0.0 };
            let text_width =
                rect.width() - text_offset - ITEM_PADDING_X - if is_selected { 30.0 } else { 0.0 };
            let galley = layout_truncated_text(
                self.ui,
                &label_text,
                if is_selected {
                    typography::semibold(typography::BODY_SIZE)
                } else {
                    typography::body()
                },
                text_color,
                text_width,
            );
            is_truncated = galley.elided;
            self.ui.painter().galley(
                rect.left_center()
                    + Vec2::new(text_offset, if detail.is_some() { -8.0 } else { 0.0 })
                    - Vec2::new(0.0, galley.size().y / 2.0),
                galley.clone(),
                text_color,
            );
            if let Some(detail) = detail {
                let detail_color = if is_focused { WHITE } else { gray::_500 };
                let detail_galley = layout_truncated_text(
                    self.ui,
                    detail,
                    typography::metadata(),
                    detail_color,
                    text_width,
                );
                is_truncated |= detail_galley.elided;
                self.ui.painter().galley(
                    rect.left_center() + Vec2::new(text_offset, 9.0)
                        - Vec2::new(0.0, detail_galley.size().y / 2.0),
                    detail_galley,
                    detail_color,
                );
            }

            if is_selected {
                let check_color = if is_focused { WHITE } else { indigo::_600 };
                let check_center = rect.right_center() - Vec2::new(22.0, 0.0);
                let stroke = Stroke::new(2.0, check_color);
                self.ui.painter().line_segment(
                    [
                        check_center + Vec2::new(-6.0, 0.0),
                        check_center + Vec2::new(-1.5, 4.5),
                    ],
                    stroke,
                );
                self.ui.painter().line_segment(
                    [
                        check_center + Vec2::new(-1.5, 4.5),
                        check_center + Vec2::new(7.0, -5.0),
                    ],
                    stroke,
                );
            }
        }

        if is_truncated && response.hovered() {
            response.show_tooltip_text(&label_text);
        }
        let is_enabled = self.ui.is_enabled();
        let accessibility_label = detail
            .filter(|detail| *detail != label_text)
            .map(|detail| format!("{label_text} ({detail})"))
            .unwrap_or_else(|| label_text.clone());
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::Checkbox,
                is_enabled,
                is_selected,
                &accessibility_label,
            )
        });

        ItemResponse {
            response,
            keyboard_selected,
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

#[derive(Clone, Copy)]
enum ComboboxSize {
    Default,
    Compact,
}

impl ComboboxSize {
    fn input_height(self) -> f32 {
        match self {
            Self::Default => INPUT_HEIGHT,
            Self::Compact => COMPACT_INPUT_HEIGHT,
        }
    }

    fn font(self) -> FontId {
        match self {
            Self::Default => typography::body(),
            Self::Compact => typography::metadata(),
        }
    }

    fn icon_size(self) -> f32 {
        match self {
            Self::Default => ICON_SIZE,
            Self::Compact => 16.0,
        }
    }

    fn icon_area_width(self) -> f32 {
        match self {
            Self::Default => ICON_AREA_WIDTH,
            Self::Compact => 32.0,
        }
    }
}

/// A Tailwind-styled searchable combobox.
pub struct TailwindCombobox<Filter> {
    id_salt: Id,
    label: Option<WidgetText>,
    accessibility_label: Option<String>,
    placeholder: Option<String>,
    search_accessibility_label: Option<String>,
    selected_text: Option<String>,
    selected_status: Option<bool>,
    width: Option<f32>,
    size: ComboboxSize,
    select_all: Option<bool>,
    item_height: f32,
    filter: Filter,
}

struct ComboboxInput<'a> {
    width: f32,
    is_open: bool,
    placeholder: Option<&'a str>,
    accessibility_label: String,
    selected_text: Option<&'a str>,
    selected_status: Option<bool>,
    size: ComboboxSize,
}

mod widget;

#[cfg(test)]
mod tests;
