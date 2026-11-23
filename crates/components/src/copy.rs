//! Click-to-copy controls: the shared behaviour of [`crate::Copyable`]
//! values, copyable [`crate::Chip`]s and the Copy button of a
//! [`crate::CodeBlock`].
//!
//! A copy control is one button whose painted contents (the value's label)
//! are its accessibility children. A click copies its text and confirms
//! with a "Copied" tooltip for [`COPIED_SECONDS`]; while that shows, the
//! control keeps its hover look.

use egui::{Align, Id, Layout, Rect, Response, Ui, Vec2, pos2};

use crate::design::size;
use crate::design::theme::ThemeExt;
use crate::icons::Icon;
use crate::interaction::{ControlState, Interaction, clickable_scope};
use crate::tooltip::{TooltipContent, TooltipExt};

/// Seconds the "Copied" confirmation stays up.
pub(crate) const COPIED_SECONDS: f64 = 1.5;
/// The confirmation shown after copying.
pub(crate) const COPIED: &str = "Copied";

/// Where a copy control remembers when it last copied. Keyed by what it
/// copies, so the confirmation stays with its value when a list reorders.
fn copied_id(ui: &Ui, name: &str, text: &str) -> Id {
    ui.id().with(("copied", name, text))
}

/// When the control under `id` copied, if its confirmation still shows.
fn showing_since(ui: &Ui, id: Id) -> Option<f64> {
    let now = ui.input(|input| input.time);
    ui.data(|data| data.get_temp::<f64>(id))
        .filter(|at| now - at < COPIED_SECONDS)
}

/// Copy `text` when `response` was clicked and show the "Copied"
/// confirmation while it is fresh; returns whether it shows. `name` is the
/// control's accessible name, which with `text` keys its state.
pub(crate) fn confirm_copy(ui: &Ui, response: &Response, name: &str, text: &str) -> bool {
    let id = copied_id(ui, name, text);
    let now = ui.input(|input| input.time);
    if response.clicked() {
        ui.ctx().copy_text(text.to_owned());
        ui.data_mut(|data| data.insert_temp(id, now));
    }
    match showing_since(ui, id) {
        Some(at) => {
            response.show_tooltip(TooltipContent::Text(COPIED));
            ui.ctx()
                .request_repaint_after_secs((COPIED_SECONDS - (now - at)) as f32);
            true
        }
        None => {
            ui.data_mut(|data| data.remove::<f64>(id));
            false
        }
    }
}

/// A copy control `size` points large at the cursor, read as the button
/// `name`, copying `text` when clicked.
///
/// `paint` adds the contents into the control's rect; it gets whether to
/// paint the hover look (hovered, `forced` to hover, or confirming a copy)
/// and returns the full text to show as a tooltip when it had to elide the
/// value.
pub(crate) fn copy_button(
    ui: &mut Ui,
    size: Vec2,
    name: &str,
    text: &str,
    forced: Option<ControlState>,
    paint: impl FnOnce(&mut Ui, Rect, bool) -> Option<String>,
) -> Response {
    let showing = showing_since(ui, copied_id(ui, name, text)).is_some();
    let inner = clickable_scope(
        ui,
        size,
        Layout::left_to_right(Align::Center),
        name,
        |ui, interaction| {
            let active = Interaction::of(interaction, forced, false).hovered || showing;
            let rect = ui.max_rect();
            paint(ui, rect, active)
        },
    );
    let response = inner.response;
    let showing = confirm_copy(ui, &response, name, text);
    match inner.inner {
        Some(full_text) if !showing => response.tooltip(full_text),
        _ => response,
    }
}

/// Paint the copy icon of a hovered copy control at the right end of the
/// first `line_height` points of `rect`, `inset` points in from its edge.
pub(crate) fn paint_copy_icon(ui: &Ui, rect: Rect, inset: f32, line_height: f32) {
    let icon = Rect::from_min_size(
        pos2(
            rect.right() - inset - size::ICON_SM,
            rect.top() + (line_height - size::ICON_SM) / 2.0,
        ),
        Vec2::splat(size::ICON_SM),
    );
    Icon::ClipboardDocument.paint(ui, icon, ui.tokens().text.secondary);
}
