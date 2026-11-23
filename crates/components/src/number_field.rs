//! The whole-number input with steppers (C04 `NumberField`).

use std::hash::Hash;

use egui::{CornerRadius, Id, Key, Modifiers, Response, Ui};

use crate::button::Button;
use crate::design::{radius, size};
use crate::icons::Icon;
use crate::interaction::ControlState;
use crate::text_input::TextInput;

/// How far Shift+↑/↓ steps.
const LARGE_STEP: i64 = 10;

/// A whole-number input joined to − and + stepper buttons (`.inp` with two
/// `.btn-secondary.icon`, sharing their edges).
///
/// The value is edited as text, so the caller can say why an entry is
/// invalid. The steppers and ↑/↓ (Shift: ×10) step from the parsed value,
/// or from [`Self::fallback`] while it does not parse, and clamp to
/// [`Self::range`]. Show the current value and bounds in the field's help
/// line.
#[must_use = "call `show` to add the number field"]
pub struct NumberField<'a> {
    text: &'a mut String,
    id: Option<Id>,
    min: i64,
    max: i64,
    fallback: i64,
    input_width: f32,
    accessibility_label: String,
    decrease_label: String,
    increase_label: String,
    invalid: bool,
    enabled: bool,
    forced_state: Option<ControlState>,
}

/// What a [`NumberField`] did this frame.
pub struct NumberFieldResponse {
    /// The text editor's response.
    pub response: Response,
    /// Whether a stepper or arrow key changed the value.
    pub stepped: bool,
}

impl NumberField<'_> {
    /// A number field editing `text`.
    pub fn new(text: &mut String) -> NumberField<'_> {
        NumberField {
            text,
            id: None,
            min: i64::MIN,
            max: i64::MAX,
            fallback: 0,
            input_width: size::NUMBER_FIELD_INPUT,
            accessibility_label: "Number".to_owned(),
            decrease_label: "Decrease".to_owned(),
            increase_label: "Increase".to_owned(),
            invalid: false,
            enabled: true,
            forced_state: None,
        }
    }
}

impl<'a> NumberField<'a> {
    /// A stable ID for the editor's state.
    pub fn id_salt(mut self, id_salt: impl Hash + std::fmt::Debug) -> Self {
        self.id = Some(Id::new(id_salt));
        self
    }

    /// The bounds stepping clamps to.
    pub fn range(mut self, range: std::ops::RangeInclusive<i64>) -> Self {
        self.min = *range.start();
        self.max = *range.end();
        self
    }

    /// The value stepping starts from while the text is not a number.
    pub fn fallback(mut self, fallback: i64) -> Self {
        self.fallback = fallback;
        self
    }

    /// The width of the text part (72 points by default).
    pub fn input_width(mut self, width: f32) -> Self {
        self.input_width = width;
        self
    }

    /// The accessible names of the input and its two steppers.
    pub fn accessibility_labels(
        mut self,
        input: impl Into<String>,
        decrease: impl Into<String>,
        increase: impl Into<String>,
    ) -> Self {
        self.accessibility_label = input.into();
        self.decrease_label = decrease.into();
        self.increase_label = increase.into();
        self
    }

