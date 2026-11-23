//! Checkbox, radio, radio card and switch (C06).
//!
//! The controls are 16 points (the switch 32 × 18) and the label is part
//! of the hit target. A switch applies its setting immediately; checkboxes
//! are for selections and for settings saved with a button. Keyboard focus
//! rings the control, not its label. Disabled controls fade to the theme's
//! disabled opacity as one group.

use egui::{
    Color32, CornerRadius, Painter, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2,
    WidgetInfo, WidgetType, pos2, vec2,
};

use crate::PointingHand;
use crate::control::paint_on_fill;
use crate::design::theme::{Theme, ThemeExt};
use crate::design::type_scale::{self, TextRole};
use crate::design::{radius, size, space, stroke};
use crate::icons::Icon;
use crate::interaction::{ControlState, Interaction};

/// The state of a checkbox.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CheckState {
    #[default]
    Off,
    On,
    /// Some of what the checkbox stands for is on (a select-all row);
    /// clicking turns everything on.
    Mixed,
}

impl CheckState {
    /// The state of a select-all checkbox when `selected` of `total` items
    /// are selected.
    pub fn from_counts(selected: usize, total: usize) -> Self {
        if selected == 0 {
            Self::Off
        } else if selected >= total {
            Self::On
        } else {
            Self::Mixed
        }
    }

    /// Describe `response` as a checkbox in this state named `label`: checked,
    /// unchecked or mixed.
    pub(crate) fn describe(self, response: &Response, enabled: bool, label: &str) {
        match self {
            Self::Mixed => {
                response.widget_info(|| WidgetInfo::labeled(WidgetType::Checkbox, enabled, label));
                response.ctx.accesskit_node_builder(response.id, |node| {
                    node.set_toggled(egui::accesskit::Toggled::Mixed);
                });
            }
            state => response.widget_info(|| {
                WidgetInfo::selected(WidgetType::Checkbox, enabled, state == Self::On, label)
            }),
        }
    }
}

impl From<bool> for CheckState {
    fn from(on: bool) -> Self {
        if on { Self::On } else { Self::Off }
    }
}

/// The fill and border of a checkbox or radio box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxLook {
    /// The fill; transparent for none.
    pub fill: Color32,
    /// The border colour; transparent for none.
    pub border: Color32,
}

impl BoxLook {
    /// An unchecked box (`.cb`, `.rd`): a `control-border` outline, with
    /// `control-border-hover` and a `control-fill` wash on hover.
    pub fn off(theme: &Theme, hovered: bool) -> Self {
        if hovered {
            Self {
                fill: theme.control.fill,
                border: theme.control.border_hover,
            }
        } else {
            Self {
                fill: Color32::TRANSPARENT,
                border: theme.control.border,
            }
        }
    }

    /// A checked checkbox (`.cb.is-on`, `.cb.is-mixed`): the accent fill,
    /// lighter on hover.
    pub fn checked(theme: &Theme, hovered: bool) -> Self {
        let fill = if hovered {
            theme.accent.fill_hover
        } else {
            theme.accent.fill
        };
        Self { fill, border: fill }
    }

    /// A selected radio (`.rd.is-on`), which on hover shows the unchecked
    /// hover box around its dot (`.rd.is-hover` overrides the fill).
    pub fn selected_radio(theme: &Theme, hovered: bool) -> Self {
        if hovered {
            Self::off(theme, true)
        } else {
            Self {
                fill: theme.accent.fill,
                border: theme.accent.fill,
            }
        }
    }

    fn of_checkbox(theme: &Theme, state: CheckState, hovered: bool) -> Self {
        match state {
            CheckState::Off => Self::off(theme, hovered),
            CheckState::On | CheckState::Mixed => Self::checked(theme, hovered),
        }
    }
}

/// Paint a 16 point checkbox or radio box in `rect`: `look`'s fill inside a
/// CSS 1.5 px border (snapped to device pixels), with `corner_radius`
/// ([`radius::CHECKBOX`] or [`radius::FULL`] for a radio).
pub fn paint_box(
    painter: &Painter,
    rect: Rect,
    corner_radius: impl Into<CornerRadius>,
    look: BoxLook,
) {
    let corner_radius = corner_radius.into();
    // The fill shows under the border, as CSS backgrounds do.
    if look.fill.a() > 0 {
        painter.rect_filled(rect, corner_radius, look.fill);
    }
    if look.border.a() > 0 && look.border != look.fill {
        painter.rect_stroke(
            rect,
            corner_radius,
            stroke::control_border(look.border, painter.pixels_per_point()),
            StrokeKind::Inside,
        );
    }
}

