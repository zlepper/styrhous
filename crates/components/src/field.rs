//! Field wells (C04 `.inp`, `.ta`; C05 `.sel`): the borderless fill every
//! text input, text area, search field and select trigger sits in, the ring
//! that marks focus or an error, the text editor inside it and the label,
//! help and error lines of a form field ([`FormField`]).
//!
//! A field has no border at rest. Hover lightens its fill; focus draws a
//! [`size::FIELD_RING`] inset accent ring with a [`size::FIELD_HALO`] halo
//! outside it, an invalid value the same in the danger colours. A disabled
//! field keeps full opacity on `control-fill-disabled` with
//! `text-disabled` text.

use std::sync::Arc;

use egui::{
    Color32, CornerRadius, Galley, Id, Margin, Rect, Response, Shape, Stroke, StrokeKind,
    TextBuffer, Ui, UiBuilder, Vec2, epaint::RectShape, pos2, vec2,
};

use crate::design::theme::{Theme, ThemeExt};
use crate::design::type_scale::{self, TextRole};
use crate::design::{size, space, stroke};
use crate::icons::Icon;
use crate::interaction::{ControlState, Interaction};

/// The height of a field, its padding and its text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FieldSize {
    /// 28 points with 12/20 text (`.inp.sm`): dense toolbars and table rows.
    Sm,
    /// 32 points with 13/20 text (`.inp`): the default.
    #[default]
    Md,
    // C16
    /// 38 points with 13/20 text (`.inp.lg`, `.sel.lg`): the search and
    /// scope pills of a list's top bar.
    Lg,
}

impl FieldSize {
    /// The height of a field of this size.
    pub const fn height(self) -> f32 {
        match self {
            Self::Sm => size::CONTROL_SM,
            Self::Md => size::CONTROL_MD,
            Self::Lg => size::TOP_BAR_CONTROL,
        }
    }

    /// Horizontal padding between the field's edge and its content.
    pub const fn padding_x(self) -> f32 {
        match self {
            Self::Sm => size::FIELD_PADDING_X_SM,
            Self::Md | Self::Lg => size::FIELD_PADDING_X,
        }
    }

    /// The left padding of a select trigger of this size (C05 `.sel`,
    /// `.sel.lg`), before its lead.
    pub const fn select_padding_left(self) -> f32 {
        match self {
            Self::Sm | Self::Md => size::FIELD_PADDING_X,
            Self::Lg => size::SELECT_PADDING_LEFT_LG,
        }
    }

    /// The right padding of a select trigger of this size, after its
    /// chevron.
    pub const fn select_padding_right(self) -> f32 {
        match self {
            Self::Sm | Self::Md => size::SELECT_PADDING_RIGHT,
            Self::Lg => size::SELECT_PADDING_RIGHT_LG,
        }
    }

    /// The role of the field's text.
    pub const fn role(self) -> TextRole {
        match self {
            Self::Sm => type_scale::FIELD_SM,
            Self::Md | Self::Lg => type_scale::UI,
        }
    }

    /// The margin of a single-line text editor filling a field of this
    /// size: the padding on the sides, the text's line box centred.
    pub fn text_edit_margin(self) -> Margin {
        if self == Self::Md {
            return TEXT_EDIT_MARGIN;
        }
        let vertical = (self.height() - self.role().line_height()) / 2.0;
        Margin::symmetric(self.padding_x() as i8, vertical as i8)
    }
}

/// The margin of a single-line text editor in a medium field (`.inp`:
/// 14 points on the sides around a 20 point line box in 32 points).
pub const TEXT_EDIT_MARGIN: Margin = Margin::symmetric(
    size::FIELD_PADDING_X as i8,
    ((size::CONTROL_MD - type_scale::UI.line_height()) / 2.0) as i8,
);

/// What a field shows this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FieldState {
    pub hovered: bool,
    pub focused: bool,
    pub invalid: bool,
    pub enabled: bool,
}

