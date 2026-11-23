//! The single-line text input (C04 `TextInput`).

use std::{fmt::Debug, hash::Hash};

use egui::{CornerRadius, Id, Margin, Rect, Response, Sense, Ui, Vec2, pos2};

use crate::design::theme::ThemeExt;
use crate::design::type_scale::{self, TextRole};
use crate::design::{radius, size, space};
use crate::field::{self, EditorSpec, FieldSize, FieldState};
use crate::icons::Icon;
use crate::interaction::ControlState;
use crate::keycap::Keycap;

/// A single-line text input (`.inp`): a borderless pill well, 32 points
/// tall ([`FieldSize::Sm`]: 28), with an optional leading icon.
///
/// Focus draws the accent ring and halo; [`Self::invalid`] the danger ring.
/// A disabled input keeps its value readable in `text-disabled` and cannot
/// be focused. Put it in a [`crate::FormField`] for a label and help or
/// error line; never use the placeholder as the label.
#[must_use = "call `show` to add the input"]
pub struct TextInput<'a> {
    value: &'a mut String,
    id: Option<Id>,
    placeholder: Option<String>,
    accessibility_label: String,
    width: Option<f32>,
    size: FieldSize,
    role: Option<TextRole>,
    leading_icon: Option<Icon>,
    invalid: bool,
    enabled: bool,
    request_focus: bool,
    forced_state: Option<ControlState>,
    corner_radius: CornerRadius,
    labelled_by: Option<Id>,
    height: Option<f32>,
    icon_size: f32,
    trailing_key: Option<String>,
}

impl<'a> TextInput<'a> {
    /// An input editing `value`.
    pub fn new(value: &'a mut String) -> Self {
        Self {
            value,
            id: None,
            placeholder: None,
            accessibility_label: "Text input".to_owned(),
            width: None,
            size: FieldSize::default(),
            role: None,
            leading_icon: None,
            invalid: false,
            enabled: true,
            request_focus: false,
            forced_state: None,
            corner_radius: radius::FULL.into(),
            labelled_by: None,
            height: None,
            icon_size: size::ICON,
            trailing_key: None,
        }
    }

    /// A stable ID for the editor's state.
    pub fn id_salt(mut self, id_salt: impl Hash + Debug) -> Self {
        self.id = Some(Id::new(id_salt));
        self
    }

    /// The `text-muted` text shown while the input is empty.
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    /// The name assistive technologies and UI tests read for the input.
    pub fn accessibility_label(mut self, label: impl Into<String>) -> Self {
        self.accessibility_label = label.into();
        self
    }

    /// Name the input after the visible label widget `label` instead of
    /// [`Self::accessibility_label`].
    pub fn labelled_by(mut self, label: Id) -> Self {
        self.labelled_by = Some(label);
        self
    }

