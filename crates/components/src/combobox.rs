//! Tailwind-styled filterable combobox component for egui
//!
//! A text input with dropdown that filters options as the user types.
//!
//! # Example
//!
//! ```ignore
//! TailwindCombobox::from_label("Assigned to")
//!     .placeholder("Search...")
//!     .filter_by(|person| &person.name)
//!     .show_items(ui, &people, |cb, person| {
//!         let is_selected = selected.contains(&person.id);
//!         if cb.item(&person.name, is_selected).clicked() {
//!             selected.insert(person.id);
//!         }
//!     });
//! ```

// Layout constants
const ITEM_HEIGHT: f32 = 36.0;
const INPUT_HEIGHT: f32 = 40.0;
const CORNER_RADIUS: u8 = 6;
const FOCUS_RING_RADIUS: u8 = 8;
const FOCUS_RING_WIDTH: f32 = 2.0;
const ICON_SIZE: f32 = 20.0;
const DROPDOWN_MAX_HEIGHT: f32 = 300.0;
const DEFAULT_WIDTH: f32 = 256.0;
const ITEM_PADDING_X: f32 = 12.0;
const INPUT_PADDING_X: f32 = 12.0;
const ICON_AREA_WIDTH: f32 = 36.0;
const FONT_SIZE: f32 = 14.0;

use egui::{
    Align2, Color32, CornerRadius, FontId, Id, InnerResponse, Key, Popup, PopupCloseBehavior,
    Rect, Response, ScrollArea, Sense, Stroke, StrokeKind, TextEdit, Ui, Vec2, WidgetText,
};

use crate::colors::{gray, indigo, WHITE};
use crate::fuzzy::{matches_fuzzy, normalize_for_search};
use crate::icons;

/// Internal state persisted across frames via egui memory
#[derive(Default, Clone)]
struct ComboboxState {
    filter_text: String,
    /// Pre-computed normalized filter chars for efficient matching.
    /// Uses NFKD normalization + accent stripping + case folding.
    filter_chars: Vec<char>,
    focused_index: usize,
}

/// Response from a combobox item
///
/// Wraps egui's Response and adds keyboard selection support.
pub struct ItemResponse {
    response: Response,
    keyboard_selected: bool,
}

impl ItemResponse {
    /// Returns true if the item was clicked (mouse) or selected via keyboard (Enter key)
    pub fn clicked(&self) -> bool {
        self.response.clicked() || self.keyboard_selected
    }

    /// Returns the underlying egui Response
    pub fn response(&self) -> &Response {
        &self.response
    }
}

/// Context passed to the render callback for each filtered item
pub struct ComboboxUi<'a> {
    ui: &'a mut Ui,
    focused_index: usize,
    current_index: usize,
    popup_id: Id,
    enter_pressed: bool,
}

impl<'a> ComboboxUi<'a> {
    /// Render a styled item row
    ///
    /// Returns an ItemResponse that can be checked for `.clicked()` to handle selection.
    /// The `is_selected` parameter controls the selected visual state (indigo background).
    ///
    /// # Important
    ///
    /// Items must be rendered in the same order they were filtered. This method
    /// internally tracks item indices for keyboard navigation - calling it out of
    /// order or skipping items will cause incorrect focus behavior.
    pub fn item(&mut self, label: impl Into<WidgetText>, is_selected: bool) -> ItemResponse {
        let label = label.into();
        let is_focused = self.current_index == self.focused_index;
        self.current_index += 1;

        // Allocate space for the item
        let available_width = self.ui.available_width();
        let (rect, response) =
            self.ui
                .allocate_exact_size(Vec2::new(available_width, ITEM_HEIGHT), Sense::click());

        // Check if this item was selected via keyboard
        let keyboard_selected = is_focused && self.enter_pressed;

        if self.ui.is_rect_visible(rect) {
            // Determine colors based on state
            let (bg_color, text_color) = if is_selected {
                (indigo::_50, indigo::_600)
            } else if is_focused {
                (gray::_100, gray::_900)
            } else if response.hovered() {
                (gray::_50, gray::_900)
            } else {
                (Color32::TRANSPARENT, gray::_900)
            };

            // Paint background
            if bg_color != Color32::TRANSPARENT {
                self.ui.painter().rect_filled(rect, 0.0, bg_color);
            }

            // Paint text
            let text_pos = rect.left_center() + Vec2::new(ITEM_PADDING_X, 0.0);
            self.ui.painter().text(
                text_pos,
                Align2::LEFT_CENTER,
                label.text(),
                FontId::proportional(FONT_SIZE),
                text_color,
            );
        }

        ItemResponse {
            response,
            keyboard_selected,
        }
    }