impl FieldState {
    /// The state of a field from its interaction (or a forced state),
    /// whether its value is invalid and whether it is enabled.
    pub fn new(interaction: Interaction, invalid: bool, enabled: bool) -> Self {
        Self {
            hovered: enabled && interaction.is_active(),
            focused: enabled && interaction.focused,
            invalid,
            enabled,
        }
    }
}

/// The interaction a field paints: `forced` when set, otherwise hover over
/// `response` and keyboard focus on `focus_id` (the editor inside it).
pub(crate) fn field_interaction(
    ui: &Ui,
    response: &Response,
    focus_id: Id,
    forced: Option<ControlState>,
) -> Interaction {
    let mut interaction = Interaction::of(response, forced, false);
    if forced.is_none() {
        interaction.focused |= ui.memory(|memory| memory.has_focus(focus_id));
    }
    interaction
}

/// The fill of a field in `state`.
pub(crate) fn field_fill(theme: &Theme, state: FieldState) -> Color32 {
    if !state.enabled {
        theme.control.fill_disabled
    } else if state.hovered {
        theme.components.field_bg_hover
    } else {
        theme.components.field_bg
    }
}

/// The ring and halo colours of a field in `state`, if it shows one. An
/// invalid value keeps its danger ring while the field has focus.
pub(crate) fn field_ring(theme: &Theme, state: FieldState) -> Option<(Color32, Color32)> {
    if state.invalid {
        Some((theme.danger.fill, theme.danger.halo))
    } else if state.focused {
        Some((theme.accent.fill, theme.focus.halo))
    } else {
        None
    }
}

/// The colour of a field's value text.
pub(crate) fn value_color(theme: &Theme, enabled: bool) -> Color32 {
    if enabled {
        theme.text.primary
    } else {
        theme.text.disabled
    }
}

/// The shapes of a field well in `rect`: the halo outside it, the fill and
/// the inset ring (CSS `box-shadow: inset 0 0 0 1.5px ring, 0 0 0 3px halo`,
/// neither snapped to device pixels).
pub(crate) fn field_shape(
    theme: &Theme,
    rect: Rect,
    corner_radius: CornerRadius,
    state: FieldState,
) -> Shape {
    let fill = Shape::Rect(RectShape::filled(
        rect,
        corner_radius,
        field_fill(theme, state),
    ));
    let Some((ring, halo)) = field_ring(theme, state) else {
        return fill;
    };
    let halo_offset = size::FIELD_HALO / 2.0;
    Shape::Vec(vec![
        Shape::Rect(RectShape::stroke(
            rect.expand(halo_offset),
            stroke::grow(corner_radius, halo_offset),
            Stroke::new(size::FIELD_HALO, halo),
            StrokeKind::Middle,
        )),
        fill,
        Shape::Rect(RectShape::stroke(
            rect,
            corner_radius,
            Stroke::new(size::FIELD_RING, ring),
            StrokeKind::Inside,
        )),
    ])
}

/// Reserve the slot of a field's well in the painter's list, so the well
/// can be painted behind content added before its state is known.
pub(crate) fn reserve_well(ui: &Ui) -> egui::layers::ShapeIdx {
    ui.painter().add(Shape::Noop)
}

/// Paint the field well reserved with [`reserve_well`].
pub(crate) fn paint_well(
    ui: &Ui,
    slot: egui::layers::ShapeIdx,
    rect: Rect,
    corner_radius: CornerRadius,
    state: FieldState,
) {
    if ui.is_rect_visible(rect.expand(size::FIELD_HALO)) {
        let theme = ui.tokens();
        // A disabled well keeps full opacity, also in a disabled `Ui`.
        let mut painter = ui.painter().clone();
        // `set_opacity`, not `multiply_opacity`, which clamps factors above 1.
        painter.set_opacity(painter.opacity() * crate::control::disabled_fade_compensation(ui));
        painter.set(slot, field_shape(&theme, rect, corner_radius, state));
    }
}