    /// Mark the entry invalid: the danger ring.
    pub fn invalid(mut self, invalid: bool) -> Self {
        self.invalid = invalid;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Paint the input in `state` (design boards).
    pub fn force_state(mut self, state: ControlState) -> Self {
        self.forced_state = Some(state);
        self
    }

    /// Add the field.
    pub fn show(self, ui: &mut Ui) -> NumberFieldResponse {
        let Self {
            text,
            id,
            min,
            max,
            fallback,
            input_width,
            accessibility_label,
            decrease_label,
            increase_label,
            invalid,
            enabled,
            forced_state,
        } = self;
        let parsed = text.trim().parse::<i64>().ok();
        let current = parsed.unwrap_or(fallback);
        let full = radius::FULL;
        ui.horizontal(|ui| {
            // The design joins the three by overlapping their 1 point (transparent)
            // borders; laid out side by side instead, the input gives up those
            // two points so the group keeps its width.
            ui.spacing_mut().item_spacing.x = 0.0;
            let mut input = TextInput::new(&mut *text)
                .width(input_width - 2.0 * size::HAIRLINE)
                .accessibility_label(accessibility_label)
                .invalid(invalid)
                .enabled(enabled)
                .corner_radius(CornerRadius {
                    nw: full,
                    sw: full,
                    ne: 0,
                    se: 0,
                });
            if let Some(id) = id {
                input = input.id_salt(id);
            }
            if let Some(state) = forced_state {
                input = input.force_state(state);
            }
            let response = input.show(ui);

            let mut step = 0;
            if response.has_focus() {
                ui.input_mut(|input| {
                    // Shift first: a key without modifiers also matches it with Shift.
                    for (modifiers, amount) in
                        [(Modifiers::SHIFT, LARGE_STEP), (Modifiers::NONE, 1)]
                    {
                        if input.consume_key(modifiers, Key::ArrowUp) {
                            step += amount;
                        }
                        if input.consume_key(modifiers, Key::ArrowDown) {
                            step -= amount;
                        }
                    }
                });
            }
            let stepper = |icon, label: String, corner_radius: CornerRadius, usable: bool| {
                Button::icon(icon, label)
                    .corner_radius(corner_radius)
                    .enabled(enabled && usable)
            };
            if stepper(
                Icon::Minus,
                decrease_label,
                CornerRadius::ZERO,
                current > min,
            )
            .show(ui)
            .clicked()
            {
                step -= 1;
            }
            if stepper(
                Icon::Plus,
                increase_label,
                CornerRadius {
                    nw: 0,
                    sw: 0,
                    ne: full,
                    se: full,
                },
                current < max,
            )
            .show(ui)
            .clicked()
            {
                step += 1;
            }

            let stepped = step != 0;
            if stepped {
                *text = current.saturating_add(step).clamp(min, max).to_string();
            }
            NumberFieldResponse { response, stepped }
        })
        .inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{UiHarnessSnapshot, setup_egui};
    use egui_kittest::Harness;
    use egui_kittest::kittest::{NodeT, Queryable};
    use std::cell::RefCell;
    use std::rc::Rc;

    fn replicas_harness(text: Rc<RefCell<String>>) -> Harness<'static> {
        ranged_harness(text, 0..=5, Rc::default())
    }

    /// A number field over `range` that counts the frames it stepped in.
    fn ranged_harness(
        text: Rc<RefCell<String>>,
        range: std::ops::RangeInclusive<i64>,
        steps: Rc<std::cell::Cell<usize>>,
    ) -> Harness<'static> {
        let mut harness = Harness::builder()
            .with_size(egui::vec2(200.0, 48.0))
            .build_ui(move |ui| {
                let response = NumberField::new(&mut text.borrow_mut())
                    .range(range.clone())
                    .fallback(2)
                    .accessibility_labels("Replicas", "Decrease replicas", "Increase replicas")
                    .show(ui);
                steps.set(steps.get() + usize::from(response.stepped));
            });
        setup_egui(&mut harness);
        harness.run();
        harness
    }

    #[test]
    fn steppers_step_and_clamp_to_the_range() {
        let text = Rc::new(RefCell::new("4".to_owned()));
        let mut harness = replicas_harness(text.clone());
        harness.ui_harness("number_field/replicas");

        harness.get_by_label("Increase replicas").click();
        harness.run();
        assert_eq!(*text.borrow(), "5");
        assert!(
            harness
                .get_by_label("Increase replicas")
                .accesskit_node()
                .is_disabled(),
            "the maximum disables the + stepper"
        );
        harness.get_by_label("Decrease replicas").click();
        harness.run();
        assert_eq!(*text.borrow(), "4");
    }

    #[test]
    fn arrow_keys_step_and_shift_steps_by_ten() {
        let text = Rc::new(RefCell::new("3".to_owned()));
        let steps = Rc::new(std::cell::Cell::new(0));
        let mut harness = ranged_harness(text.clone(), 0..=100, steps.clone());
        harness.get_by_label("Replicas").focus();
        harness.run();
        harness.key_press(Key::ArrowDown);
        harness.run();
        assert_eq!(*text.borrow(), "2");
        harness.key_press_modifiers(Modifiers::SHIFT, Key::ArrowUp);
        harness.run();
        assert_eq!(*text.borrow(), "12");
        harness.key_press_modifiers(Modifiers::SHIFT, Key::ArrowDown);
        harness.run();
        assert_eq!(*text.borrow(), "2");
        assert_eq!(steps.get(), 3);
    }

    #[test]
    fn out_of_range_values_step_back_into_the_range() {
        let text = Rc::new(RefCell::new("12".to_owned()));
        let mut harness = replicas_harness(text.clone());
        assert!(
            harness
                .get_by_label("Increase replicas")
                .accesskit_node()
                .is_disabled()
        );
        harness.get_by_label("Decrease replicas").click();
        harness.run();
        assert_eq!(*text.borrow(), "5", "stepping clamps to the maximum");

        *text.borrow_mut() = "-3".to_owned();
        harness.run();
        assert!(
            harness
                .get_by_label("Decrease replicas")
                .accesskit_node()
                .is_disabled()
        );
        harness.get_by_label("Increase replicas").click();
        harness.run();
        assert_eq!(*text.borrow(), "0", "stepping clamps to the minimum");
    }

    #[test]
    fn stepping_an_unparsable_value_starts_from_the_fallback() {
        let text = Rc::new(RefCell::new("many".to_owned()));
        let mut harness = replicas_harness(text.clone());
        harness.get_by_label("Increase replicas").click();
        harness.run();
        assert_eq!(*text.borrow(), "3");
    }
}