    /// Close the dropdown programmatically
    pub fn close(&self) {
        Popup::close_id(self.ui.ctx(), self.popup_id);
    }
}

/// Marker type for combobox without a filter function set
pub struct NoFilter;

/// Wrapper type indicating filter function is configured
pub struct WithFilter<F>(F);

/// A Tailwind-styled filterable combobox
///
/// Generic over `Filter` which represents the filter configuration state.
/// Use [`NoFilter`] for unconfigured state, [`WithFilter<F>`] after calling `filter_by`.
pub struct TailwindCombobox<Filter> {
    id_salt: Id,
    label: Option<WidgetText>,
    placeholder: Option<String>,
    width: Option<f32>,
    filter: Filter,
}

impl TailwindCombobox<NoFilter> {
    /// Create a new combobox with an explicit ID
    pub fn new(id_salt: impl std::hash::Hash) -> Self {
        TailwindCombobox {
            id_salt: Id::new(id_salt),
            label: None,
            placeholder: None,
            width: None,
            filter: NoFilter,
        }
    }

    /// Create a new combobox with the label as the ID source
    pub fn from_label(label: impl Into<WidgetText>) -> Self {
        let label = label.into();
        TailwindCombobox {
            id_salt: Id::new(label.text()),
            label: Some(label),
            placeholder: None,
            width: None,
            filter: NoFilter,
        }
    }

    /// Set the filter function that extracts searchable text from each item
    ///
    /// The component performs fuzzy subsequence matching: characters must appear
    /// in order but not consecutively. Uses Unicode NFKD normalization for
    /// accent-insensitive, case-insensitive matching.
    ///
    /// Examples:
    /// - "mf" matches "Michael Foster"
    /// - "cafe" matches "Café"
    /// - "fobr" matches "foobar"
    pub fn filter_by<T, F>(self, filter_fn: F) -> TailwindCombobox<WithFilter<F>>
    where
        F: Fn(&T) -> &str,
    {
        TailwindCombobox {
            id_salt: self.id_salt,
            label: self.label,
            placeholder: self.placeholder,
            width: self.width,
            filter: WithFilter(filter_fn),
        }
    }
}

impl<Filter> TailwindCombobox<Filter> {
    /// Set the placeholder text shown when the input is empty
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    /// Set the width of the combobox (clamped to non-negative)
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width.max(0.0));
        self
    }
}