/// The text caret the design draws in a focused field (`.inp .caret`): a
/// [`size::CARET`] accent bar, 1 point after the end of the text, centred
/// in `row`. Fields paint it for a forced focus state; real focus shows
/// egui's own cursor.
pub(crate) fn paint_caret(ui: &Ui, row: Rect, text_end: f32) {
    let caret = Rect::from_center_size(
        pos2(text_end + size::CARET.x / 2.0 + 1.0, row.center().y),
        size::CARET,
    );
    ui.painter()
        .rect_filled(caret, size::CARET.x / 2.0, ui.tokens().accent.fill);
}

// C16
/// How far a line of `role` text moves down when the design's caret sits
/// in its line box (`.trunc > .caret`, an inline block on the baseline):
/// the caret is taller than the text above the baseline, so the line box
/// grows upwards and, centred in the field, pushes the baseline down.
pub(crate) fn caret_line_shift(role: TextRole, pixels_per_point: f32) -> f32 {
    let baseline = role.css_baseline(pixels_per_point);
    let above = baseline.max(size::CARET.y);
    let line_box = above + role.line_height() - baseline;
    let shift = (role.line_height() - line_box) / 2.0 + above - baseline;
    (shift * pixels_per_point).round() / pixels_per_point
}

/// The design's caret standing on the baseline of a line of `role` text
/// whose line box is `row` (see [`caret_line_shift`]), 1 point after the
/// text's end.
pub(crate) fn paint_baseline_caret(ui: &Ui, role: TextRole, row: Rect, text_end: f32) {
    let baseline = row.top() + role.css_baseline(ui.pixels_per_point());
    let caret = Rect::from_min_size(pos2(text_end + 1.0, baseline - size::CARET.y), size::CARET);
    ui.painter()
        .rect_filled(caret, size::CARET.x / 2.0, ui.tokens().accent.fill);
}

/// How a field's editor lays out and labels its text.
pub(crate) struct EditorSpec<'a> {
    pub id: Id,
    pub role: TextRole,
    pub color: Color32,
    pub placeholder: Option<&'a str>,
    pub placeholder_color: Color32,
    pub accessibility_label: &'a str,
    pub enabled: bool,
    pub multiline: bool,
    pub margin: Margin,
}

/// A text editor filling `rect` (the whole field, so a click anywhere in it
/// focuses the editor), with its text in `spec.role`'s CSS line boxes from
/// the top of its margin, and the placeholder painted in their place while
/// it is empty. It paints no well; see [`field_shape`].
pub(crate) fn show_editor(
    ui: &mut Ui,
    rect: Rect,
    value: &mut dyn TextBuffer,
    spec: &EditorSpec<'_>,
) -> Response {
    let role = spec.role;
    let color = spec.color;
    let mut layouter = move |ui: &Ui, text: &dyn TextBuffer, wrap_width: f32| -> Arc<Galley> {
        let mut job = role.layout_job(
            ui.ctx(),
            text.as_str(),
            color,
            if spec.multiline {
                wrap_width
            } else {
                f32::INFINITY
            },
        );
        job.keep_trailing_whitespace = true;
        role.galley_from_job(ui.ctx(), job)
    };
    let editor = if spec.multiline {
        egui::TextEdit::multiline(value)
    } else {
        egui::TextEdit::singleline(value)
    };
    let editor = editor
        .id(spec.id)
        .frame(egui::Frame::NONE.inner_margin(spec.margin))
        .desired_width(rect.width() - spec.margin.sum().x)
        .min_size(rect.size())
        .desired_rows(1)
        .text_color(color)
        .interactive(spec.enabled)
        .layouter(&mut layouter);
    let mut editor_ui = ui.new_child(UiBuilder::new().max_rect(rect).layout(
        egui::Layout::centered_and_justified(egui::Direction::TopDown),
    ));
    let response = editor_ui.add(editor);
    if response.has_focus() {
        crate::interaction::note_text_focus(ui.ctx());
    }

    let content = Rect::from_min_max(
        rect.min + vec2(spec.margin.leftf(), spec.margin.topf()),
        rect.max - vec2(spec.margin.rightf(), spec.margin.bottomf()),
    );
    if value.as_str().is_empty()
        && let Some(placeholder) = spec.placeholder
    {
        let galley = text_galley(ui, role, placeholder, content.width(), spec.multiline);
        if ui.is_rect_visible(content) {
            type_scale::paint_galley(
                &ui.painter().with_clip_rect(content),
                content.min,
                galley,
                spec.placeholder_color,
            );
        }
    }

    let label = spec.accessibility_label.to_owned();
    let placeholder = spec.placeholder.map(str::to_owned);
    let enabled = spec.enabled;
    ui.ctx().accesskit_node_builder(response.id, |node| {
        // Only the name: `widget_info` would also reset a text area's
        // multi-line role. An empty label leaves the name to a
        // `labelled_by` relation.
        if !label.is_empty() {
            node.set_label(label);
        }
        if let Some(placeholder) = placeholder {
            node.set_placeholder(placeholder);
        }
        if !enabled {
            node.set_disabled();
        }
    });
    response
}