/// Paint a checkbox in `state` into the [`size::CHECKBOX`] square `rect`:
/// its box ([`paint_box`]) and the `text-on-fill` check or minus.
pub fn paint_checkbox(ui: &mut Ui, rect: Rect, state: CheckState, hovered: bool) {
    let theme = ui.tokens();
    paint_box(
        ui.painter(),
        rect,
        radius::CHECKBOX,
        BoxLook::of_checkbox(&theme, state, hovered),
    );
    let mark = match state {
        CheckState::Off => return,
        CheckState::On => Icon::Check,
        CheckState::Mixed => Icon::Minus,
    };
    let glyph = Rect::from_center_size(rect.center(), Vec2::splat(size::ICON_XS));
    paint_on_fill(ui, theme.text.on_fill, |ui, color, _| {
        mark.paint(ui, glyph, color)
    });
}

/// Paint a radio into the [`size::CHECKBOX`] square `rect`: a circle and,
/// when `selected`, the [`size::RADIO_DOT`] `text-on-fill` dot.
pub fn paint_radio(ui: &mut Ui, rect: Rect, selected: bool, hovered: bool) {
    let theme = ui.tokens();
    if !selected {
        paint_box(
            ui.painter(),
            rect,
            radius::FULL,
            BoxLook::off(&theme, hovered),
        );
        return;
    }
    let look = BoxLook::selected_radio(&theme, hovered);
    if hovered {
        paint_box(ui.painter(), rect, radius::FULL, look);
    } else {
        ui.painter()
            .circle_filled(rect.center(), rect.width() / 2.0, look.fill);
    }
    paint_on_fill(ui, theme.text.on_fill, |ui, color, _| {
        ui.painter()
            .circle_filled(rect.center(), size::RADIO_DOT / 2.0, color);
    });
}

/// Paint a switch into the [`size::SWITCH`] `rect`: the track and its
/// rounded-square knob, at the end when `on`.
pub fn paint_switch(ui: &mut Ui, rect: Rect, on: bool) {
    let theme = ui.tokens();
    let (track, knob, knob_left) = if on {
        (
            theme.accent.fill,
            theme.text.on_fill,
            rect.right() - size::SWITCH_KNOB_INSET - size::SWITCH_KNOB,
        )
    } else {
        (
            theme.control.fill_strong,
            theme.text.secondary,
            rect.left() + size::SWITCH_KNOB_INSET,
        )
    };
    ui.painter().rect_filled(rect, radius::FULL, track);
    let knob_rect = Rect::from_min_size(
        pos2(knob_left, rect.top() + size::SWITCH_KNOB_INSET),
        Vec2::splat(size::SWITCH_KNOB),
    );
    paint_on_fill(ui, knob, |ui, color, _| {
        ui.painter()
            .rect_filled(knob_rect, radius::SWITCH_KNOB, color);
    });
}

/// Which control a [`Selection`] draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Checkbox(CheckState),
    Radio(bool),
    Switch(bool),
}

impl Kind {
    fn control_size(self) -> Vec2 {
        match self {
            Self::Switch(_) => size::SWITCH,
            Self::Checkbox(_) | Self::Radio(_) => Vec2::splat(size::CHECKBOX),
        }
    }

    /// Gap between the control and its label (`label.row gap`).
    fn label_gap(self) -> f32 {
        match self {
            Self::Switch(_) => size::SWITCH_LABEL_GAP,
            Self::Checkbox(_) | Self::Radio(_) => space::S8,
        }
    }

    fn corner_radius(self) -> CornerRadius {
        match self {
            Self::Checkbox(_) => radius::CHECKBOX.into(),
            Self::Radio(_) | Self::Switch(_) => radius::FULL.into(),
        }
    }

    /// The accessibility role the control is read as.
    fn role(self) -> egui::accesskit::Role {
        match self {
            Self::Checkbox(_) => egui::accesskit::Role::CheckBox,
            Self::Radio(_) => egui::accesskit::Role::RadioButton,
            Self::Switch(_) => egui::accesskit::Role::Switch,
        }
    }

