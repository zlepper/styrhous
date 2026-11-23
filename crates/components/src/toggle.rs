//! Pressed-state controls (C03): toggle buttons, button groups and the
//! segmented control.
//!
//! All three are pills: a [`ButtonGroup`] and a [`Segmented`] control are a
//! `well-bg` tray ([`well_frame`]) with [`size::TRAY_PADDING`] around
//! children [`size::TRAY_GAP`] apart, sized to their children. A pressed
//! toggle or the selected segment is a cream `pill-on-bg` pill drawn inside
//! its own bounds.
//!
//! ```ignore
//! let views = ButtonGroup::new("View options")
//!     .toggle(ToggleButton::icon(Icon::NumberedList, "Line numbers", numbers))
//!     .toggle(ToggleButton::icon(Icon::Clock, "Timestamps", timestamps))
//!     .show(ui);
//! if views[0].clicked() { numbers = !numbers; }
//!
//! Segmented::new(&mut instance)
//!     .option(Instance::Current, "Current")
//!     .option(Instance::Previous, "Previous")
//!     .show(ui);
//! ```

use egui::accesskit::Role;
use egui::{
    Align, Color32, Layout, Rect, Response, Sense, Ui, UiBuilder, Vec2, WidgetInfo, WidgetType,
    pos2, vec2,
};

use crate::badge::well_frame;
use crate::control::{self, Content, Leading, Look};
use crate::design::theme::{Theme, ThemeExt};
use crate::design::type_scale::{self, TextRole};
use crate::design::{radius, size};
use crate::icons::Icon;
use crate::interaction::{ControlState, Feedback, Interaction, arrow_step, step_index};

/// Seconds the selected segment's thumb takes to move to a new option.
const THUMB_ANIMATION: f32 = 0.12;

/// The size of a tray holding children `widths` wide, `height` tall.
fn tray_size(widths: &[f32], height: f32) -> Vec2 {
    let gaps = size::TRAY_GAP * widths.len().saturating_sub(1) as f32;
    vec2(
        widths.iter().sum::<f32>() + gaps + 2.0 * size::TRAY_PADDING,
        height,
    )
}

