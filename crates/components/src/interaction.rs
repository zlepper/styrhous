//! Shared interaction affordances for custom egui controls.

use crate::tooltip::TooltipExt;
use egui::{CursorIcon, Response};

/// The tooltip of a control whose action is designed but not built yet.
pub const NOT_IMPLEMENTED: &str = "Not yet implemented";

/// Mark `response` as a designed action that is not built yet: it carries the
/// [`NOT_IMPLEMENTED`] tooltip. The control must be shown without click
/// sense, so it never acts. [`crate::Button::not_implemented`] does both;
/// every unbuilt action goes through this one helper so they stay greppable.
pub fn not_implemented(response: Response) -> Response {
    response.tooltip(NOT_IMPLEMENTED)
}

/// The interaction state a control paints, normally read from its response.
///
/// Controls accept a forced state (`force_state`) so design boards and
/// snapshots can show hover, pressed and focus states side by side.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ControlState {
    /// Neither hovered, pressed nor focused.
    #[default]
    Rest,
    /// The pointer is over the control.
    Hovered,
    /// The primary button is held down on the control.
    Pressed,
    /// The control has keyboard focus (CSS `:focus-visible`).
    Focused,
}

/// Which parts of [`ControlState`] apply to one frame of a control.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Interaction {
    pub hovered: bool,
    pub pressed: bool,
    pub focused: bool,
}

impl Interaction {
    /// The state of `response`, or `forced` when set. A `passive` control
    /// (loading, not implemented) shows no pointer feedback.
    pub fn of(response: &Response, forced: Option<ControlState>, passive: bool) -> Self {
        match forced {
            Some(state) => Self {
                hovered: state == ControlState::Hovered,
                pressed: state == ControlState::Pressed,
                focused: state == ControlState::Focused,
            },
            None => Self {
                hovered: !passive && response.hovered(),
                pressed: !passive && response.is_pointer_button_down_on(),
                focused: response.has_focus(),
            },
        }
    }

    /// `press` while pressed, `hover` while hovered, otherwise `rest`.
    pub fn pick<T>(self, rest: T, hover: T, press: T) -> T {
        if self.pressed {
            press
        } else if self.hovered {
            hover
        } else {
            rest
        }
    }

    /// Whether the pointer is over or pressing the control.
    pub fn is_active(self) -> bool {
        self.hovered || self.pressed
    }
}

/// The tooltips and cursor a control's response carries.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Feedback<'a> {
    /// Shown while the control is enabled.
    pub tooltip: Option<&'a str>,
    /// Shown while it is disabled, falling back to [`Self::tooltip`].
    pub disabled_reason: Option<&'a str>,
    /// Whether a click does something (pointing-hand cursor).
    pub clickable: bool,
    /// The control is a designed action that is not built yet
    /// ([`not_implemented`]).
    pub not_implemented: bool,
}

impl Feedback<'_> {
    /// Attach the cursor and tooltip to `response`.
    pub fn apply(self, response: Response) -> Response {
        if self.not_implemented {
            return not_implemented(response);
        }
        if response.enabled() {
            let response = if self.clickable {
                response.with_pointing_hand()
            } else {
                response
            };
            match self.tooltip {
                Some(tooltip) => response.tooltip(tooltip),
                None => response,
            }
        } else {
            match self.disabled_reason.or(self.tooltip) {
                Some(reason) => response.disabled_tooltip(reason),
                None => response,
            }
        }
    }
}

/// Read a ←/→ key press as a step of -1 or +1 (0 for neither) for a control
/// that moves focus or selection among its own children, and stop egui from
/// also moving focus to the neighbouring widget that frame.
pub(crate) fn arrow_step(ui: &egui::Ui) -> isize {
    step_between(ui, egui::Key::ArrowLeft, egui::Key::ArrowRight)
}

// C19
/// [`arrow_step`] for a vertical list: ↑ steps back, ↓ forward.
pub(crate) fn vertical_arrow_step(ui: &egui::Ui) -> isize {
    step_between(ui, egui::Key::ArrowUp, egui::Key::ArrowDown)
}

fn step_between(ui: &egui::Ui, back: egui::Key, forward: egui::Key) -> isize {
    let step = ui.input_mut(|input| {
        if input.consume_key(egui::Modifiers::NONE, back) {
            -1
        } else if input.consume_key(egui::Modifiers::NONE, forward) {
            1
        } else {
            0
        }
    });
    if step != 0 {
        ui.memory_mut(|memory| memory.move_focus(egui::FocusDirection::None));
    }
    step
}

/// The index `step` places from `from` (or from before the start/after the
/// end when `None`) whose item satisfies `usable`, skipping the rest.
pub(crate) fn step_index(
    from: Option<usize>,
    step: isize,
    len: usize,
    usable: impl Fn(usize) -> bool,
) -> Option<usize> {
    if step == 0 {
        return from;
    }
    let mut candidate = match from {
        Some(index) => index as isize + step,
        None if step > 0 => 0,
        None => len as isize - 1,
    };
    while let Some(index) = usize::try_from(candidate).ok().filter(|index| *index < len) {
        if usable(index) {
            return Some(index);
        }
        candidate += step;
    }
    None
}

