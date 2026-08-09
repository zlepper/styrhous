//! Tailwind-styled searchable combobox component for egui.
//!
//! The component owns presentation, filtering, keyboard navigation, and the
//! selection gestures. Callers continue to own their selected values.

const ITEM_HEIGHT: f32 = 32.0;
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
    ScrollArea, Sense, Stroke, StrokeKind, TextEdit, Ui, Vec2, WidgetText,
};
use std::sync::Arc;

use crate::colors::{SUCCESS, WHITE, gray, indigo};
use crate::design::{radius, typography};
use crate::fuzzy::{matches_fuzzy, normalize_for_search};
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
        self.item_internal(label, is_selected, status, true)
    }

    fn select_all_item(&mut self, all_selected: bool) -> ItemResponse {
        self.item_internal("Select all", all_selected, None, false)
    }

    fn item_internal(
        &mut self,
        label: impl Into<WidgetText>,
        is_selected: bool,
        status: Option<bool>,
        close_on_replace: bool,
    ) -> ItemResponse {
        let label = label.into();
        let label_text = label.text().to_owned();
        let is_focused = self.current_index == self.focused_index;
        self.current_index += 1;

        let available_width = self.ui.available_width();
        let (rect, response) = self
            .ui
            .allocate_exact_size(Vec2::new(available_width, ITEM_HEIGHT), Sense::click());

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
                rect.left_center() + Vec2::new(text_offset, 0.0)
                    - Vec2::new(0.0, galley.size().y / 2.0),
                galley,
                text_color,
            );

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
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, is_enabled, &label_text)
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
    placeholder: Option<String>,
    selected_text: Option<String>,
    selected_status: Option<bool>,
    width: Option<f32>,
    size: ComboboxSize,
    select_all: Option<bool>,
    filter: Filter,
}

impl TailwindCombobox<NoFilter> {
    /// Create a new combobox with an explicit ID.
    pub fn new(id_salt: impl std::hash::Hash) -> Self {
        TailwindCombobox {
            id_salt: Id::new(id_salt),
            label: None,
            placeholder: None,
            selected_text: None,
            selected_status: None,
            width: None,
            size: ComboboxSize::Default,
            select_all: None,
            filter: NoFilter,
        }
    }

    /// Create a new combobox with the label as the ID source.
    pub fn from_label(label: impl Into<WidgetText>) -> Self {
        let label = label.into();
        TailwindCombobox {
            id_salt: Id::new(label.text()),
            label: Some(label),
            placeholder: None,
            selected_text: None,
            selected_status: None,
            width: None,
            size: ComboboxSize::Default,
            select_all: None,
            filter: NoFilter,
        }
    }

    /// Configure the text used for fuzzy filtering.
    pub fn filter_by<T, F>(self, filter_fn: F) -> TailwindCombobox<WithFilter<F>>
    where
        F: Fn(&T) -> &str,
    {
        TailwindCombobox {
            id_salt: self.id_salt,
            label: self.label,
            placeholder: self.placeholder,
            selected_text: self.selected_text,
            selected_status: self.selected_status,
            width: self.width,
            size: self.size,
            select_all: self.select_all,
            filter: WithFilter(filter_fn),
        }
    }
}

impl<Filter> TailwindCombobox<Filter> {
    /// Set the placeholder shown when the input is empty.
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    /// Set the text displayed while the combobox is closed.
    pub fn selected_text(mut self, text: impl Into<String>) -> Self {
        self.selected_text = Some(text.into());
        self
    }

    /// Set the optional status marker shown beside the closed selected value.
    pub fn selected_status(mut self, status: Option<bool>) -> Self {
        self.selected_status = status;
        self
    }

    /// Set the width of the combobox.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width.max(0.0));
        self
    }

    /// Use the smaller sizing intended for dense toolbar controls.
    pub fn compact(mut self) -> Self {
        self.size = ComboboxSize::Compact;
        self
    }

    /// Enable a Select all row while the search field is empty.
    ///
    /// `all_selected` controls its checkmark. Its activation is reported by
    /// [`ComboboxResponse::select_all_clicked`].
    pub fn select_all(mut self, all_selected: bool) -> Self {
        self.select_all = Some(all_selected);
        self
    }
}