impl<F> TailwindCombobox<WithFilter<F>> {
    /// Show the combobox with the given items
    ///
    /// The `render` callback is called once for each item that matches the filter.
    /// Use `cb.item(label, is_selected)` to render each row.
    pub fn show_items<'a, T, I, R>(
        self,
        ui: &mut Ui,
        items: I,
        mut render: R,
    ) -> InnerResponse<()>
    where
        F: Fn(&T) -> &str,
        I: IntoIterator<Item = &'a T>,
        T: 'a,
        R: FnMut(&mut ComboboxUi<'_>, &T),
    {
        // Destructure self to avoid partial move issues
        let TailwindCombobox {
            id_salt,
            label,
            placeholder,
            width,
            filter: WithFilter(filter_fn),
        } = self;

        let state_id = ui.make_persistent_id(id_salt);

        // Load state from memory
        let mut state = ui
            .ctx()
            .memory_mut(|mem| mem.data.get_temp::<ComboboxState>(state_id).unwrap_or_default());

        // Render label if present
        if let Some(label) = &label {
            ui.label(label.clone());
        }

        // Render the input field with chevron
        let width = width.unwrap_or(DEFAULT_WIDTH);
        // First pass: render without knowing is_open state (affects focus ring only)
        let input_response =
            Self::render_input(ui, &mut state, width, false, placeholder.as_deref());

        // Get popup_id from the rendered response
        let popup_id = Popup::default_response_id(&input_response);
        let is_open = Popup::is_id_open(ui.ctx(), popup_id);

        // Handle keyboard navigation
        let enter_pressed = Self::handle_keyboard(ui, &mut state, is_open, popup_id);

        // Update normalized filter chars for fuzzy matching
        state.filter_chars = normalize_for_search(&state.filter_text).collect();

        // Collect filtered items using fuzzy subsequence matching
        let filtered_items: Vec<_> = items
            .into_iter()
            .filter(|item| matches_fuzzy(filter_fn(item), &state.filter_chars))
            .collect();

        // Clamp focused index to valid range
        let item_count = filtered_items.len();
        if item_count > 0 {
            state.focused_index = state.focused_index.min(item_count.saturating_sub(1));
        }

        // Store state once after all modifications
        ui.ctx()
            .memory_mut(|mem| mem.data.insert_temp(state_id, state.clone()));

        // Show popup (uses default ID derived from input_response.id)
        let _inner = Popup::menu(&input_response)
            .width(width)
            .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                ScrollArea::vertical().max_height(DROPDOWN_MAX_HEIGHT).show(ui, |ui| {
                    let mut cb = ComboboxUi {
                        ui,
                        focused_index: state.focused_index,
                        current_index: 0,
                        popup_id,
                        enter_pressed,
                    };

                    for item in &filtered_items {
                        render(&mut cb, item);
                    }
                });
            });

        InnerResponse {
            inner: (),
            response: input_response,
        }
    }

    /// Render the styled input field with chevron icon
    fn render_input(
        ui: &mut Ui,
        state: &mut ComboboxState,
        width: f32,
        is_open: bool,
        placeholder: Option<&str>,
    ) -> Response {
        let corner_radius = CornerRadius::same(CORNER_RADIUS);

        // Allocate the full input area
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, INPUT_HEIGHT), Sense::click());

        let has_focus = response.has_focus()
            || ui.memory(|m| m.has_focus(response.id.with("input")))
            || is_open;

        if ui.is_rect_visible(rect) {
            // Background
            ui.painter().rect_filled(rect, corner_radius, WHITE);

            // Border
            let border_color = if has_focus { indigo::_500 } else { gray::_300 };
            ui.painter().rect_stroke(
                rect,
                corner_radius,
                Stroke::new(1.0, border_color),
                StrokeKind::Inside,
            );

            // Focus ring (outer glow)
            if has_focus {
                let focus_rect = rect.expand(FOCUS_RING_WIDTH);
                ui.painter().rect_stroke(
                    focus_rect,
                    CornerRadius::same(FOCUS_RING_RADIUS),
                    Stroke::new(FOCUS_RING_WIDTH, indigo::_500.gamma_multiply(0.5)),
                    StrokeKind::Outside,
                );
            }

            // Chevron icon on the right
            let icon_rect = Rect::from_center_size(
                rect.right_center() - Vec2::new(ICON_SIZE / 2.0 + 8.0, 0.0),
                Vec2::splat(ICON_SIZE),
            );

            // Create a child UI for the icon
            let mut icon_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(icon_rect)
                    .layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight)),
            );
            icons::chevron_down(&mut icon_ui, ICON_SIZE, gray::_400);
        }

        // Text input area (inside the rect)
        let input_rect = Rect::from_min_max(
            rect.min + Vec2::new(INPUT_PADDING_X, 0.0),
            rect.max - Vec2::new(ICON_AREA_WIDTH, 0.0), // Leave space for chevron
        );

        let mut input_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(input_rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );

        // Style the text edit to be invisible (we draw the frame ourselves)
        let mut text_edit = TextEdit::singleline(&mut state.filter_text)
            .frame(false)
            .desired_width(input_rect.width())
            .vertical_align(egui::Align::Center)
            .id(response.id.with("input"));

        if let Some(placeholder) = placeholder {
            text_edit = text_edit.hint_text(placeholder);
        }

        let text_response = input_ui.add(text_edit);

        // Merge responses - Popup::menu will handle opening based on click state
        response | text_response
    }

    /// Handle keyboard navigation
    fn handle_keyboard(ui: &mut Ui, state: &mut ComboboxState, is_open: bool, popup_id: Id) -> bool {
        if !is_open {
            return false;
        }

        let mut enter_pressed = false;

        ui.input(|i| {
            if i.key_pressed(Key::ArrowDown) {
                state.focused_index = state.focused_index.saturating_add(1);
            }
            if i.key_pressed(Key::ArrowUp) {
                state.focused_index = state.focused_index.saturating_sub(1);
            }
            if i.key_pressed(Key::Enter) {
                enter_pressed = true;
            }
            if i.key_pressed(Key::Escape) {
                Popup::close_id(ui.ctx(), popup_id);
            }
        });

        enter_pressed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use std::cell::RefCell;
    use std::collections::HashSet;
    use std::rc::Rc;

    struct Person {
        name: String,
        id: u32,
    }

    fn test_people() -> Vec<Person> {
        vec![
            Person {
                name: "Michael Foster".to_string(),
                id: 1,
            },
            Person {
                name: "Floyd Miles".to_string(),
                id: 2,
            },
            Person {
                name: "Emily Selman".to_string(),
                id: 3,
            },
            Person {
                name: "Benjamin Russel".to_string(),
                id: 4,
            },
        ]
    }

    /// Helper to simulate a click at a position
    fn click_at(harness: &mut Harness<'_>, pos: egui::Pos2) {
        harness
            .input_mut()
            .events
            .push(egui::Event::PointerMoved(pos));
        harness.input_mut().events.push(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        });
        harness.input_mut().events.push(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::default(),
        });
    }

    #[test]
    fn test_combobox_flow() {
        let people = test_people();
        // Use Rc<RefCell> so the closure can mutate selection state
        let selected: Rc<RefCell<HashSet<u32>>> = Rc::new(RefCell::new(HashSet::new()));
        let selected_clone = selected.clone();

        let mut harness = Harness::new_ui(move |ui| {
            let mut selected = selected_clone.borrow_mut();
            TailwindCombobox::from_label("Assigned to")
                .placeholder("Search...")
                .width(250.0)
                .filter_by(|p: &Person| &p.name)
                .show_items(ui, &people, |cb, person| {
                    let is_selected = selected.contains(&person.id);
                    if cb.item(&person.name, is_selected).clicked() {
                        if is_selected {
                            selected.remove(&person.id);
                        } else {
                            selected.insert(person.id);
                        }
                    }
                });
        });

        // Install image loaders for SVG icons
        egui_extras::install_image_loaders(&harness.ctx);

        // 1. Initial closed state
        harness.run();
        harness.snapshot("combobox_closed");

        // 2. Click to open dropdown
        let input_pos = egui::pos2(125.0, 40.0);
        click_at(&mut harness, input_pos);
        harness.run();
        harness.snapshot("combobox_open");

        // 3. Type "mi" to filter (shows Michael Foster, Emily Selman)
        harness
            .input_mut()
            .events
            .push(egui::Event::Text("mi".into()));
        harness.run();
        harness.snapshot("combobox_filtered");

        // 4. Click on first filtered item (Michael Foster) to select it
        // Dropdown items start below input (~60px), each item is 36px tall
        let first_item_pos = egui::pos2(125.0, 80.0);
        click_at(&mut harness, first_item_pos);
        harness.run();

        // Verify selection was toggled
        assert!(
            selected.borrow().contains(&1),
            "Michael Foster (id=1) should be selected"
        );

        // 5. Re-open to show selected state
        click_at(&mut harness, input_pos);
        harness.run();
        harness.snapshot("combobox_selected");
    }
}