/// Consume a press of `key` with `modifiers` like
/// [`egui::InputState::consume_key`], together with the text it typed.
///
/// The platform sends a printable key both as a key event and as the text
/// it types (Space types " ", Alt+R types "r" or "®"); a shortcut on such a
/// key must remove both, or a focused text editor inserts the text.
pub(crate) fn consume_key_with_text(
    input: &mut egui::InputState,
    modifiers: egui::Modifiers,
    key: egui::Key,
) -> bool {
    let Some(index) = input.events.iter().position(|event| {
        matches!(
            event,
            egui::Event::Key { key: pressed_key, pressed: true, modifiers: pressed_modifiers, .. }
                if *pressed_key == key && pressed_modifiers.matches_logically(modifiers)
        )
    }) else {
        return false;
    };
    input.events.remove(index);
    if matches!(input.events.get(index), Some(egui::Event::Text(_))) {
        input.events.remove(index);
    }
    true
}

/// Adds the standard pointing-hand cursor to a clickable response.
pub trait PointingHand {
    /// Mark an enabled clickable response with the standard pointing-hand cursor.
    ///
    /// Disabled egui responses are never hovered, so they retain the platform's
    /// default cursor without additional branching at every call site.
    fn with_pointing_hand(self) -> Response;
}

impl PointingHand for Response {
    fn with_pointing_hand(self) -> Response {
        self.on_hover_cursor(CursorIcon::PointingHand)
    }
}

// C13
/// A button `size` points large at the cursor, allocated through the
/// layout (so it wraps in a wrapping row), whose contents `add_contents`
/// adds in a child `Ui` filling it (laid out with `layout`): the labels
/// inside are the button's accessibility children, and it is read as `name`.
///
/// `add_contents` gets the button's interaction so far (from the previous
/// pass, as egui senses a `Ui`); the returned response has this pass's rect.
/// A `passive` button (stale, disabled) shows no pointing hand.
pub(crate) fn clickable_scope<R>(
    ui: &mut egui::Ui,
    size: egui::Vec2,
    layout: egui::Layout,
    name: &str,
    add_contents: impl FnOnce(&mut egui::Ui, &Response) -> R,
) -> egui::InnerResponse<R> {
    // Allocate first, so a wrapping layout places it; the scope then reuses
    // exactly that rect (advancing the cursor to the same place again).
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let inner = ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(layout)
            .sense(egui::Sense::click()),
        |child| {
            child.set_min_size(rect.size());
            let interaction = child.response();
            add_contents(child, &interaction)
        },
    );
    let response = inner.response.with_pointing_hand();
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), name));
    egui::InnerResponse::new(inner.inner, response)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_button(enabled: bool) -> egui::PlatformOutput {
        let ctx = egui::Context::default();
        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(120.0, 80.0));
        let mut first_output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(screen_rect),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.add_enabled(enabled, egui::Button::new("Action"))
                        .with_pointing_hand();
                });
            },
        );
        first_output.textures_delta.clear();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(screen_rect),
                events: vec![egui::Event::PointerMoved(egui::pos2(16.0, 16.0))],
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.add_enabled(enabled, egui::Button::new("Action"))
                        .with_pointing_hand();
                });
            },
        );
        output.textures_delta.clear();
        output.platform_output
    }

    #[test]
    fn pointing_hand_is_only_emitted_for_enabled_hovered_actions() {
        assert_eq!(render_button(true).cursor_icon, CursorIcon::PointingHand);
        assert_eq!(render_button(false).cursor_icon, CursorIcon::Default);
    }
}

/// The frame in which a text field last held the keyboard focus.
#[derive(Clone, Copy)]
struct TextFocusFrame(u64);

/// Where a viewport's [`TextFocusFrame`] is kept: frame numbers count per
/// viewport, so each keeps its own.
fn text_focus_id(ctx: &egui::Context) -> egui::Id {
    egui::Id::new(("text-focus-frame", ctx.viewport_id()))
}

/// Record that a text field has the focus in this frame, so that
/// [`escape_left_text_field`] can tell what an Esc press left.
pub(crate) fn note_text_focus(ctx: &egui::Context) {
    let frame = TextFocusFrame(ctx.cumulative_frame_nr());
    let id = text_focus_id(ctx);
    ctx.data_mut(|data| data.insert_temp(id, frame));
}

/// Whether Esc is pressed in this frame and a text field had the focus in the
/// one before: egui drops the focus on Esc before any widget runs, so that
/// press only left the field and must not also cancel what holds the field.
pub fn escape_left_text_field(ctx: &egui::Context) -> bool {
    let id = text_focus_id(ctx);
    ctx.input(|input| input.key_pressed(egui::Key::Escape))
        && ctx
            .data(|data| data.get_temp::<TextFocusFrame>(id))
            .is_some_and(|frame| frame.0 + 1 >= ctx.cumulative_frame_nr())
}