/// Allocate a tray for children `widths` wide at the cursor and add them
/// left to right with `add`, in a child exposed to assistive technologies
/// as a group named `label`.
fn tray<R>(
    ui: &mut Ui,
    widths: &[f32],
    height: f32,
    label: &str,
    add: impl FnOnce(&mut Ui) -> R,
) -> (Response, R) {
    let (rect, response) = ui.allocate_exact_size(tray_size(widths, height), Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter().add(well_frame(&ui.tokens()).paint(rect));
    }
    let mut group = ui.new_child(
        UiBuilder::new()
            .max_rect(rect.shrink2(vec2(size::TRAY_PADDING, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    group
        .ctx()
        .accesskit_node_builder(group.unique_id(), |node| {
            node.set_role(Role::Group);
            if !label.is_empty() {
                node.set_label(label);
            }
        });
    group.spacing_mut().item_spacing = vec2(size::TRAY_GAP, 0.0);
    let inner = add(&mut group);
    (response, inner)
}

/// Where a [`ToggleButton`] sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Placement {
    /// On its own (`.tgl-solo`): 32 points on the well.
    Solo,
    /// In a [`ButtonGroup`] (`.tgl`): a 28 point pill on the group's tray.
    InGroup,
}

impl Placement {
    const fn height(self) -> f32 {
        match self {
            Self::Solo => size::CONTROL_MD,
            Self::InGroup => size::CONTROL_SM,
        }
    }

    const fn padding_x(self) -> f32 {
        match self {
            Self::Solo => size::TOGGLE_SOLO_PADDING_X,
            Self::InGroup => size::TOGGLE_PADDING_X,
        }
    }
}

/// A toggle button (`.tgl`, `.tgl-solo`): an icon, a label or both, pressed
/// or not. Clicking it (or Space/Enter while focused) reports a click; flip
/// the state at the call site.
///
/// Shown on its own it is a 32 point standalone toggle on the well; inside a
/// [`ButtonGroup`] it is a 28 point pill. Its tooltip should say what a
/// click does ("Show line numbers" / "Hide line numbers").
#[must_use = "call `show` or add it to a `ButtonGroup`"]
pub struct ToggleButton {
    pressed: Option<bool>,
    icon: Option<Icon>,
    lead: Option<Leading>,
    label: Option<String>,
    name: String,
    tooltip: Option<String>,
    disabled_reason: Option<String>,
    enabled: bool,
    forced_state: Option<ControlState>,
}

impl ToggleButton {
    fn with(pressed: Option<bool>, name: String) -> Self {
        Self {
            pressed,
            icon: None,
            lead: None,
            label: None,
            name,
            tooltip: None,
            disabled_reason: None,
            enabled: true,
            forced_state: None,
        }
    }

    /// An icon-only toggle named `name`, which is also its tooltip.
    pub fn icon(icon: Icon, name: impl Into<String>, pressed: bool) -> Self {
        let name = name.into();
        Self {
            icon: Some(icon),
            tooltip: Some(name.clone()),
            ..Self::with(Some(pressed), name)
        }
    }

    /// A toggle labelled `label`.
    pub fn text(label: impl Into<String>, pressed: bool) -> Self {
        let label = label.into();
        Self {
            label: Some(label.clone()),
            ..Self::with(Some(pressed), label)
        }
    }

    /// A momentary icon action styled like an unpressed toggle, for the
    /// actions in a [`ButtonGroup`] (previous / next line). It has no
    /// pressed state.
    pub fn action(icon: Icon, name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            icon: Some(icon),
            tooltip: Some(name.clone()),
            ..Self::with(None, name)
        }
    }

    /// Lead the label with `icon`.
    pub fn leading_icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    // C16
    /// Lead the label with a status lamp in `tone` (the danger lamp of
    /// "Problems only").
    pub fn leading_dot(mut self, tone: crate::design::theme::Tone) -> Self {
        self.lead = Some(Leading::Dot(tone));
        self
    }

    // C20
    /// Lead the label with a 14 point `icon` (`.ic.s14`, the Follow toggle).
    pub fn leading_small_icon(mut self, icon: Icon) -> Self {
        self.lead = Some(Leading::SmallIcon(icon));
        self
    }

    /// The tooltip, replacing the default (the name of an icon-only toggle).
    pub fn tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    /// The tooltip while the toggle is disabled, saying why ("Matches only"
    /// with an empty query).
    pub fn disabled_reason(mut self, reason: impl Into<String>) -> Self {
        self.disabled_reason = Some(reason.into());
        self
    }

    /// Name the toggle for assistive technologies differently from its label.
    pub fn accessibility_label(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Enable or disable the toggle.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Paint `state` whatever the pointer and keyboard do (design boards and
    /// snapshots).
    pub fn force_state(mut self, state: ControlState) -> Self {
        self.forced_state = Some(state);
        self
    }

    /// Add the toggle on its own (`.tgl-solo`).
    pub fn show(self, ui: &mut Ui) -> Response {
        let content = self.content(ui);
        self.add(ui, content, Placement::Solo)
    }

    /// Add the toggle with its laid-out `content` at `placement`, disabled
    /// when it is (without fading twice in a disabled `ui`).
    fn add(self, ui: &mut Ui, content: Content, placement: Placement) -> Response {
        let enabled = self.enabled;
        ui.add_enabled(
            enabled,
            Placed {
                toggle: self,
                content,
                placement,
            },
        )
    }

    fn content(&self, ui: &Ui) -> Content {
        Content {
            leading: self.lead.or(self.icon.map(Leading::Icon)),
            text: self
                .label
                .as_deref()
                .map(|label| Content::label(ui, type_scale::UI_CONTROL, label)),
            trailing: None,
            gap: size::BUTTON_GAP,
        }
    }

    fn align(content: &Content, placement: Placement) -> control::Align {
        if content.is_icon_only() {
            control::Align::Centred
        } else {
            control::Align::Inset(placement.padding_x())
        }
    }

    /// The fill and content colour in `state`.
    fn look(&self, theme: &Theme, state: Interaction, placement: Placement) -> Look {
        let ring = state.focused.then_some(theme.focus.ring);
        let components = &theme.components;
        if self.pressed == Some(true) {
            let fill = if state.is_active() {
                components.pill_on_bg_hover
            } else {
                components.pill_on_bg
            };
            return Look::pill(fill, components.pill_on_text, ring);
        }
        let rest = match placement {
            Placement::Solo => components.well_bg,
            Placement::InGroup => Color32::TRANSPARENT,
        };
        Look::pill(
            state.pick(rest, theme.control.fill_hover, theme.control.fill_active),
            state.pick(theme.text.secondary, theme.text.primary, theme.text.primary),
            ring,
        )
    }

    fn paint(self, ui: &mut Ui, content: Content, placement: Placement) -> Response {
        let align = Self::align(&content, placement);
        let size = content.control_size(placement.height(), align);
        {
            let (rect, response) = ui.allocate_exact_size(size, Sense::click());
            let enabled = ui.is_enabled();
            response.widget_info(|| match self.pressed {
                Some(pressed) => {
                    WidgetInfo::selected(WidgetType::Button, enabled, pressed, &self.name)
                }
                None => WidgetInfo::labeled(WidgetType::Button, enabled, &self.name),
            });
            let state = Interaction::of(&response, self.forced_state, false);
            let look = self.look(&ui.tokens(), state, placement);
            control::paint(ui, rect, look, &content, align);
            Feedback {
                tooltip: self.tooltip.as_deref(),
                disabled_reason: self.disabled_reason.as_deref(),
                clickable: true,
                not_implemented: false,
            }
            .apply(response)
        }
    }
}

/// A toggle with its laid-out content at its placement, as an egui widget so
/// [`Ui::add_enabled`] can disable it.
struct Placed {
    toggle: ToggleButton,
    content: Content,
    placement: Placement,
}

impl egui::Widget for Placed {
    fn ui(self, ui: &mut Ui) -> Response {
        self.toggle.paint(ui, self.content, self.placement)
    }
}

enum GroupItem {
    Toggle(ToggleButton, Content),
    Separator,
}

impl GroupItem {
    fn width(&self) -> f32 {
        match self {
            Self::Toggle(_, content) => {
                let align = ToggleButton::align(content, Placement::InGroup);
                content.control_size(Placement::InGroup.height(), align).x
            }
            Self::Separator => size::HAIRLINE + 2.0 * size::TRAY_SEPARATOR_MARGIN,
        }
    }
}

/// A button group (`.grp`): toggles and actions in one 32 point tray,
/// sized to its children. Arrow keys move focus between the children.
#[must_use = "call `show` to add the group"]
pub struct ButtonGroup {
    label: String,
    toggles: Vec<ToggleButton>,
    separators: Vec<usize>,
}

impl ButtonGroup {
    /// An empty group named `label` for assistive technologies.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            toggles: Vec::new(),
            separators: Vec::new(),
        }
    }

    /// Add a toggle or action.
    pub fn toggle(mut self, toggle: ToggleButton) -> Self {
        self.toggles.push(toggle);
        self
    }

    /// Separate the following children from the previous ones with a
    /// vertical divider.
    pub fn separator(mut self) -> Self {
        self.separators.push(self.toggles.len());
        self
    }

    /// Add the group. Returns one response per toggle, in order.
    pub fn show(self, ui: &mut Ui) -> Vec<Response> {
        let mut items = Vec::with_capacity(self.toggles.len() + self.separators.len());
        for (index, toggle) in self.toggles.into_iter().enumerate() {
            if self.separators.contains(&index) {
                items.push(GroupItem::Separator);
            }
            let content = toggle.content(ui);
            items.push(GroupItem::Toggle(toggle, content));
        }
        let widths: Vec<f32> = items.iter().map(GroupItem::width).collect();
        let (_, responses) = tray(ui, &widths, size::CONTROL_MD, &self.label, |ui| {
            let mut responses = Vec::new();
            for item in items {
                match item {
                    GroupItem::Toggle(toggle, content) => {
                        responses.push(toggle.add(ui, content, Placement::InGroup));
                    }
                    GroupItem::Separator => {
                        let (rect, _) = ui.allocate_exact_size(
                            vec2(GroupItem::Separator.width(), size::TRAY_SEPARATOR),
                            Sense::hover(),
                        );
                        let line = Rect::from_center_size(
                            rect.center(),
                            vec2(size::HAIRLINE, size::TRAY_SEPARATOR),
                        );
                        ui.painter()
                            .rect_filled(line, 0, ui.tokens().divider.strong);
                    }
                }
            }
            responses
        });
        // ←/→ move focus between the children that can take it.
        if let Some(focused) = responses.iter().position(Response::has_focus) {
            let step = arrow_step(ui);
            if let Some(target) = step_index(Some(focused), step, responses.len(), |index| {
                responses[index].enabled()
            }) {
                responses[target].request_focus();
            }
        }
        responses
    }
}

/// The size of a [`Segmented`] control.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SegmentedSize {
    /// A 28 point tray of 24 point segments with 12 point labels.
    Sm,
    /// A 32 point tray of 28 point segments (the default).
    #[default]
    Md,
}

impl SegmentedSize {
    const fn height(self) -> f32 {
        match self {
            Self::Sm => size::CONTROL_SM,
            Self::Md => size::CONTROL_MD,
        }
    }

    const fn padding_x(self) -> f32 {
        match self {
            Self::Sm => size::SEGMENT_PADDING_X_SM,
            Self::Md => size::SEGMENT_PADDING_X,
        }
    }

    const fn role(self) -> TextRole {
        match self {
            Self::Sm => type_scale::SEGMENT_SM,
            Self::Md => type_scale::UI_CONTROL,
        }
    }
}

struct Segment<T> {
    value: T,
    label: String,
    icon: Option<Icon>,
    disabled_reason: Option<String>,
}

/// A segmented control (`.seg`): two or three mutually exclusive options in
/// a tray; exactly one is selected, shown as a cream thumb that slides to a
/// newly selected option.
///
/// Clicking an option selects it; ←/→ move the selection while the control
/// has focus, and the control is a single tab stop. The returned response
/// is changed when the selection changed.
#[must_use = "call `show` to add the control"]
pub struct Segmented<'a, T> {
    selected: &'a mut T,
    options: Vec<Segment<T>>,
    size: SegmentedSize,
    label: String,
    enabled: bool,
    disabled_reason: Option<String>,
    forced_hover: Option<usize>,
}

