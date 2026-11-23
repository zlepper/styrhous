//! Tailwind-inspired multiline text input for editable resource content.

use std::hash::Hash;

use egui::{Id, Response, TextBuffer, TextEdit, TextStyle, Ui};

use crate::design::theme::ThemeExt;
use crate::design::{radius, space};
use crate::text_input::field_stroke;

/// A bordered multiline text area with Tailwind-inspired input states.
pub struct TailwindTextArea<'a> {
    text: &'a mut dyn TextBuffer,
    id_salt: Option<Id>,
    desired_rows: usize,
    monospace: bool,
    enabled: bool,
    accessibility_label: Option<String>,
}

impl<'a> TailwindTextArea<'a> {
    /// Create a text area backed by the supplied text buffer.
    pub fn new(text: &'a mut dyn TextBuffer) -> Self {
        Self {
            text,
            id_salt: None,
            desired_rows: 3,
            monospace: false,
            enabled: true,
            accessibility_label: None,
        }
    }

    /// Set a stable ID salt for the text area's persistent editor state.
    pub fn id_salt(mut self, id_salt: impl Hash + std::fmt::Debug) -> Self {
        self.id_salt = Some(Id::new(id_salt));
        self
    }

    /// Set the preferred visible line count.
    pub fn desired_rows(mut self, rows: usize) -> Self {
        self.desired_rows = rows;
        self
    }

    /// Render text in a monospaced font.
    pub fn monospace(mut self) -> Self {
        self.monospace = true;
        self
    }

    /// Enable or disable text editing.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Set the required label announced for this text area by assistive technologies.
    pub fn accessibility_label(mut self, label: impl Into<String>) -> Self {
        self.accessibility_label = Some(label.into());
        self
    }

    /// Render the text area.
    pub fn show(self, ui: &mut Ui) -> Response {
        let id = self.id_salt.unwrap_or_else(|| ui.next_auto_id());
        let accessibility_label = self.accessibility_label;
        let focused = ui.memory(|memory| memory.has_focus(id));
        let tokens = ui.tokens();
        let (fill, text_color) = if self.enabled {
            (tokens.components.field_bg, tokens.text.primary)
        } else {
            (tokens.control.fill_disabled, tokens.text.disabled)
        };
        let stroke = field_stroke(&tokens, fill, focused && self.enabled, false);

        egui::Frame::new()
            .fill(fill)
            .stroke(stroke)
            .corner_radius(egui::CornerRadius::same(radius::KEY))
            .inner_margin(egui::Margin::symmetric(
                (space::S8 + space::S4) as i8,
                space::S8 as i8,
            ))
            .show(ui, |ui| {
                let mut editor = TextEdit::multiline(self.text)
                    .id(id)
                    .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(4, 2)))
                    .text_color(text_color)
                    .desired_width(f32::INFINITY)
                    .desired_rows(self.desired_rows);
                if self.monospace {
                    editor = editor.font(TextStyle::Monospace);
                }
                let response = ui.add_enabled(self.enabled, editor);
                if let Some(accessibility_label) = &accessibility_label {
                    ui.ctx().accesskit_node_builder(response.id, |builder| {
                        builder.set_label(accessibility_label.clone());
                    });
                }
                response
            })
            .inner
    }
}
