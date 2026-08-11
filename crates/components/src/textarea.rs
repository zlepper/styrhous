//! Tailwind-inspired multiline text input for editable resource content.

use std::hash::Hash;

use egui::{Id, Response, TextBuffer, TextEdit, TextStyle, Ui};

use crate::colors::{WHITE, gray, indigo};
use crate::design::{radius, spacing};

/// A bordered multiline text area with Tailwind-inspired input states.
pub struct TailwindTextArea<'a> {
    text: &'a mut dyn TextBuffer,
    id_salt: Option<Id>,
    desired_rows: usize,
    monospace: bool,
    enabled: bool,
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

    /// Render the text area.
    pub fn show(self, ui: &mut Ui) -> Response {
        let id = self.id_salt.unwrap_or_else(|| ui.next_auto_id());
        let focused = ui.memory(|memory| memory.has_focus(id));
        let (fill, stroke) = if !self.enabled {
            (gray::_100, egui::Stroke::new(1.0, gray::_200))
        } else if focused {
            (WHITE, egui::Stroke::new(1.0, indigo::_500))
        } else {
            (gray::_50, egui::Stroke::new(1.0, gray::_300))
        };

        egui::Frame::new()
            .fill(fill)
            .stroke(stroke)
            .corner_radius(radius::control())
            .inner_margin(egui::Margin::symmetric(
                (spacing::SM + spacing::XS) as i8,
                spacing::SM as i8,
            ))
            .show(ui, |ui| {
                let mut editor = TextEdit::multiline(self.text)
                    .id(id)
                    .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(4, 2)))
                    .text_color(gray::_800)
                    .desired_width(f32::INFINITY)
                    .desired_rows(self.desired_rows);
                if self.monospace {
                    editor = editor.font(TextStyle::Monospace);
                }
                ui.add_enabled(self.enabled, editor)
            })
            .inner
    }
}