impl<'a, T: Clone + PartialEq> Segmented<'a, T> {
    /// A control selecting into `selected`.
    pub fn new(selected: &'a mut T) -> Self {
        Self {
            selected,
            options: Vec::new(),
            size: SegmentedSize::default(),
            label: String::new(),
            enabled: true,
            disabled_reason: None,
            forced_hover: None,
        }
    }

    /// Add an option for `value` labelled `label`.
    pub fn option(self, value: T, label: impl Into<String>) -> Self {
        self.push(value, None, label.into())
    }

    /// Add an option with a leading `icon`.
    pub fn option_with_icon(self, value: T, icon: Icon, label: impl Into<String>) -> Self {
        self.push(value, Some(icon), label.into())
    }

    fn push(mut self, value: T, icon: Option<Icon>, label: String) -> Self {
        self.options.push(Segment {
            value,
            label,
            icon,
            disabled_reason: None,
        });
        self
    }

    /// Disable the last added option, with a tooltip saying why (Previous
    /// when the container never restarted).
    pub fn disable_last(mut self, reason: impl Into<String>) -> Self {
        if let Some(option) = self.options.last_mut() {
            option.disabled_reason = Some(reason.into());
        }
        self
    }

    pub fn size(mut self, size: SegmentedSize) -> Self {
        self.size = size;
        self
    }

