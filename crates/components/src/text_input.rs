use std::{fmt::Debug, hash::Hash};

use egui::{self, Color32, Id, Margin, Stroke};

use crate::design::theme::{Theme, ThemeExt};
use crate::design::{radius, space, stroke, type_scale};

/// The 1 point outline of a text field filled with `fill` (`.inp`).
///
/// Harbour fields have no visible border at rest, so the outline matches the
/// fill and only keeps the control's footprint. Focus draws the accent ring
/// and an invalid value the danger ring.
pub(crate) fn field_stroke(theme: &Theme, fill: Color32, focused: bool, invalid: bool) -> Stroke {
    let color = if invalid {
        theme.danger.fill
    } else if focused {
        theme.accent.fill
    } else {
        fill
    };
    stroke::hairline(color)
}

/// A single-line text input styled like the application's standard controls.
pub struct TailwindTextInput<'a> {
    value: &'a mut String,
    id: Option<Id>,
    hint_text: Option<String>,
    accessibility_label: String,
}

impl<'a> TailwindTextInput<'a> {
    pub fn new(value: &'a mut String) -> Self {
        Self {
            value,
            id: None,
            hint_text: None,
            accessibility_label: "Text input".to_owned(),
        }
    }

    pub fn id_salt(mut self, id_salt: impl Hash + Debug) -> Self {
        self.id = Some(Id::new(id_salt));
        self
    }

    pub fn hint_text(mut self, hint_text: impl Into<String>) -> Self {
        self.hint_text = Some(hint_text.into());
        self
    }

    pub fn accessibility_label(mut self, label: impl Into<String>) -> Self {
        self.accessibility_label = label.into();
        self
    }

    pub fn show(self, ui: &mut egui::Ui) -> egui::Response {
        let id = self.id.unwrap_or_else(|| ui.next_auto_id());
        let focused = ui.memory(|memory| memory.has_focus(id));
        let tokens = ui.tokens();
        let fill = tokens.components.field_bg;
        let stroke = field_stroke(&tokens, fill, focused, false);
        let accessibility_label = self.accessibility_label;
        let mut text_edit = egui::TextEdit::singleline(self.value)
            .id(id)
            .frame(
                egui::Frame::new()
                    .fill(fill)
                    .stroke(stroke)
                    .corner_radius(egui::CornerRadius::same(radius::KEY))
                    .inner_margin(Margin::symmetric(space::S8 as i8, space::S4 as i8)),
            )
            .font(type_scale::BODY.font_id(ui.ctx()))
            .text_color(tokens.text.primary);
        if let Some(hint_text) = self.hint_text {
            text_edit = text_edit.hint_text(hint_text);
        }
        let response = ui.add_sized(egui::vec2(ui.available_width(), 28.0), text_edit);
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::TextEdit,
                ui.is_enabled(),
                accessibility_label.clone(),
            )
        });
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::UiHarnessSnapshot;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    #[test]
    fn snapshots_empty_filled_and_focused_inputs() {
        let mut empty_value = String::new();
        let mut filled_value = "Application".to_owned();
        let mut harness = Harness::new_ui(|ui| {
            ui.set_width(320.0);
            TailwindTextInput::new(&mut empty_value)
                .id_salt("empty-input")
                .hint_text("Enter a metadata key")
                .accessibility_label("Metadata key")
                .show(ui);
            ui.add_space(space::S12);
            TailwindTextInput::new(&mut filled_value)
                .id_salt("filled-input")
                .accessibility_label("Column header")
                .show(ui);
        });
        crate::test_support::setup_egui(&mut harness);

        harness.run();
        harness.ui_harness("text_inputs/snapshots_empty_filled_and_focused_inputs/unfocused");

        harness.get_by_label("Metadata key").focus();
        harness.run();
        assert!(harness.get_by_label("Metadata key").is_focused());
        harness.ui_harness("text_inputs/snapshots_empty_filled_and_focused_inputs/focused");
    }
}