/// `text` in `role`'s CSS line boxes, elided at `width` for single-line
/// text (`.trunc`) or wrapped at it.
pub(crate) fn text_galley(
    ui: &Ui,
    role: TextRole,
    text: &str,
    width: f32,
    wrap: bool,
) -> Arc<Galley> {
    let mut job = role.layout_job(ui.ctx(), text, Color32::PLACEHOLDER, width);
    if !wrap {
        job.wrap = egui::text::TextWrapping::truncate_at_width(width.max(0.0));
    }
    role.galley_from_job(ui.ctx(), job)
}

/// The kind of line under a field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldMessage<'a> {
    /// Help text (`.help`): 12/16 `text-muted`.
    Help(&'a str),
    /// An error (`.err`): 12/16 `danger-text` after an exclamation-circle
    /// icon.
    Error(&'a str),
}

impl FieldMessage<'_> {
    /// Add the message line, wrapping at the available width.
    pub fn show(self, ui: &mut Ui) -> Response {
        let theme = ui.tokens();
        let role = type_scale::META;
        match self {
            Self::Help(text) => role.label(ui, text, theme.text.muted),
            Self::Error(text) => {
                let color = theme.danger.text;
                let indent = size::ICON_SM + space::S4;
                let galley = role.galley(ui.ctx(), text, color, ui.available_width() - indent);
                let (rect, response) = type_scale::allocate_text(
                    ui,
                    vec2(indent + galley.rect.width(), galley.rect.height()),
                    galley.text(),
                );
                let first_line = role.line_height();
                let icon = Rect::from_min_size(
                    pos2(rect.left(), rect.top() + (first_line - size::ICON_SM) / 2.0),
                    Vec2::splat(size::ICON_SM),
                );
                if ui.is_rect_visible(icon) {
                    Icon::ExclamationCircle.paint(ui, icon, color);
                }
                type_scale::paint_text(
                    ui,
                    &response,
                    pos2(rect.left() + indent, rect.top()),
                    galley,
                    color,
                );
                response
            }
        }
    }
}

/// A form field (`.field`): a label above a control and an optional help
/// or error line under it, [`space::LABEL_TO_CONTROL`] apart. The error
/// replaces the help text while set.
///
/// ```ignore
/// FormField::new("Display name")
///     .help("Leave blank to use the kubeconfig context name.")
///     .show(ui, |ui| TextInput::new(&mut name).accessibility_label("Display name").show(ui));
/// ```
#[must_use = "call `show` to add the field"]
pub struct FormField<'a> {
    label: &'a str,
    help: Option<&'a str>,
    error: Option<&'a str>,
}

impl<'a> FormField<'a> {
    /// A field labelled `label` (12/16 semibold `text-secondary`).
    pub fn new(label: &'a str) -> Self {
        Self {
            label,
            help: None,
            error: None,
        }
    }