    /// Name the control for assistive technologies.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    /// Enable or disable the whole control.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// The tooltip while the whole control is disabled, saying why.
    pub fn disabled_reason(mut self, reason: impl Into<String>) -> Self {
        self.disabled_reason = Some(reason.into());
        self
    }

    /// Paint the option at `index` hovered (design boards and snapshots).
    pub fn force_hover(mut self, index: usize) -> Self {
        self.forced_hover = Some(index);
        self
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        if self.enabled || !ui.is_enabled() {
            self.add(ui)
        } else {
            ui.add_enabled_ui(false, |ui| self.add(ui)).inner
        }
    }

    fn add(self, ui: &mut Ui) -> Response {
        let role = self.size.role();
        let padding_x = self.size.padding_x();
        let item_height = self.size.height() - 2.0 * size::TRAY_PADDING;
        let contents: Vec<Content> = self
            .options
            .iter()
            .map(|option| Content {
                leading: option.icon.map(Leading::Icon),
                text: Some(Content::label(ui, role, &option.label)),
                trailing: None,
                gap: size::BUTTON_GAP,
            })
            .collect();
        let widths: Vec<f32> = contents
            .iter()
            .map(|content| content.width() + 2.0 * padding_x)
            .collect();
        let selected = self
            .options
            .iter()
            .position(|option| option.value == *self.selected);
        let usable = |index: usize| self.options[index].disabled_reason.is_none();
        // The single tab stop: the selected option, else the first usable one.
        let tab_stop = selected
            .filter(|index| usable(*index))
            .or_else(|| step_index(None, 1, self.options.len(), usable));

        let (response, picked) = tray(ui, &widths, self.size.height(), &self.label, |ui| {
            let thumb_id = ui.unique_id().with("segmented-thumb");
            let tray_left = ui.max_rect().left();
            let mut responses = Vec::with_capacity(widths.len());
            for (index, (option, width)) in self.options.iter().zip(&widths).enumerate() {
                let sense = if !usable(index) {
                    Sense::hover()
                } else if tab_stop == Some(index) {
                    Sense::click()
                } else {
                    Sense::CLICK
                };
                let response = ui.allocate_exact_size(vec2(*width, item_height), sense).1;
                let enabled = ui.is_enabled() && usable(index);
                let is_selected = selected == Some(index);
                response.widget_info(|| {
                    WidgetInfo::selected(WidgetType::Button, enabled, is_selected, &option.label)
                });
                responses.push(response);
            }

            let theme = ui.tokens();
            // The thumb slides between options, relative to the tray.
            if let Some(index) = selected {
                let target = responses[index].rect;
                let slide = |edge: &str, x: f32| {
                    if ui.style().animation_time == 0.0 {
                        x
                    } else {
                        ui.ctx().animate_value_with_time(
                            thumb_id.with(edge),
                            x - tray_left,
                            THUMB_ANIMATION,
                        ) + tray_left
                    }
                };
                let left = slide("left", target.left());
                let right = slide("right", target.right());
                let thumb =
                    Rect::from_min_max(pos2(left, target.top()), pos2(right, target.bottom()));
                let opacity = if usable(index) {
                    1.0
                } else {
                    ui.visuals().disabled_alpha()
                };
                control::with_opacity(ui, opacity, |ui| {
                    ui.painter()
                        .rect_filled(thumb, radius::FULL, theme.components.pill_on_bg);
                });
            }

            let mut picked = None;
            for (index, ((option, content), response)) in self
                .options
                .iter()
                .zip(&contents)
                .zip(responses.iter_mut())
                .enumerate()
            {
                let is_selected = selected == Some(index);
                let forced = (self.forced_hover == Some(index)).then_some(ControlState::Hovered);
                let state = Interaction::of(response, forced, !usable(index));
                let ring = state.focused.then_some(theme.focus.ring);
                let look = if is_selected {
                    Look::pill(Color32::TRANSPARENT, theme.components.pill_on_text, ring)
                } else {
                    Look::pill(
                        state.pick(
                            Color32::TRANSPARENT,
                            theme.control.fill_hover,
                            theme.control.fill_active,
                        ),
                        state.pick(theme.text.secondary, theme.text.primary, theme.text.primary),
                        ring,
                    )
                };
                let opacity = if usable(index) {
                    1.0
                } else {
                    ui.visuals().disabled_alpha()
                };
                control::with_opacity(ui, opacity, |ui| {
                    control::paint(
                        ui,
                        response.rect,
                        look,
                        content,
                        control::Align::Inset(padding_x),
                    );
                });
                *response = Feedback {
                    tooltip: option.disabled_reason.as_deref(),
                    disabled_reason: option
                        .disabled_reason
                        .as_deref()
                        .or(self.disabled_reason.as_deref()),
                    clickable: usable(index) && !is_selected,
                    not_implemented: false,
                }
                .apply(response.clone());
                if response.clicked() && !is_selected {
                    picked = Some(index);
                }
            }

            // ←/→ move the selection while an option has focus.
            if responses.iter().any(Response::has_focus) {
                let step = arrow_step(ui);
                if let Some(index) = step_index(selected, step, responses.len(), usable) {
                    if Some(index) != selected {
                        picked = Some(index);
                    }
                    responses[index].request_focus();
                }
            }
            picked
        });

        let mut response = response;
        if let Some(index) = picked {
            *self.selected = self.options[index].value.clone();
            response.mark_changed();
        }
        response
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use super::*;
    use crate::test_support::{UiHarnessSnapshot, setup_egui};
    use egui::accesskit::Toggled;
    use egui_kittest::Harness;
    use egui_kittest::kittest::{NodeT, Queryable};

    fn harness(add: impl FnMut(&mut Ui) + 'static) -> Harness<'static> {
        let mut add = add;
        let mut harness = Harness::builder().build_ui(move |ui| {
            ui.horizontal(|ui| add(ui));
        });
        setup_egui(&mut harness);
        harness.run();
        harness
    }

    fn toggled(harness: &Harness<'_>, label: &str) -> Option<Toggled> {
        harness.get_by_label(label).accesskit_node().toggled()
    }

    #[test]
    fn toggles_report_clicks_and_their_pressed_state() {
        let on = Rc::new(Cell::new(false));
        let state = on.clone();
        let mut harness = harness(move |ui| {
            if ToggleButton::icon(Icon::NumberedList, "Line numbers", state.get())
                .show(ui)
                .clicked()
            {
                state.set(!state.get());
            }
        });
        assert_eq!(toggled(&harness, "Line numbers"), Some(Toggled::False));
        harness.get_by_label("Line numbers").click();
        harness.run();
        assert!(on.get());
        assert_eq!(toggled(&harness, "Line numbers"), Some(Toggled::True));
        let rect = harness.get_by_label("Line numbers").rect();
        assert_eq!(
            rect.size(),
            Vec2::splat(size::CONTROL_MD),
            "standalone toggles are 32 square"
        );
    }

    #[test]
    fn groups_size_to_their_children_and_keep_them_inside() {
        let harness = harness(|ui| {
            ButtonGroup::new("View options")
                .toggle(ToggleButton::icon(Icon::NumberedList, "Line numbers", true))
                .toggle(ToggleButton::icon(Icon::Clock, "Timestamps", false))
                .separator()
                .toggle(ToggleButton::text("Follow", false).leading_icon(Icon::ChevronDoubleDown))
                .toggle(ToggleButton::action(Icon::ArrowUp, "Previous line"))
                .show(ui);
        });
        let numbers = harness.get_by_label("Line numbers").rect();
        let timestamps = harness.get_by_label("Timestamps").rect();
        let follow = harness.get_by_label("Follow").rect();
        let previous = harness.get_by_label("Previous line").rect();
        assert_eq!(numbers.size(), Vec2::splat(size::CONTROL_SM));
        assert_eq!(timestamps.left() - numbers.right(), size::TRAY_GAP);
        assert_eq!(
            follow.left() - timestamps.right(),
            2.0 * size::TRAY_GAP + 2.0 * size::TRAY_SEPARATOR_MARGIN + size::HAIRLINE,
            "a separator sits between sub-groups"
        );
        assert_eq!(previous.left() - follow.right(), size::TRAY_GAP);
        assert_eq!(
            toggled(&harness, "Previous line"),
            None,
            "actions have no pressed state"
        );
        let group = harness.get_by_role_and_label(Role::Group, "View options");
        assert_eq!(group.children().count(), 4);
    }

    #[test]
    fn arrow_keys_step_one_child_at_a_time_and_stay_in_the_group() {
        let mut harness = harness(|ui| {
            ButtonGroup::new("Navigation")
                .toggle(ToggleButton::action(Icon::ArrowUp, "First"))
                .toggle(ToggleButton::action(Icon::ArrowLeft, "Second").enabled(false))
                .toggle(ToggleButton::action(Icon::ArrowRight, "Third"))
                .toggle(ToggleButton::action(Icon::ArrowDown, "Fourth"))
                .show(ui);
            crate::Button::secondary("Neighbour").show(ui);
        });
        let focused = |harness: &Harness<'_>, label: &str| harness.get_by_label(label).is_focused();
        harness.key_press(egui::Key::Tab);
        harness.run();
        assert!(focused(&harness, "First"));
        harness.key_press(egui::Key::ArrowRight);
        harness.run();
        assert!(
            focused(&harness, "Third"),
            "skips the disabled child, one step only"
        );
        harness.key_press(egui::Key::ArrowRight);
        harness.run();
        assert!(focused(&harness, "Fourth"));
        harness.key_press(egui::Key::ArrowRight);
        harness.run();
        assert!(focused(&harness, "Fourth"), "the last child keeps focus");
        harness.key_press(egui::Key::ArrowLeft);
        harness.run();
        assert!(focused(&harness, "Third"));
    }

    #[test]
    fn toggles_activate_from_the_keyboard_and_show_the_focus_ring() {
        let on = Rc::new(Cell::new(false));
        let state = on.clone();
        let mut harness = harness(move |ui| {
            if ToggleButton::text("Follow", state.get())
                .leading_icon(Icon::ChevronDoubleDown)
                .show(ui)
                .clicked()
            {
                state.set(!state.get());
            }
        });
        harness.key_press(egui::Key::Tab);
        harness.run();
        let ring = harness.ctx.tokens().focus.ring;
        let painted_ring = harness.output().shapes.iter().any(|clipped| {
            matches!(&clipped.shape, egui::Shape::Rect(rect) if rect.stroke.color == ring)
        });
        assert!(painted_ring, "keyboard focus shows the focus ring");
        harness.key_press(egui::Key::Space);
        harness.run();
        assert!(on.get());
        harness.key_press(egui::Key::Enter);
        harness.run();
        assert!(!on.get());
    }

    #[test]
    fn disabled_toggles_explain_why_and_keep_their_state() {
        let clicks = Rc::new(Cell::new(0));
        let counter = clicks.clone();
        let mut harness = harness(move |ui| {
            let solo = ToggleButton::text("Matches only", true)
                .leading_icon(Icon::Funnel)
                .enabled(false)
                .disabled_reason("Type a search first")
                .show(ui);
            let grouped = ButtonGroup::new("View options")
                .toggle(ToggleButton::icon(Icon::Clock, "Timestamps", false).enabled(false))
                .show(ui);
            if solo.clicked() || grouped[0].clicked() {
                counter.set(counter.get() + 1);
            }
        });
        harness.get_by_label("Matches only").click();
        harness.get_by_label("Timestamps").click();
        harness.run();
        assert_eq!(clicks.get(), 0);
        let node = harness.get_by_label("Matches only");
        assert!(node.accesskit_node().is_disabled());
        assert_eq!(toggled(&harness, "Matches only"), Some(Toggled::True));
        harness.get_by_label("Matches only").hover();
        harness.run();
        harness.ui_harness("toggles/disabled_toggle_tooltip");
        assert!(harness.query_by_label("Type a search first").is_some());
    }

    #[derive(Clone, Copy, Debug, PartialEq)]
    enum Window {
        One,
        Six,
        Day,
    }

    fn segmented_harness(
        selected: Rc<RefCell<Window>>,
        changes: Rc<Cell<u32>>,
    ) -> Harness<'static> {
        harness(move |ui| {
            let mut value = *selected.borrow();
            if Segmented::new(&mut value)
                .option(Window::One, "1 h")
                .option(Window::Six, "6 h")
                .option(Window::Day, "24 h")
                .disable_last("Only 6 hours of metrics are kept")
                .label("Range")
                .show(ui)
                .changed()
            {
                changes.set(changes.get() + 1);
            }
            *selected.borrow_mut() = value;
            crate::Button::secondary("Export").show(ui);
        })
    }

    #[test]
    fn segments_select_by_click_and_arrows_and_skip_disabled_options() {
        let selected = Rc::new(RefCell::new(Window::Six));
        let changes = Rc::new(Cell::new(0));
        let mut harness = segmented_harness(selected.clone(), changes.clone());
        assert_eq!(toggled(&harness, "6 h"), Some(Toggled::True));

        harness.get_by_label("6 h").click();
        harness.run();
        assert_eq!(
            changes.get(),
            0,
            "clicking the selected option does nothing"
        );

        harness.get_by_label("1 h").click();
        harness.run();
        assert_eq!(*selected.borrow(), Window::One);
        assert_eq!(changes.get(), 1);

        harness.get_by_label("24 h").click();
        harness.run();
        assert_eq!(
            *selected.borrow(),
            Window::One,
            "disabled options cannot be picked"
        );
        assert!(harness.get_by_label("24 h").accesskit_node().is_disabled());

        // One tab stop: the selected option; arrows move the selection.
        harness.key_press(egui::Key::Tab);
        harness.run();
        assert!(harness.get_by_label("1 h").is_focused());
        harness.key_press(egui::Key::ArrowRight);
        harness.run();
        harness.run();
        assert_eq!(*selected.borrow(), Window::Six);
        assert!(harness.get_by_label("6 h").is_focused());
        harness.key_press(egui::Key::ArrowRight);
        harness.run();
        assert_eq!(
            *selected.borrow(),
            Window::Six,
            "the disabled last option is skipped"
        );
        assert!(
            harness.get_by_label("6 h").is_focused(),
            "focus stays in the control instead of moving to Export"
        );
        harness.key_press(egui::Key::ArrowLeft);
        harness.run();
        assert_eq!(*selected.borrow(), Window::One);
        assert!(harness.get_by_label("1 h").is_focused());
        assert_eq!(changes.get(), 3);
    }

    #[test]
    fn neighbouring_segmented_thumbs_settle_on_their_own_options() {
        let picks = Rc::new(RefCell::new((0, 0)));
        let state = picks.clone();
        let mut harness = Harness::builder().build_ui(move |ui| {
            ui.horizontal(|ui| {
                let (mut first, mut second) = *state.borrow();
                Segmented::new(&mut first)
                    .option(0, "A")
                    .option(1, "B")
                    .show(ui);
                Segmented::new(&mut second)
                    .option(0, "C")
                    .option(1, "D")
                    .show(ui);
                *state.borrow_mut() = (first, second);
            });
        });
        setup_egui(&mut harness);
        // Keep egui's animations on so the thumbs slide.
        harness
            .ctx
            .global_style_mut(|style| style.animation_time = egui::Style::default().animation_time);
        harness.run();
        harness.get_by_label("B").click();
        harness.run();
        assert!(
            harness.run_ok().is_some(),
            "both thumbs settle instead of retargeting each other"
        );
        let pill = harness.ctx.tokens().components.pill_on_bg;
        let thumbs: Vec<Rect> = harness
            .output()
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Rect(rect) if rect.fill == pill => Some(rect.rect),
                _ => None,
            })
            .collect();
        let b = harness.get_by_label("B").rect();
        let c = harness.get_by_label("C").rect();
        assert_eq!(thumbs, [b, c]);
    }

    #[test]
    fn hovered_toggles_and_segments_by_real_interaction() {
        let mut harness = Harness::builder().build_ui(|ui| {
            ui.horizontal(|ui| {
                ButtonGroup::new("View options")
                    .toggle(ToggleButton::icon(Icon::NumberedList, "Line numbers", true))
                    .toggle(ToggleButton::icon(Icon::Clock, "Timestamps", false))
                    .show(ui);
                ToggleButton::text("Follow", false)
                    .leading_icon(Icon::ChevronDoubleDown)
                    .show(ui);
                Segmented::new(&mut 0)
                    .option(0, "Current")
                    .option(1, "Previous")
                    .show(ui);
            });
        });
        setup_egui(&mut harness);
        harness.run();
        harness.get_by_label("Timestamps").hover();
        harness.run();
        harness.ui_harness("toggles/hovered_group_toggle");
        harness.get_by_label("Previous").hover();
        harness.run();
        harness.ui_harness("toggles/hovered_segment");
    }
}