    /// Describe `response` as this control named `label`.
    fn describe(self, response: &Response, enabled: bool, label: &str) {
        match self {
            Self::Checkbox(state) => state.describe(response, enabled, label),
            Self::Radio(selected) => response.widget_info(|| {
                WidgetInfo::selected(WidgetType::RadioButton, enabled, selected, label)
            }),
            Self::Switch(on) => response
                .widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, enabled, on, label)),
        }
    }

    fn paint(self, ui: &mut Ui, rect: Rect, hovered: bool) {
        match self {
            Self::Checkbox(state) => paint_checkbox(ui, rect, state, hovered),
            Self::Radio(selected) => paint_radio(ui, rect, selected, hovered),
            Self::Switch(on) => paint_switch(ui, rect, on),
        }
    }
}

/// The text of a selection control: a label and an optional description
/// under it.
struct Text<'a> {
    label: &'a str,
    description: Option<&'a str>,
    role: TextRole,
}

/// A checkbox, radio or switch with its label; the whole row is the hit
/// target.
struct Selection<'a> {
    kind: Kind,
    text: Text<'a>,
    accessibility_label: Option<&'a str>,
    enabled: bool,
    forced_state: Option<ControlState>,
}

impl egui::Widget for Selection<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        self.paint(ui)
    }
}

impl Selection<'_> {
    /// Add the control, disabled with [`Ui::add_enabled`] (no child `Ui`, and
    /// a disabled `Ui` is never faded twice).
    fn show(self, ui: &mut Ui) -> Response {
        let enabled = self.enabled;
        ui.add_enabled(enabled, self)
    }

    fn paint(self, ui: &mut Ui) -> Response {
        let theme = ui.tokens();
        let control = self.kind.control_size();
        let label = (!self.text.label.is_empty()).then(|| {
            let role = self.text.role;
            let gap = self.kind.label_gap();
            let width = ui.available_width() - control.x - gap;
            let title = role.galley(ui.ctx(), self.text.label, theme.text.primary, width);
            let description = self.text.description.map(|description| {
                type_scale::META.galley(ui.ctx(), description, theme.text.muted, width)
            });
            (gap, title, description)
        });
        let (size, text_height) = match &label {
            None => (control, 0.0),
            Some((gap, title, description)) => {
                let text_width = title
                    .rect
                    .width()
                    .max(description.as_ref().map_or(0.0, |d| d.rect.width()));
                let text_height =
                    title.rect.height() + description.as_ref().map_or(0.0, |d| d.rect.height());
                (
                    vec2(control.x + gap + text_width, text_height.max(control.y)),
                    text_height,
                )
            }
        };
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        let name = self.accessibility_label.unwrap_or(self.text.label);
        self.kind.describe(&response, ui.is_enabled(), name);
        let role = self.kind.role();
        ui.ctx()
            .accesskit_node_builder(response.id, |node| node.set_role(role));
        let response = if ui.is_enabled() {
            response.with_pointing_hand()
        } else {
            response
        };

        // A single line centres the control on it (`align-items: center`); a
        // label with a description puts it at the top (`flex-start`).
        let control_top = if label
            .as_ref()
            .is_some_and(|(_, _, description)| description.is_some())
        {
            rect.top()
        } else {
            rect.center().y - control.y / 2.0
        };
        let control_rect = Rect::from_min_size(pos2(rect.left(), control_top), control);
        let state = Interaction::of(&response, self.forced_state, false);
        if ui.is_rect_visible(rect.expand(size::FOCUS_RING_EXTENT)) {
            if state.focused {
                stroke::paint_focus_ring(
                    ui.painter(),
                    control_rect,
                    self.kind.corner_radius(),
                    theme.surface.canvas,
                    theme.focus.ring,
                );
            }
            self.kind.paint(ui, control_rect, state.hovered);
            if let Some((gap, title, description)) = label {
                let text_left = control_rect.right() + gap;
                let text_top = rect.center().y - text_height / 2.0;
                let title_pos = pos2(text_left, text_top);
                type_scale::expose_text_runs_as(ui, &response, title_pos, &title, self.kind.role());
                type_scale::paint_galley(
                    ui.painter(),
                    title_pos,
                    title.clone(),
                    theme.text.primary,
                );
                if let Some(description) = description {
                    let text = description.text().to_owned();
                    ui.ctx()
                        .accesskit_node_builder(response.id, |node| node.set_description(text));
                    let pos = pos2(text_left, text_top + title.rect.height());
                    type_scale::paint_galley(ui.painter(), pos, description, theme.text.muted);
                }
            }
        }
        response
    }
}