    /// The width of the input; the available width by default.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    pub fn size(mut self, size: FieldSize) -> Self {
        self.size = size;
        self
    }

    /// Set the value in IBM Plex Mono, for templates and data values.
    pub fn monospace(mut self) -> Self {
        let role = self.size.role();
        self.role = Some(type_scale::MONO.with_size(role.size(), role.line_height()));
        self
    }

    // C09
    /// Set the field's height in place of its size's (the sidebar's 34 point
    /// kind filter); the text stays centred.
    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    /// Draw the leading icon `size` points square instead of [`size::ICON`].
    pub fn icon_size(mut self, size: f32) -> Self {
        self.icon_size = size;
        self
    }

    /// End the field with the keycap of its shortcut `key` (`.inp .kbd`).
    pub fn trailing_keycap(mut self, key: impl Into<String>) -> Self {
        self.trailing_key = Some(key.into());
        self
    }

    /// Lead the value with a `text-muted` icon (`.inp > .ic`).
    pub fn leading_icon(mut self, icon: Icon) -> Self {
        self.leading_icon = Some(icon);
        self
    }

    /// Mark the value invalid: the danger ring, also while focused. Say why
    /// in the field's error line.
    pub fn invalid(mut self, invalid: bool) -> Self {
        self.invalid = invalid;
        self
    }

    /// Enable or disable editing.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Focus the input in this frame.
    pub fn request_focus(mut self, focus: bool) -> Self {
        self.request_focus = focus;
        self
    }

    /// Paint `state` whatever the pointer and keyboard do (design boards and
    /// snapshots); a forced focus also draws the design's caret after the
    /// value. The input still responds to real input.
    pub fn force_state(mut self, state: ControlState) -> Self {
        self.forced_state = Some(state);
        self
    }

    /// Square off corners where the input joins other controls
    /// ([`crate::NumberField`]).
    pub(crate) fn corner_radius(mut self, corner_radius: impl Into<CornerRadius>) -> Self {
        self.corner_radius = corner_radius.into();
        self
    }

    /// Add the input. The response is the editor's: check
    /// [`Response::changed`], [`Response::lost_focus`] and focus on it.
    pub fn show(self, ui: &mut Ui) -> Response {
        let width = self.width.unwrap_or_else(|| ui.available_width());
        let (rect, slot) = ui.allocate_exact_size(
            Vec2::new(width, self.height.unwrap_or(self.size.height())),
            Sense::hover(),
        );
        let id = self.id.unwrap_or_else(|| slot.id.with("editor"));
        let enabled = self.enabled && ui.is_enabled();
        let theme = ui.tokens();
        let role = self.role.unwrap_or(self.size.role());
        let well = field::reserve_well(ui);

        let mut margin = self.size.text_edit_margin();
        if let Some(height) = self.height {
            // Whole-point margins; an odd remainder goes below the text.
            let spare = (height - role.line_height()).max(0.0).round() as i8;
            margin.top = spare / 2;
            margin.bottom = spare - spare / 2;
        }
        let content_left = rect.left() + margin.leftf();
        if self.leading_icon.is_some() {
            margin.left += (self.icon_size + space::S8) as i8;
        }
        if let Some(key) = &self.trailing_key {
            margin.right += (Keycap::new(key).size(ui).x + space::S8) as i8;
        }
        let spec = EditorSpec {
            id,
            role,
            color: field::value_color(&theme, enabled),
            placeholder: self.placeholder.as_deref(),
            placeholder_color: theme.text.muted,
            accessibility_label: if self.labelled_by.is_some() {
                ""
            } else {
                &self.accessibility_label
            },
            enabled,
            multiline: false,
            margin,
        };
        let response = field::show_editor(ui, rect, self.value, &spec);
        let response = match self.labelled_by {
            Some(label) => response.labelled_by(label),
            None => response,
        };
        if self.request_focus && enabled {
            response.request_focus();
        }

        let interaction = field::field_interaction(ui, &response, id, self.forced_state);
        let state = FieldState::new(interaction, self.invalid, enabled);
        field::paint_well(ui, well, rect, self.corner_radius, state);
        if let Some(icon) = self.leading_icon {
            let icon_rect = Rect::from_min_size(
                pos2(content_left, rect.center().y - self.icon_size / 2.0),
                Vec2::splat(self.icon_size),
            );
            icon.paint(ui, icon_rect, theme.text.muted);
        }
        if let Some(key) = &self.trailing_key {
            let keycap = Keycap::new(key);
            let key_size = keycap.size(ui);
            let key_rect = Rect::from_min_size(
                pos2(
                    rect.right() - self.size.padding_x() - key_size.x,
                    rect.center().y - key_size.y / 2.0,
                ),
                key_size,
            );
            keycap.paint(ui, key_rect);
        }
        if self.forced_state == Some(ControlState::Focused) {
            let row = text_row(rect, margin);
            let width = field::text_galley(ui, role, self.value, f32::INFINITY, true)
                .rect
                .width();
            field::paint_caret(ui, row, row.left() + width);
        }
        response
    }
}

/// The line box of a field's text: `rect` inside `margin`.
pub(crate) fn text_row(rect: Rect, margin: Margin) -> Rect {
    Rect::from_min_max(
        rect.min + egui::vec2(margin.leftf(), margin.topf()),
        rect.max - egui::vec2(margin.rightf(), margin.bottomf()),
    )
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
            TextInput::new(&mut empty_value)
                .id_salt("empty-input")
                .placeholder("Enter a metadata key")
                .accessibility_label("Metadata key")
                .show(ui);
            ui.add_space(space::S12);
            TextInput::new(&mut filled_value)
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
