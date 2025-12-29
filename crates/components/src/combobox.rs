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

use egui::{
    Align2, Color32, CornerRadius, FontId, Id, InnerResponse, Key, Popup, PopupCloseBehavior,
    Rect, Response, ScrollArea, Sense, Stroke, StrokeKind, TextEdit, Ui, Vec2, WidgetText,
};

use crate::colors::{gray, indigo, WHITE};
use crate::icons;

/// Internal state persisted across frames via egui memory
#[derive(Default, Clone)]
struct ComboboxState {
    filter_text: String,
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
    pub fn item(&mut self, label: impl Into<WidgetText>, is_selected: bool) -> ItemResponse {
        let label = label.into();
        let is_focused = self.current_index == self.focused_index;
        self.current_index += 1;

        // Allocate space for the item
        let item_height = 36.0;
        let available_width = self.ui.available_width();
        let (rect, response) =
            self.ui
                .allocate_exact_size(Vec2::new(available_width, item_height), Sense::click());

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
            let text_pos = rect.left_center() + Vec2::new(12.0, 0.0);
            self.ui.painter().text(
                text_pos,
                Align2::LEFT_CENTER,
                label.text(),
                FontId::proportional(14.0),
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

/// A Tailwind-styled filterable combobox
///
/// Generic over `F` which is the filter function type.
pub struct TailwindCombobox<F> {
    id_salt: Id,
    label: Option<WidgetText>,
    placeholder: Option<String>,
    width: Option<f32>,
    filter_fn: Option<F>,
}

impl TailwindCombobox<fn(&()) -> &str> {
    /// Create a new combobox with an explicit ID
    pub fn new(id_salt: impl std::hash::Hash) -> TailwindCombobox<fn(&()) -> &str> {
        TailwindCombobox {
            id_salt: Id::new(id_salt),
            label: None,
            placeholder: None,
            width: None,
            filter_fn: None,
        }
    }

    /// Create a new combobox with the label as the ID source
    pub fn from_label(label: impl Into<WidgetText>) -> TailwindCombobox<fn(&()) -> &str> {
        let label = label.into();
        TailwindCombobox {
            id_salt: Id::new(label.text()),
            label: Some(label),
            placeholder: None,
            width: None,
            filter_fn: None,
        }
    }
}

impl<F> TailwindCombobox<F> {
    /// Set the placeholder text shown when the input is empty
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    /// Set the width of the combobox
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// Set the filter function that extracts searchable text from each item
    ///
    /// The component will perform case-insensitive substring matching.
    pub fn filter_by<T, F2>(self, filter_fn: F2) -> TailwindCombobox<F2>
    where
        F2: Fn(&T) -> &str,
    {
        TailwindCombobox {
            id_salt: self.id_salt,
            label: self.label,
            placeholder: self.placeholder,
            width: self.width,
            filter_fn: Some(filter_fn),
        }
    }
}

impl<F> TailwindCombobox<F> {
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
            filter_fn,
        } = self;

        let filter_fn = filter_fn.expect("filter_by() must be called before show_items()");

        let id = ui.make_persistent_id(id_salt);
        let popup_id = id.with("popup");

        // Load state from memory
        let mut state = ui
            .ctx()
            .memory_mut(|mem| mem.data.get_temp::<ComboboxState>(id).unwrap_or_default());

        let is_open = Popup::is_id_open(ui.ctx(), popup_id);

        // Render label if present
        if let Some(label) = &label {
            ui.label(label.clone());
        }

        // Render the input field with chevron
        let width = width.unwrap_or(256.0);
        let input_response =
            Self::render_input_static(ui, &mut state, width, is_open, placeholder.as_deref());

        // Handle keyboard navigation
        let enter_pressed = Self::handle_keyboard_static(ui, &mut state, is_open);

        // Store updated state
        ui.ctx()
            .memory_mut(|mem| mem.data.insert_temp(id, state.clone()));

        // Collect filtered items first to know the count
        let filter_lower = state.filter_text.to_lowercase();
        let filtered_items: Vec<_> = items
            .into_iter()
            .filter(|item| {
                if filter_lower.is_empty() {
                    true
                } else {
                    filter_fn(item).to_lowercase().contains(&filter_lower)
                }
            })
            .collect();

        // Clamp focused index to valid range
        let item_count = filtered_items.len();
        if item_count > 0 {
            state.focused_index = state.focused_index.min(item_count.saturating_sub(1));
            // Update stored state with clamped value
            ui.ctx()
                .memory_mut(|mem| mem.data.insert_temp(id, state.clone()));
        }

        // Show popup
        let _inner = Popup::menu(&input_response)
            .id(popup_id)
            .width(width)
            .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
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

    /// Render the styled input field with chevron icon (static version)
    fn render_input_static(
        ui: &mut Ui,
        state: &mut ComboboxState,
        width: f32,
        is_open: bool,
        placeholder: Option<&str>,
    ) -> Response {
        let height = 40.0;
        let corner_radius = CornerRadius::same(6);

        // Allocate the full input area
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());

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
                let focus_rect = rect.expand(2.0);
                ui.painter().rect_stroke(
                    focus_rect,
                    CornerRadius::same(8),
                    Stroke::new(2.0, indigo::_500.gamma_multiply(0.5)),
                    StrokeKind::Outside,
                );
            }

            // Chevron icon on the right
            let icon_size = 20.0;
            let icon_rect = Rect::from_center_size(
                rect.right_center() - Vec2::new(icon_size / 2.0 + 8.0, 0.0),
                Vec2::splat(icon_size),
            );

            // Create a child UI for the icon
            let mut icon_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(icon_rect)
                    .layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight)),
            );
            icons::chevron_down(&mut icon_ui, icon_size, gray::_400);
        }

        // Text input area (inside the rect)
        let input_rect = Rect::from_min_max(
            rect.min + Vec2::new(12.0, 0.0),
            rect.max - Vec2::new(36.0, 0.0), // Leave space for chevron
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

        // Open popup when input gains focus or is clicked
        if text_response.gained_focus() || response.clicked() {
            Popup::open_id(ui.ctx(), response.id.with("popup"));
        }

        // Return the outer response but merge with text response for focus tracking
        response | text_response
    }

    /// Handle keyboard navigation (static version)
    fn handle_keyboard_static(ui: &mut Ui, state: &mut ComboboxState, is_open: bool) -> bool {
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