/// A checkbox (`.cb`) with its label as part of the hit target.
///
/// ```ignore
/// if Checkbox::new(&mut show_system, "Show system namespaces").show(ui).changed() { … }
/// ```
#[must_use = "call `show` to add the checkbox"]
pub struct Checkbox<'a> {
    state: CheckState,
    value: Option<&'a mut bool>,
    label: &'a str,
    description: Option<&'a str>,
    accessibility_label: Option<&'a str>,
    enabled: bool,
    forced_state: Option<ControlState>,
}

impl<'a> Checkbox<'a> {
    /// A checkbox toggling `value`, labelled `label` (empty for none: then
    /// set [`Self::accessibility_label`]).
    pub fn new(value: &'a mut bool, label: &'a str) -> Self {
        Self {
            state: (*value).into(),
            value: Some(value),
            label,
            description: None,
            accessibility_label: None,
            enabled: true,
            forced_state: None,
        }
    }

    /// A checkbox showing `state` that the caller updates when it is
    /// clicked (a select-all checkbox in [`CheckState::Mixed`]).
    pub fn with_state(state: CheckState, label: &'a str) -> Self {
        Self {
            state,
            value: None,
            label,
            description: None,
            accessibility_label: None,
            enabled: true,
            forced_state: None,
        }
    }

    /// A 12/16 `text-muted` line under the label, when the effect is not
    /// obvious.
    pub fn description(mut self, description: &'a str) -> Self {
        self.description = Some(description);
        self
    }

    /// The name read by assistive technologies, when there is no visible
    /// label or it is ambiguous out of context.
    pub fn accessibility_label(mut self, label: &'a str) -> Self {
        self.accessibility_label = Some(label);
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Paint `state` whatever the pointer and keyboard do (design boards).
    pub fn force_state(mut self, state: ControlState) -> Self {
        self.forced_state = Some(state);
        self
    }

    /// Add the checkbox. A click toggles the bound value and marks the
    /// response changed.
    pub fn show(self, ui: &mut Ui) -> Response {
        let mut response = Selection {
            kind: Kind::Checkbox(self.state),
            text: Text {
                label: self.label,
                description: self.description,
                role: type_scale::UI,
            },
            accessibility_label: self.accessibility_label,
            enabled: self.enabled,
            forced_state: self.forced_state,
        }
        .show(ui);
        if response.clicked()
            && let Some(value) = self.value
        {
            *value = !*value;
            response.mark_changed();
        }
        response
    }
}

/// A radio (`.rd`) with its label as part of the hit target; one of a
/// group sharing a value.
///
/// ```ignore
/// Radio::new(&mut server, Server::Cloud, "Styrhous cloud").show(ui);
/// Radio::new(&mut server, Server::SelfHosted, "Self-hosted server").show(ui);
/// ```
#[must_use = "call `show` to add the radio"]
pub struct Radio<'a, T> {
    current: &'a mut T,
    value: T,
    label: &'a str,
    description: Option<&'a str>,
    accessibility_label: Option<&'a str>,
    enabled: bool,
    forced_state: Option<ControlState>,
}

impl<'a, T: PartialEq> Radio<'a, T> {
    /// A radio that selects `value` into `current`.
    pub fn new(current: &'a mut T, value: T, label: &'a str) -> Self {
        Self {
            current,
            value,
            label,
            description: None,
            accessibility_label: None,
            enabled: true,
            forced_state: None,
        }
    }

    /// The name read by assistive technologies, when there is no visible
    /// label.
    pub fn accessibility_label(mut self, label: &'a str) -> Self {
        self.accessibility_label = Some(label);
        self
    }

