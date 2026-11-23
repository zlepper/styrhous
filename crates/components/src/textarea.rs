//! The multi-line text area (C04 `TextArea`).

use std::hash::Hash;

use egui::{Id, Margin, Response, Sense, TextBuffer, Ui, Vec2};

use crate::design::theme::ThemeExt;
use crate::design::type_scale::{self, TextRole};
use crate::design::{radius, size};
use crate::field::{self, EditorSpec, FieldState};
use crate::interaction::ControlState;

/// The role of a monospaced text area (`.ta.mono` at 12 px in the 20 point
/// line boxes of the surrounding UI text).
const MONO_ROLE: TextRole = type_scale::MONO_ROW;

/// A multi-line text area (`.ta`): a rounded field well that grows with its
/// content from [`Self::min_rows`] to [`Self::max_rows`] rows, then scrolls.
///
/// Focus and [`Self::invalid`] draw the same rings as a [`crate::TextInput`].
#[must_use = "call `show` to add the text area"]
pub struct TextArea<'a> {
    text: &'a mut dyn TextBuffer,
    id_salt: Option<Id>,
    min_rows: usize,
    max_rows: usize,
    min_height: f32,
    monospace: bool,
    enabled: bool,
    invalid: bool,
    placeholder: Option<String>,
    accessibility_label: Option<String>,
    forced_state: Option<ControlState>,
}

impl<'a> TextArea<'a> {
    /// A text area editing `text`.
    pub fn new(text: &'a mut dyn TextBuffer) -> Self {
        Self {
            text,
            id_salt: None,
            min_rows: 3,
            max_rows: 12,
            min_height: 0.0,
            monospace: false,
            enabled: true,
            invalid: false,
            placeholder: None,
            accessibility_label: None,
            forced_state: None,
        }
    }

    /// A stable ID salt for the editor's state.
    pub fn id_salt(mut self, id_salt: impl Hash + std::fmt::Debug) -> Self {
        self.id_salt = Some(Id::new(id_salt));
        self
    }

    /// The fewest rows the area shows (3 by default).
    pub fn min_rows(mut self, rows: usize) -> Self {
        self.min_rows = rows.max(1);
        self
    }

    /// The most rows the area grows to before it scrolls (12 by default).
    pub fn max_rows(mut self, rows: usize) -> Self {
        self.max_rows = rows.max(1);
        self
    }

    /// The least height of the well, whatever [`Self::min_rows`] says.
    pub fn min_height(mut self, height: f32) -> Self {
        self.min_height = height;
        self
    }

    /// Set the text in IBM Plex Mono 12 px, for templates and data values.
    pub fn monospace(mut self) -> Self {
        self.monospace = true;
        self
    }

    /// Enable or disable editing.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Mark the value invalid: the danger ring.
    pub fn invalid(mut self, invalid: bool) -> Self {
        self.invalid = invalid;
        self
    }

    /// The `text-muted` text shown while the area is empty.
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    /// The name assistive technologies and UI tests read for the area.
    pub fn accessibility_label(mut self, label: impl Into<String>) -> Self {
        self.accessibility_label = Some(label.into());
        self
    }

    /// Paint `state` whatever the pointer and keyboard do (design boards).
    pub fn force_state(mut self, state: ControlState) -> Self {
        self.forced_state = Some(state);
        self
    }

    /// Add the text area across the available width.
    pub fn show(self, ui: &mut Ui) -> Response {
        let role = if self.monospace {
            MONO_ROLE
        } else {
            type_scale::UI
        };
        let enabled = self.enabled && ui.is_enabled();
        let theme = ui.tokens();
        let margin =
            Margin::symmetric(size::FIELD_PADDING_X as i8, size::TEXT_AREA_PADDING_Y as i8);
        let width = ui.available_width();
        let rows = field::text_galley(ui, role, self.text.as_str(), width - margin.sum().x, true)
            .rows
            .len();
        let visible_rows = rows.clamp(self.min_rows, self.max_rows.max(self.min_rows));
        let height =
            (visible_rows as f32 * role.line_height() + margin.sum().y).max(self.min_height);

        let (rect, slot) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
        let id = self.id_salt.map_or_else(
            || slot.id.with("editor"),
            |salt| ui.make_persistent_id(salt),
        );
        let well = field::reserve_well(ui);
        let label = self.accessibility_label.unwrap_or_default();
        let spec = EditorSpec {
            id,
            role,
            color: field::value_color(&theme, enabled),
            placeholder: self.placeholder.as_deref(),
            placeholder_color: theme.text.muted,
            accessibility_label: &label,
            enabled,
            multiline: true,
            margin,
        };
        let response = if rows > visible_rows {
            let mut scroll_ui = ui.new_child(egui::UiBuilder::new().max_rect(rect));
            crate::scroll::vertical()
                .id_salt(id.with("scroll"))
                .max_height(height)
                .show(&mut scroll_ui, |ui| {
                    let content = egui::Rect::from_min_size(
                        ui.cursor().min,
                        Vec2::new(width, rows as f32 * role.line_height() + margin.sum().y),
                    );
                    ui.allocate_rect(content, Sense::hover());
                    field::show_editor(ui, content, self.text, &spec)
                })
                .inner
        } else {
            field::show_editor(ui, rect, self.text, &spec)
        };

        let interaction = field::field_interaction(ui, &response, id, self.forced_state);
        let state = FieldState::new(interaction, self.invalid, enabled);
        field::paint_well(ui, well, rect, radius::TEXT_AREA.into(), state);
        response
    }
}