impl<F> TailwindCombobox<WithFilter<F>> {
    /// Show the combobox and call `render` for every filtered option.
    pub fn show_items<'a, T, I, R>(self, ui: &mut Ui, items: I, mut render: R) -> ComboboxResponse
    where
        F: Fn(&T) -> &str,
        I: IntoIterator<Item = &'a T>,
        T: 'a,
        R: FnMut(&mut ComboboxUi<'_>, &T),
    {
        let TailwindCombobox {
            id_salt,
            label,
            placeholder,
            selected_text,
            selected_status,
            width,
            size,
            select_all,
            filter: WithFilter(filter_fn),
        } = self;

        let state_id = ui.make_persistent_id(id_salt);
        let mut state = ui.ctx().memory_mut(|mem| {
            mem.data
                .get_temp::<ComboboxState>(state_id)
                .unwrap_or_default()
        });

        if let Some(label) = &label {
            ui.label(label.clone());
        }

        let width = width.unwrap_or(DEFAULT_WIDTH);
        let is_open = state.was_open;
        let keyboard = Self::handle_keyboard(ui, &mut state, is_open);
        let input_response = Self::render_input(
            ui,
            &mut state,
            width,
            is_open,
            placeholder.as_deref(),
            selected_text.as_deref(),
            selected_status,
            size,
        );

        let is_enabled = ui.is_enabled();
        let label_text = label.as_ref().map(|label| label.text().to_owned());
        input_response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::ComboBox,
                is_enabled,
                label_text.as_deref().unwrap_or("Combobox"),
            )
        });

        let popup_id = Popup::default_response_id(&input_response);
        state.filter_chars = normalize_for_search(&state.filter_text).collect();

        let filtered_items: Vec<_> = items
            .into_iter()
            .filter(|item| matches_fuzzy(filter_fn(item), &state.filter_chars))
            .collect();
        let select_all_visible = select_all.is_some() && state.filter_text.is_empty();
        let item_count = filtered_items.len() + usize::from(select_all_visible);
        let item_spacing = ui.spacing().item_spacing.y;
        let content_height =
            item_count as f32 * ITEM_HEIGHT + item_count.saturating_sub(1) as f32 * item_spacing;
        let dropdown_height = content_height.clamp(ITEM_HEIGHT, DROPDOWN_MAX_HEIGHT);
        let scroll_to_focused = keyboard.scroll_to_focused && content_height > DROPDOWN_MAX_HEIGHT;
        if item_count > 0 {
            state.focused_index = state.focused_index.min(item_count - 1);
        }

        let mut select_all_clicked = false;
        Popup::menu(&input_response)
            .width(width)
            .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                ui.set_min_height(dropdown_height);
                ScrollArea::vertical()
                    .max_height(dropdown_height)
                    .min_scrolled_height(dropdown_height)
                    .show(ui, |ui| {
                        let mut cb = ComboboxUi {
                            ui,
                            focused_index: state.focused_index,
                            current_index: 0,
                            popup_id,
                            enter_pressed: keyboard.enter_pressed,
                            scroll_to_focused,
                        };

                        if let Some(all_selected) = select_all.filter(|_| select_all_visible)
                            && cb.select_all_item(all_selected).clicked()
                        {
                            select_all_clicked = true;
                        }

                        for item in &filtered_items {
                            render(&mut cb, item);
                        }
                    });
            });

        if keyboard.close_requested {
            Popup::close_id(ui.ctx(), popup_id);
        }
        let is_open_after = Popup::is_id_open(ui.ctx(), popup_id);
        if state.was_open && !is_open_after {
            state.filter_text.clear();
            state.filter_chars.clear();
            state.focused_index = 0;
        }
        state.was_open = is_open_after;
        ui.ctx()
            .memory_mut(|mem| mem.data.insert_temp(state_id, state));

        ComboboxResponse {
            response: input_response,
            select_all_clicked,
        }
    }

    fn render_input(
        ui: &mut Ui,
        state: &mut ComboboxState,
        width: f32,
        is_open: bool,
        placeholder: Option<&str>,
        selected_text: Option<&str>,
        selected_status: Option<bool>,
        size: ComboboxSize,
    ) -> Response {
        let corner_radius = radius::control();
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(width, size.input_height()), Sense::click());
        let has_focus = response.has_focus()
            || ui.memory(|memory| memory.has_focus(response.id.with("input")))
            || is_open;

        if ui.is_rect_visible(rect) {
            ui.painter().rect_filled(rect, corner_radius, WHITE);
            ui.painter().rect_stroke(
                rect,
                corner_radius,
                Stroke::new(
                    if has_focus { 2.0 } else { 1.0 },
                    if has_focus { indigo::_500 } else { gray::_300 },
                ),
                StrokeKind::Inside,
            );

            let icon_rect = Rect::from_center_size(
                rect.right_center() - Vec2::new(size.icon_size() / 2.0 + 8.0, 0.0),
                Vec2::splat(size.icon_size()),
            );
            let mut icon_ui = ui.new_child(egui::UiBuilder::new().max_rect(icon_rect).layout(
                egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
            ));
            icons::chevron_down(&mut icon_ui, size.icon_size(), gray::_400);
        }

        let input_rect = Rect::from_min_max(
            rect.min + Vec2::new(INPUT_PADDING_X, 0.0),
            rect.max - Vec2::new(size.icon_area_width(), 0.0),
        );

        if !is_open {
            if let Some(text) = selected_text.filter(|text| !text.is_empty()) {
                if let Some(is_active) = selected_status {
                    ui.painter().circle_filled(
                        input_rect.left_center() + Vec2::new(5.0, 0.0),
                        5.0,
                        if is_active { SUCCESS } else { gray::_200 },
                    );
                }
                let text_pos = input_rect.left_center()
                    + Vec2::new(if selected_status.is_some() { 20.0 } else { 0.0 }, 0.0);
                let galley =
                    layout_truncated_text(ui, text, size.font(), gray::_900, input_rect.width());
                let is_truncated = galley.elided;
                ui.painter().galley(
                    text_pos - Vec2::new(0.0, galley.size().y / 2.0),
                    galley,
                    gray::_900,
                );
                return if is_truncated {
                    response.on_hover_text(text)
                } else {
                    response
                };
            }
            if let Some(placeholder) = placeholder {
                ui.painter().text(
                    input_rect.left_center(),
                    Align2::LEFT_CENTER,
                    placeholder,
                    size.font(),
                    gray::_400,
                );
            }
            return response;
        }

        let mut input_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(input_rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        let mut text_edit = TextEdit::singleline(&mut state.filter_text)
            .frame(false)
            .desired_width(input_rect.width())
            .vertical_align(egui::Align::Center)
            .id(response.id.with("input"));
        if matches!(size, ComboboxSize::Compact) {
            text_edit = text_edit.font(size.font());
        }
        if let Some(placeholder) = placeholder {
            text_edit = text_edit.hint_text(placeholder);
        }
        let text_response = input_ui.add(text_edit);
        if !text_response.has_focus() {
            text_response.request_focus();
        }
        response | text_response
    }

    fn handle_keyboard(ui: &mut Ui, state: &mut ComboboxState, is_open: bool) -> KeyboardInput {
        if !is_open {
            return KeyboardInput::default();
        }

        let mut keyboard = KeyboardInput::default();
        ui.input_mut(|input| {
            if input.consume_key(Modifiers::NONE, Key::ArrowDown) {
                state.focused_index = state.focused_index.saturating_add(1);
                keyboard.scroll_to_focused = true;
            }
            if input.consume_key(Modifiers::NONE, Key::ArrowUp) {
                state.focused_index = state.focused_index.saturating_sub(1);
                keyboard.scroll_to_focused = true;
            }
            if input.consume_key(Modifiers::NONE, Key::Enter) {
                keyboard.enter_pressed = true;
            }
            if input.consume_key(Modifiers::NONE, Key::Escape) {
                keyboard.close_requested = true;
            }
        });
        keyboard
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;
    use std::cell::RefCell;
    use std::collections::HashSet;
    use std::rc::Rc;

    struct Person {
        name: String,
        id: u32,
        active: bool,
    }

    fn test_people() -> Vec<Person> {
        vec![
            Person {
                name: "Michael Foster".into(),
                id: 1,
                active: true,
            },
            Person {
                name: "Floyd Miles".into(),
                id: 2,
                active: false,
            },
            Person {
                name: "Emily Selman".into(),
                id: 3,
                active: false,
            },
            Person {
                name: "Benjamin Russel".into(),
                id: 4,
                active: true,
            },
        ]
    }

    #[test]
    fn test_combobox_flow() {
        let people = test_people();
        let selected = Rc::new(RefCell::new(HashSet::new()));
        let selected_for_ui = selected.clone();
        let mut harness = Harness::new_ui(move |ui| {
            let mut selected = selected_for_ui.borrow_mut();
            TailwindCombobox::from_label("Assigned to")
                .placeholder("Search...")
                .width(250.0)
                .select_all(selected.len() == people.len())
                .filter_by(|person: &Person| &person.name)
                .show_items(ui, &people, |cb, person| {
                    let is_selected = selected.contains(&person.id);
                    if let Some(action) = cb
                        .item_with_status(&person.name, is_selected, Some(person.active))
                        .selection_action()
                    {
                        match action {
                            SelectionAction::Replace => {
                                selected.clear();
                                selected.insert(person.id);
                            }
                            SelectionAction::Toggle => {
                                if !selected.insert(person.id) {
                                    selected.remove(&person.id);
                                }
                            }
                        }
                    }
                });
        });
        crate::test_support::setup_egui(&harness.ctx);

        harness.run();
        harness.snapshot("comboboxes/closed");

        harness
            .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Assigned to")
            .click();
        harness.run();
        harness.snapshot("comboboxes/open");

        harness
            .input_mut()
            .events
            .push(egui::Event::Text("mi".into()));
        harness.run();
        harness.snapshot("comboboxes/filtered");

        harness.get_by_label("Michael Foster").click();
        harness.run();
        assert_eq!(*selected.borrow(), HashSet::from([1]));

        harness
            .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Assigned to")
            .click();
        harness.run();
        harness.snapshot("comboboxes/selected");
    }

    #[test]
    fn modifier_click_toggles_without_closing() {
        let people = test_people();
        let selected = Rc::new(RefCell::new(HashSet::from([1])));
        let selected_for_ui = selected.clone();
        let select_all_activated = Rc::new(RefCell::new(false));
        let select_all_for_ui = select_all_activated.clone();
        let mut harness = Harness::new_ui(move |ui| {
            let mut selected = selected_for_ui.borrow_mut();
            let response = TailwindCombobox::from_label("People")
                .placeholder("Search...")
                .width(250.0)
                .select_all(selected.len() == people.len())
                .filter_by(|person: &Person| &person.name)
                .show_items(ui, &people, |cb, person| {
                    let is_selected = selected.contains(&person.id);
                    if let Some(action) = cb.item(&person.name, is_selected).selection_action() {
                        match action {
                            SelectionAction::Replace => {
                                selected.clear();
                                selected.insert(person.id);
                            }
                            SelectionAction::Toggle => {
                                if !selected.insert(person.id) {
                                    selected.remove(&person.id);
                                }
                            }
                        }
                    }
                });
            if response.select_all_clicked {
                *select_all_for_ui.borrow_mut() = true;
            }
        });
        crate::test_support::setup_egui(&harness.ctx);
        harness.run();
        harness
            .get_by_role_and_label(egui::accesskit::Role::ComboBox, "People")
            .click();
        harness.run();
        harness.get_by_label("Select all");

        let floyd_position = harness.get_by_label("Floyd Miles").rect().center();
        let modifiers = egui::Modifiers {
            ctrl: true,
            ..Default::default()
        };
        harness.input_mut().modifiers = modifiers;
        harness.event(egui::Event::PointerMoved(floyd_position));
        harness.event(egui::Event::PointerButton {
            pos: floyd_position,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers,
        });
        harness.event(egui::Event::PointerButton {
            pos: floyd_position,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers,
        });
        harness.run();
        harness.input_mut().modifiers = egui::Modifiers::default();
        assert_eq!(*selected.borrow(), HashSet::from([1, 2]));
        harness.get_by_label("Select all");
        assert!(!*select_all_activated.borrow());
    }

    #[test]
    fn select_all_is_hidden_while_searching() {
        let people = test_people();
        let mut harness = Harness::new_ui(move |ui| {
            TailwindCombobox::from_label("People")
                .placeholder("Search...")
                .width(250.0)
                .select_all(false)
                .filter_by(|person: &Person| &person.name)
                .show_items(ui, &people, |cb, person| {
                    cb.item(&person.name, false);
                });
        });
        crate::test_support::setup_egui(&harness.ctx);
        harness.run();
        harness
            .get_by_role_and_label(egui::accesskit::Role::ComboBox, "People")
            .click();
        harness.run();
        harness.run();
        harness
            .input_mut()
            .events
            .push(egui::Event::Text("flo".into()));
        harness.run();

        assert!(harness.query_by_label("Select all").is_none());
        harness.get_by_label("Floyd Miles");
    }

    #[test]
    fn keyboard_navigation_moves_focus_and_selects_the_focused_item() {
        let people = test_people();
        let selected = Rc::new(RefCell::new(HashSet::new()));
        let selected_for_ui = selected.clone();
        let mut harness = Harness::new_ui(move |ui| {
            let mut selected = selected_for_ui.borrow_mut();
            TailwindCombobox::from_label("People")
                .placeholder("Search...")
                .width(250.0)
                .filter_by(|person: &Person| &person.name)
                .show_items(ui, &people, |cb, person| {
                    if let Some(action) = cb
                        .item(&person.name, selected.contains(&person.id))
                        .selection_action()
                    {
                        match action {
                            SelectionAction::Replace => {
                                selected.clear();
                                selected.insert(person.id);
                            }
                            SelectionAction::Toggle => unreachable!("keyboard activation replaces"),
                        }
                    }
                });
        });
        crate::test_support::setup_egui(&harness.ctx);
        harness.run();
        harness
            .get_by_role_and_label(egui::accesskit::Role::ComboBox, "People")
            .click();
        harness.run();

        harness.key_down(Key::ArrowDown);
        harness.run();
        harness.key_down(Key::Enter);
        harness.run();

        assert_eq!(*selected.borrow(), HashSet::from([2]));
        assert!(harness.query_by_label("Michael Foster").is_none());
    }

    #[test]
    fn long_namespace_names_are_truncated_without_hiding_selection_affordances() {
        let namespaces = [
            "namespace-with-a-very-long-name-that-must-not-overlap-the-status-or-checkmark",
            "default",
        ];
        let selected_namespace = namespaces[0];
        let mut harness = Harness::new_ui(move |ui| {
            TailwindCombobox::from_label("Namespaces")
                .selected_text(selected_namespace)
                .selected_status(Some(true))
                .width(230.0)
                .filter_by(|namespace: &&str| *namespace)
                .show_items(ui, &namespaces, |cb, namespace| {
                    cb.item_with_status(*namespace, *namespace == selected_namespace, Some(true));
                });
        });
        crate::test_support::setup_egui(&harness.ctx);
        harness.run();
        harness.snapshot("comboboxes/long_namespace_closed");
        harness
            .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Namespaces")
            .hover();
        harness.run();
        harness.snapshot("comboboxes/long_namespace_closed_tooltip");

        let namespaces = [
            "namespace-with-a-very-long-name-that-must-not-overlap-the-status-or-checkmark",
            "default",
        ];
        let mut open_harness = Harness::new_ui(move |ui| {
            TailwindCombobox::from_label("Namespaces")
                .selected_text(selected_namespace)
                .selected_status(Some(true))
                .width(230.0)
                .filter_by(|namespace: &&str| *namespace)
                .show_items(ui, &namespaces, |cb, namespace| {
                    cb.item_with_status(*namespace, *namespace == selected_namespace, Some(true));
                });
        });
        crate::test_support::setup_egui(&open_harness.ctx);
        open_harness.run();
        open_harness
            .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Namespaces")
            .click();
        open_harness.run();
        open_harness.snapshot("comboboxes/long_namespace_open");
        open_harness.get_by_label(selected_namespace).hover();
        open_harness.run_ok();
        open_harness.snapshot("comboboxes/long_namespace_open_tooltip");
    }

    #[test]
    fn keyboard_navigation_does_not_scroll_a_three_item_result_list() {
        let namespaces = ["system", "staging", "sandbox", "default"];
        let selected = Rc::new(RefCell::new(None));
        let selected_for_ui = selected.clone();
        let mut harness = Harness::new_ui(move |ui| {
            TailwindCombobox::from_label("Namespaces")
                .placeholder("Search namespaces...")
                .width(250.0)
                .filter_by(|namespace: &&str| *namespace)
                .show_items(ui, &namespaces, |cb, namespace| {
                    if cb.item(*namespace, false).clicked() {
                        *selected_for_ui.borrow_mut() = Some(*namespace);
                    }
                });
        });
        crate::test_support::setup_egui(&harness.ctx);
        harness.run();
        harness
            .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Namespaces")
            .click();
        harness.run();
        harness
            .input_mut()
            .events
            .push(egui::Event::Text("s".into()));
        harness.run();

        let system_rect = harness.get_by_label("system").rect();
        let staging_rect = harness.get_by_label("staging").rect();
        let sandbox_rect = harness.get_by_label("sandbox").rect();

        harness.key_down(Key::ArrowDown);
        harness.run();
        harness.key_down(Key::ArrowDown);
        harness.run();

        assert_eq!(harness.get_by_label("system").rect(), system_rect);
        assert_eq!(harness.get_by_label("staging").rect(), staging_rect);
        assert_eq!(harness.get_by_label("sandbox").rect(), sandbox_rect);
        harness.snapshot("comboboxes/three_filtered_results");

        harness.key_down(Key::Enter);
        harness.run();
        assert_eq!(*selected.borrow(), Some("sandbox"));
    }

    #[test]
    fn keyboard_navigation_scrolls_a_focused_item_into_view() {
        let namespaces = (0..20)
            .map(|index| format!("namespace-{index:03}"))
            .collect::<Vec<_>>();
        let selected = Rc::new(RefCell::new(None));
        let selected_for_ui = selected.clone();
        let mut harness = Harness::new_ui(move |ui| {
            TailwindCombobox::from_label("Namespaces")
                .placeholder("Search namespaces...")
                .width(250.0)
                .filter_by(|namespace: &String| namespace)
                .show_items(ui, &namespaces, |cb, namespace| {
                    if cb.item(namespace, false).clicked() {
                        *selected_for_ui.borrow_mut() = Some(namespace.clone());
                    }
                });
        });
        crate::test_support::setup_egui(&harness.ctx);
        harness.run();
        let combobox_rect = harness
            .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Namespaces")
            .rect();
        harness
            .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Namespaces")
            .click();
        harness.run();

        for _ in 0..10 {
            harness.key_down(Key::ArrowDown);
            harness.run();
        }

        let focused_rect = harness.get_by_label("namespace-010").rect();
        assert!(focused_rect.top() >= combobox_rect.bottom());
        assert!(focused_rect.bottom() <= combobox_rect.bottom() + DROPDOWN_MAX_HEIGHT);
        harness.snapshot("comboboxes/keyboard_scroll_into_view");

        harness.key_down(Key::Enter);
        harness.run();
        assert_eq!(selected.borrow().as_deref(), Some("namespace-010"));
    }

    #[test]
    fn dropdown_expands_after_backspacing_to_more_search_results() {
        let namespaces = ["system", "staging", "sandbox", "services"];
        let mut harness = Harness::new_ui(move |ui| {
            TailwindCombobox::from_label("Namespaces")
                .placeholder("Search namespaces...")
                .width(250.0)
                .filter_by(|namespace: &&str| *namespace)
                .show_items(ui, &namespaces, |cb, namespace| {
                    cb.item(*namespace, false);
                });
        });
        crate::test_support::setup_egui(&harness.ctx);
        harness.run();
        harness
            .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Namespaces")
            .click();
        harness.run();
        harness
            .input_mut()
            .events
            .push(egui::Event::Text("sy".into()));
        harness.run();
        harness.get_by_label("system");

        harness.key_down(Key::Backspace);
        harness.run();
        harness.get_by_label("staging");
        harness.get_by_label("sandbox");
        harness.snapshot("comboboxes/filter_expands");
    }

    #[test]
    fn dropdown_with_two_hundred_items_is_capped_to_a_scrollable_height() {
        let namespaces = (0..200)
            .map(|index| format!("namespace-{index:03}"))
            .collect::<Vec<_>>();
        let mut harness = Harness::new_ui(move |ui| {
            TailwindCombobox::from_label("Namespaces")
                .placeholder("Search namespaces...")
                .width(250.0)
                .filter_by(|namespace: &String| namespace)
                .show_items(ui, &namespaces, |cb, namespace| {
                    cb.item(namespace, false);
                });
        });
        crate::test_support::setup_egui(&harness.ctx);
        harness.run();
        harness
            .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Namespaces")
            .click();
        harness.run();
        harness.snapshot("comboboxes/two_hundred_items");
    }
}