    /// A 12/16 `text-muted` line under the label.
    pub fn description(mut self, description: &'a str) -> Self {
        self.description = Some(description);
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Paint `state` whatever the pointer and keyboard do (design boards).
    pub fn force_state(mut self, state: ControlState) -> Self {
        self.forced_state = Some(state);
        self
    }

    /// Add the radio. A click selects its value and marks the response
    /// changed when that changed the selection.
    pub fn show(self, ui: &mut Ui) -> Response {
        let selected = *self.current == self.value;
        let mut response = Selection {
            kind: Kind::Radio(selected),
            text: Text {
                label: self.label,
                description: self.description,
                role: type_scale::UI,
            },
            accessibility_label: self.accessibility_label,
            enabled: self.enabled,
            forced_state: self.forced_state,
        }
        .show(ui);
        if response.clicked() && !selected {
            *self.current = self.value;
            response.mark_changed();
        }
        response
    }
}

/// A switch (`.sw`): a 32 × 18 track with a knob, for settings that apply
/// immediately. Use a [`Checkbox`] when a Save button applies the setting.
#[must_use = "call `show` to add the switch"]
pub struct Switch<'a> {
    on: &'a mut bool,
    label: &'a str,
    accessibility_label: Option<&'a str>,
    enabled: bool,
    forced_state: Option<ControlState>,
}

impl<'a> Switch<'a> {
    /// A switch toggling `on`, labelled `label` (describe the on state).
    pub fn new(on: &'a mut bool, label: &'a str) -> Self {
        Self {
            on,
            label,
            accessibility_label: None,
            enabled: true,
            forced_state: None,
        }
    }

    /// The name read by assistive technologies, when there is no visible
    /// label.
    pub fn accessibility_label(mut self, label: &'a str) -> Self {
        self.accessibility_label = Some(label);
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Paint `state` whatever the pointer and keyboard do (design boards).
    pub fn force_state(mut self, state: ControlState) -> Self {
        self.forced_state = Some(state);
        self
    }

    /// Add the switch. A click toggles it and marks the response changed.
    pub fn show(self, ui: &mut Ui) -> Response {
        let mut response = Selection {
            kind: Kind::Switch(*self.on),
            text: Text {
                label: self.label,
                description: None,
                role: type_scale::UI,
            },
            accessibility_label: self.accessibility_label,
            enabled: self.enabled,
            forced_state: self.forced_state,
        }
        .show(ui);
        if response.clicked() {
            *self.on = !*self.on;
            response.mark_changed();
        }
        response
    }
}

/// A radio card (`.rcard`): a whole-card hit target with a radio, a 13/20
/// medium title and a 12/16 `text-muted` description. Selected cards use
/// `item-selected` with a 1.5 px inset accent ring; hover lightens the
/// surface. Content that belongs to the option (a command template) goes
/// in [`Self::show`]'s body, shown inside the selected card.
#[must_use = "call `show` to add the radio card"]
pub struct RadioCard<'a> {
    selected: bool,
    title: &'a str,
    description: &'a str,
    forced_state: Option<ControlState>,
}

/// What a [`RadioCard`] did this frame.
pub struct RadioCardResponse<R> {
    /// The card's response; check [`Response::clicked`] to select it.
    pub response: Response,
    /// What the body returned, if the card is selected and has one.
    pub inner: Option<R>,
}

impl<'a> RadioCard<'a> {
    pub fn new(selected: bool, title: &'a str, description: &'a str) -> Self {
        Self {
            selected,
            title,
            description,
            forced_state: None,
        }
    }

    /// Paint `state` whatever the pointer and keyboard do (design boards).
    pub fn force_state(mut self, state: ControlState) -> Self {
        self.forced_state = Some(state);
        self
    }

    /// Add the card across the available width.
    pub fn show(self, ui: &mut Ui) -> Response {
        self.show_with_body(ui, None::<fn(&mut Ui)>).response
    }