    /// The help line under the control.
    pub fn help(mut self, help: &'a str) -> Self {
        self.help = Some(help);
        self
    }

    /// The error under the control, in place of the help line.
    pub fn error(mut self, error: Option<&'a str>) -> Self {
        self.error = error;
        self
    }

    /// Add the label, `control` and the message line, top to bottom.
    pub fn show<R>(self, ui: &mut Ui, control: impl FnOnce(&mut Ui) -> R) -> R {
        ui.allocate_ui_with_layout(
            vec2(ui.available_width(), 0.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, space::LABEL_TO_CONTROL);
                let theme = ui.tokens();
                type_scale::FIELD_LABEL.label(ui, self.label, theme.text.secondary);
                let inner = control(ui);
                let message = match (self.error, self.help) {
                    (Some(error), _) => Some(FieldMessage::Error(error)),
                    (None, Some(help)) => Some(FieldMessage::Help(help)),
                    (None, None) => None,
                };
                if let Some(message) = message {
                    message.show(ui);
                }
                inner
            },
        )
        .inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{UiHarnessSnapshot, setup_egui};
    use crate::{NumberField, TextArea, TextInput};
    use egui_kittest::Harness;

    #[test]
    fn snapshots_form_fields_with_help_error_and_text_area() {
        let mut harness = Harness::builder()
            .with_size(egui::vec2(400.0, 420.0))
            .build_ui(|ui| {
                ui.set_max_width(360.0);
                ui.spacing_mut().item_spacing.y = space::S20;
                FormField::new("Display name")
                    .help("Leave blank to use the kubeconfig context name.")
                    .show(ui, |ui| {
                        TextInput::new(&mut "Production Europe".to_owned())
                            .accessibility_label("Display name")
                            .show(ui);
                    });
                FormField::new("Badge")
                    .help("One or two characters.")
                    .error(Some("Use one or two characters."))
                    .show(ui, |ui| {
                        TextInput::new(&mut "PRX".to_owned())
                            .accessibility_label("Badge")
                            .width(120.0)
                            .invalid(true)
                            .show(ui);
                    });
                FormField::new("Command template")
                    .help("Use {command} as the placeholder.")
                    .show(ui, |ui| {
                        TextArea::new(&mut "alacritty -e {command}\n--title {name}".to_owned())
                            .monospace()
                            .min_rows(2)
                            .accessibility_label("Command template")
                            .show(ui);
                    });
                FormField::new("Replicas")
                    .help("Current: 2 · Min 0")
                    .show(ui, |ui| {
                        NumberField::new(&mut "3".to_owned())
                            .accessibility_labels("Replicas", "Decrease", "Increase")
                            .show(ui);
                    });
            });
        setup_egui(&mut harness);
        harness.run();
        harness.ui_harness("text_inputs/form_fields");
    }

    #[test]
    fn a_text_area_grows_with_its_rows_up_to_its_maximum() {
        let height = |text: &str| {
            let mut text = text.to_owned();
            let height = std::rc::Rc::new(std::cell::Cell::new(0.0));
            let used = height.clone();
            let mut harness = Harness::builder()
                .with_size(egui::vec2(300.0, 400.0))
                .build_ui(move |ui| {
                    TextArea::new(&mut text)
                        .min_rows(2)
                        .max_rows(4)
                        .accessibility_label("Notes")
                        .show(ui);
                    used.set(ui.min_rect().height());
                });
            setup_egui(&mut harness);
            harness.run();
            height.get()
        };
        let line = type_scale::UI.line_height();
        let padding = 2.0 * size::TEXT_AREA_PADDING_Y;
        assert_eq!(height("one"), 2.0 * line + padding);
        assert_eq!(height("1\n2\n3"), 3.0 * line + padding);
        assert_eq!(height("1\n2\n3\n4\n5\n6"), 4.0 * line + padding);
    }
}