    /// Add the card with `body` under its text while it is selected.
    pub fn show_with_body<R>(
        self,
        ui: &mut Ui,
        body: Option<impl FnOnce(&mut Ui) -> R>,
    ) -> RadioCardResponse<R> {
        let theme = ui.tokens();
        let padding = vec2(size::RADIO_CARD_PADDING_X, size::RADIO_CARD_PADDING_Y);
        let width = ui.available_width();
        let text_left = size::CHECKBOX + space::S12;
        let text_width = width - 2.0 * padding.x - text_left;
        let title =
            type_scale::UI_CONTROL.galley(ui.ctx(), self.title, theme.text.primary, text_width);
        let description =
            type_scale::META.galley(ui.ctx(), self.description, theme.text.muted, text_width);
        let text_height = title.rect.height() + description.rect.height();

        let top = ui.cursor().min;
        let background = ui.painter().add(egui::Shape::Noop);
        // The card above its body is the hit target; registered first, so
        // the body's own widgets stay on top of it.
        // Without a body the whole card; with one, the card above it.
        let header_height = if self.selected && body.is_some() {
            padding.y + text_height + space::S12
        } else {
            2.0 * padding.y + text_height
        };
        let header = Rect::from_min_size(top, vec2(width, header_height));
        let response = ui
            .interact(header, ui.next_auto_id().with("radio-card"), Sense::click())
            .with_pointing_hand();
        response.widget_info(|| {
            WidgetInfo::selected(
                WidgetType::RadioButton,
                ui.is_enabled(),
                self.selected,
                self.title,
            )
        });
        let mut inner = None;
        let mut body_height = 0.0;
        if self.selected
            && let Some(body) = body
        {
            let body_rect = Rect::from_min_size(
                top + vec2(padding.x + text_left, padding.y + text_height + space::S12),
                vec2(text_width, f32::INFINITY),
            );
            let mut body_ui = ui.new_child(egui::UiBuilder::new().max_rect(body_rect));
            inner = Some(body(&mut body_ui));
            body_height = space::S12 + body_ui.min_rect().height();
        }
        let rect = Rect::from_min_size(
            top,
            vec2(width, 2.0 * padding.y + text_height + body_height),
        );
        ui.allocate_rect(rect, Sense::hover());

        let state = Interaction::of(&response, self.forced_state, false);
        if ui.is_rect_visible(rect) {
            let corner_radius = CornerRadius::same(radius::RADIO_CARD);
            ui.painter().set(
                background,
                selectable_surface(
                    &theme,
                    rect,
                    corner_radius,
                    (self.selected, state.hovered),
                    Some(theme.surface.panel),
                    true,
                ),
            );
            if state.focused {
                stroke::paint_focus_ring(
                    ui.painter(),
                    rect,
                    corner_radius,
                    theme.surface.canvas,
                    theme.focus.ring,
                );
            }
            let control = Rect::from_min_size(
                rect.min + padding + vec2(0.0, size::RADIO_CARD_RADIO_OFFSET),
                Vec2::splat(size::CHECKBOX),
            );
            paint_radio(ui, control, self.selected, false);
            let title_pos = rect.min + padding + vec2(text_left, 0.0);
            type_scale::expose_text_runs_as(
                ui,
                &response,
                title_pos,
                &title,
                egui::accesskit::Role::RadioButton,
            );
            let text = description.text().to_owned();
            ui.ctx()
                .accesskit_node_builder(response.id, |node| node.set_description(text));
            type_scale::paint_galley(ui.painter(), title_pos, title.clone(), theme.text.primary);
            type_scale::paint_galley(
                ui.painter(),
                title_pos + vec2(0.0, title.rect.height()),
                description,
                theme.text.muted,
            );
        }
        RadioCardResponse { response, inner }
    }
}

// C18
/// The surface of a selectable card or list item (`.rcard`, `.ctile`,
/// `.si`): `item-selected` while `selected` (ringed 1.5 points inside in
/// `accent-fill` when `ring`), `surface-panel-hover` while `hovered`, and
/// `rest` otherwise (nothing for a transparent rest).
pub(crate) fn selectable_surface(
    theme: &crate::design::theme::Theme,
    rect: egui::Rect,
    corner_radius: impl Into<CornerRadius>,
    (selected, hovered): (bool, bool),
    rest: Option<egui::Color32>,
    ring: bool,
) -> egui::Shape {
    let corner_radius = corner_radius.into();
    let fill = if selected {
        Some(theme.item.selected)
    } else if hovered {
        Some(theme.surface.panel_hover)
    } else {
        rest
    };
    let mut shapes: Vec<egui::Shape> = fill
        .map(|fill| egui::Shape::rect_filled(rect, corner_radius, fill))
        .into_iter()
        .collect();
    if selected && ring {
        shapes.push(egui::Shape::Rect(egui::epaint::RectShape::stroke(
            rect,
            corner_radius,
            Stroke::new(size::FIELD_RING, theme.accent.fill),
            StrokeKind::Inside,
        )));
    }
    egui::Shape::Vec(shapes)
}

#[cfg(test)]
mod tests;
