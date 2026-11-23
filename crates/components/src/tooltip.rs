//! Tooltips (C07 `.tip`): cream (`tooltip-bg`) with dark text, 12/16
//! Medium, at most 280 points wide, shown after 500 ms.
//!
//! Attach them with [`TooltipExt`] instead of egui's `on_hover_text`, which
//! paints egui's own popup frame:
//!
//! ```ignore
//! use components::tooltip::{TooltipContent, TooltipExt};
//!
//! status_dot(ui, Tone::Success, "Online").tooltip("Kubelet reports Ready");
//! response.tooltip_content(TooltipContent::Shortcut { label: "Next match", key: "Enter" });
//! response.tooltip_content(TooltipContent::KeyValues(&[("State", "Waiting")]));
//! ```
//!
//! A truncation tooltip repeats text only when it is actually clipped:
//! [`elided_label`] does both. The delay and the grace time between
//! neighbouring tooltips are set globally by the theme ([`configure_style`]).

use std::sync::Arc;

use egui::{Galley, Pos2, Rect, RectAlign, Response, Tooltip, Ui, pos2, vec2};

use crate::design::theme::ThemeExt;
use crate::design::type_scale::{self, TextRole, Weight};
use crate::design::{size, space};
use crate::frames::{show_elevated, tooltip_frame};

/// How long the pointer rests on a control before its tooltip shows.
pub const DELAY: f32 = 0.5;
/// Moving to another tooltipped control within this time shows its tooltip
/// at once (S07).
pub const GRACE_TIME: f32 = 0.3;

/// Tooltip text · 12/16 · 500 (`.tip`).
const TEXT_ROLE: TextRole = type_scale::META.with_weight(Weight::Medium);
/// Keys inside a tooltip · Plex Mono 11/16 (`.tip .k`).
const KEY_ROLE: TextRole = type_scale::MONO.with_size(11.0, 16.0);
/// Gap between a tooltip's words and its shortcut key (`margin-left: 8px`
/// after a space).
const SHORTCUT_GAP: f32 = space::S8;

/// The widest a tooltip's content runs before it wraps.
/// The narrowest a key/value row's value wraps to, however long its key.
const MIN_WRAPPED_SPAN_WIDTH: f32 = 80.0;

fn max_content_width() -> f32 {
    size::TOOLTIP_MAX_WIDTH - 2.0 * size::TOOLTIP_PADDING.x
}

/// Set egui's tooltip timing to the design's, so every tooltip in the app
/// (ours and egui's own) shows after [`DELAY`].
pub(crate) fn configure_style(style: &mut egui::Style) {
    style.interaction.tooltip_delay = DELAY;
    style.interaction.tooltip_grace_time = GRACE_TIME;
}

/// What a tooltip says.
#[derive(Clone, Copy, Debug)]
pub enum TooltipContent<'a> {
    /// A label or explanation, wrapped at the tooltip's width.
    Text(&'a str),
    /// An action's name and its keyboard shortcut ("Next match `Enter`").
    Shortcut {
        /// The action.
        label: &'a str,
        /// The key, set in Plex Mono.
        key: &'a str,
    },
    /// Key/value rows such as a container's status, keys in Plex Mono.
    KeyValues(&'a [(&'a str, &'a str)]),
}

impl TooltipContent<'_> {
    /// Add the content (without the tooltip frame) to `ui`.
    fn add(self, ui: &mut Ui) {
        let color = ui.tokens().components.tooltip_text;
        match self {
            Self::Text(text) => {
                let max_width = max_content_width();
                // CSS lets the space at a line's end hang past the wrap
                // width; egui counts it, so allow for one.
                let space = TEXT_ROLE
                    .galley(ui.ctx(), " ", color, f32::INFINITY)
                    .rect
                    .width();
                let galley = TEXT_ROLE.galley(ui.ctx(), text, color, max_width + space);
                // Like a CSS box with a max-width, a tooltip whose text
                // wraps is as wide as it may be, not as its longest line.
                let width = if galley.rows.len() > 1 {
                    max_width
                } else {
                    galley.rect.width()
                };
                let (rect, response) =
                    type_scale::allocate_text(ui, vec2(width, galley.rect.height()), text);
                type_scale::paint_text(ui, &response, rect.min, galley, color);
            }
            Self::Shortcut { label, key } => {
                InlineRow::new(ui.cursor().min)
                    .text(ui, TEXT_ROLE, &format!("{label} "), 0.0)
                    .key(ui, key, SHORTCUT_GAP)
                    .finish(ui);
            }
            Self::KeyValues(rows) => {
                let mut top = ui.cursor().top();
                for (key, value) in rows {
                    let row = InlineRow::new(pos2(ui.cursor().left(), top))
                        .key(ui, key, 0.0)
                        // The space belongs to the value, in its font.
                        .text(ui, TEXT_ROLE, &format!(" {value}"), 0.0)
                        .finish(ui);
                    top = row.bottom();
                }
            }
        }
    }
}

/// One line of mixed tooltip text on a shared baseline, like CSS inline
/// spans in a 16 point line box.
struct InlineRow {
    origin: Pos2,
    x: f32,
    /// Where a span wraps: the right edge of the tooltip's widest content.
    wrap_right: f32,
    /// The baseline, from the top of the line box.
    baseline: f32,
    bounds: Rect,
}

impl InlineRow {
    fn new(origin: Pos2) -> Self {
        Self {
            origin,
            x: origin.x,
            wrap_right: origin.x + max_content_width(),
            baseline: 0.0,
            bounds: Rect::NOTHING,
        }
    }

    fn key(self, ui: &mut Ui, key: &str, gap: f32) -> Self {
        let color = ui.tokens().components.tooltip_key;
        self.text_colored(ui, KEY_ROLE, key, gap, color)
    }

    fn text(self, ui: &mut Ui, role: TextRole, text: &str, gap: f32) -> Self {
        let color = ui.tokens().components.tooltip_text;
        self.text_colored(ui, role, text, gap, color)
    }

    fn text_colored(
        mut self,
        ui: &mut Ui,
        role: TextRole,
        text: &str,
        gap: f32,
        color: egui::Color32,
    ) -> Self {
        let pixels_per_point = ui.pixels_per_point();
        // The line's baseline is that of the tooltip's own text (the root
        // inline box); every span sits on it.
        if self.bounds == Rect::NOTHING {
            self.baseline = TEXT_ROLE.css_baseline(pixels_per_point);
        }
        // A long span (a container's message) wraps under its own start
        // rather than past the tooltip's max width. CSS lets the space at a
        // line's end hang past the wrap width; egui counts it, so allow for one.
        let space = role
            .galley(ui.ctx(), " ", color, f32::INFINITY)
            .rect
            .width();
        let wrap_width = (self.wrap_right - self.x - gap).max(MIN_WRAPPED_SPAN_WIDTH) + space;
        let galley = role.galley(ui.ctx(), text, color, wrap_width);
        let top = self.origin.y + self.baseline - role.css_baseline(pixels_per_point);
        let rect = Rect::from_min_size(pos2(self.x + gap, top), galley.rect.size());
        paint_span(ui, rect, galley, color);
        self.x = rect.right();
        self.bounds = self.bounds.union(rect);
        self
    }

    /// The row's line box: at least one tooltip line tall.
    fn finish(self, ui: &mut Ui) -> Rect {
        let line = Rect::from_min_size(
            self.origin,
            vec2(self.x - self.origin.x, TEXT_ROLE.line_height()),
        );
        let rect = line.union(self.bounds);
        ui.advance_cursor_after_rect(rect);
        rect
    }
}

fn paint_span(ui: &mut Ui, rect: Rect, galley: Arc<Galley>, color: egui::Color32) {
    let response = type_scale::allocate_text_at(ui, rect, galley.text());
    type_scale::paint_text(ui, &response, rect.min, galley, color);
}

/// Show `content` in a tooltip frame in place, as the design boards draw
/// tooltips next to what they explain.
pub fn show_inline(ui: &mut Ui, content: TooltipContent<'_>) -> Response {
    let theme = ui.tokens();
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
        show_elevated(ui, tooltip_frame(&theme), theme.shadow.tooltip, |ui| {
            content.add(ui)
        })
        .response
    })
    .inner
}

/// Attach design tooltips to a widget's [`Response`].
pub trait TooltipExt: Sized {
    /// Show `text` while the (enabled) widget is hovered.
    fn tooltip(self, text: impl AsRef<str>) -> Self;
    /// Show `text` while the widget is hovered but disabled, saying why.
    fn disabled_tooltip(self, text: impl AsRef<str>) -> Self;
    /// Show rich `content` while the (enabled) widget is hovered.
    fn tooltip_content(self, content: TooltipContent<'_>) -> Self;
    /// Show `content` now, whether or not the widget is hovered (for
    /// example a truncation tooltip the caller decided to show).
    fn show_tooltip(&self, content: TooltipContent<'_>);
}

impl TooltipExt for Response {
    fn tooltip(self, text: impl AsRef<str>) -> Self {
        self.tooltip_content(TooltipContent::Text(text.as_ref()))
    }

    fn disabled_tooltip(self, text: impl AsRef<str>) -> Self {
        show(
            Tooltip::for_disabled(&self),
            TooltipContent::Text(text.as_ref()),
        );
        self
    }

    fn tooltip_content(self, content: TooltipContent<'_>) -> Self {
        show(Tooltip::for_enabled(&self), content);
        self
    }

    fn show_tooltip(&self, content: TooltipContent<'_>) {
        show(Tooltip::for_widget(self), content);
    }
}

fn show(mut tooltip: Tooltip<'_>, content: TooltipContent<'_>) {
    let theme = tooltip.popup.ctx().tokens();
    tooltip.popup = tooltip
        .popup
        .frame(egui::Frame::NONE)
        .align(RectAlign::TOP)
        .gap(size::TOOLTIP_GAP);
    tooltip.show(|ui| {
        ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
        show_elevated(ui, tooltip_frame(&theme), theme.shadow.tooltip, |ui| {
            content.add(ui)
        });
    });
}

/// A single line of `text` in `role` and `color`, elided to the available
/// width; when (and only when) it is clipped, hovering it shows the whole
/// text in a tooltip.
pub fn elided_label(ui: &mut Ui, role: TextRole, text: &str, color: egui::Color32) -> Response {
    let galley = role.galley_elided(ui.ctx(), text, color, ui.available_width().max(0.0), 1);
    let elided = galley.elided;
    let (rect, response) = type_scale::allocate_text(ui, galley.rect.size(), text);
    type_scale::paint_text(ui, &response, rect.min, galley, color);
    if elided {
        response.tooltip(text)
    } else {
        response
    }
}

/// An egui label of `text`, truncated to one line; when (and only when) it
/// is clipped, hovering it shows the whole text in the design's tooltip,
/// never egui's own. `configure` sets the label's other options
/// (selectable, sense).
pub fn truncated_label(
    ui: &mut Ui,
    text: impl Into<egui::WidgetText>,
    configure: impl FnOnce(egui::Label) -> egui::Label,
) -> Response {
    let text = text.into();
    let full_text = text.text().to_owned();
    let galley = text.into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        ui.available_width(),
        egui::FontSelection::Default,
    );
    let elided = galley.elided;
    let response = ui.add(configure(egui::Label::new(galley)).show_tooltip_when_elided(false));
    if elided {
        response.tooltip(full_text)
    } else {
        response
    }
}

#[cfg(test)]
mod tests {
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    use super::*;
    use crate::icons::Icon;
    use crate::test_support::{UiHarnessSnapshot, setup_egui};

    #[test]
    fn a_hovered_control_shows_its_tooltip_above_it() {
        let mut harness = Harness::builder()
            .with_size(egui::vec2(240.0, 120.0))
            .build_ui(|ui| {
                ui.add_space(60.0);
                ui.horizontal(|ui| {
                    ui.add_space(100.0);
                    crate::Button::icon(Icon::Clock, "Timestamps")
                        .tooltip("Show timestamps")
                        .show(ui);
                });
            });
        setup_egui(&mut harness);
        harness.run();
        assert!(harness.query_by_label("Show timestamps").is_none());
        harness.get_by_label("Timestamps").hover();
        harness.run();

        let control = harness.get_by_label("Timestamps").rect();
        let text = harness.get_by_label("Show timestamps").rect();
        let tooltip_bottom = text.bottom() + size::TOOLTIP_PADDING.y;
        assert_eq!(tooltip_bottom + size::TOOLTIP_GAP, control.top());
        // The popup snaps to whole points; the text keeps its fractional width.
        assert!(
            (text.center().x - control.center().x).abs() < 1.0,
            "centred above"
        );
        harness.ui_harness("tooltip/a_hovered_control_shows_its_tooltip_above_it");
    }

    #[test]
    fn rich_tooltips_set_keys_and_shortcuts_in_mono_on_the_text_baseline() {
        let mut harness = Harness::builder().build_ui(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 16.0);
            ui.add_space(8.0);
            ui.horizontal_top(|ui| {
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    show_inline(
                        ui,
                        TooltipContent::KeyValues(&[
                            ("Container", "sidecar"),
                            ("State", "Waiting · ContainerCreating"),
                        ]),
                    );
                    show_inline(
                        ui,
                        TooltipContent::Shortcut {
                            label: "Next match",
                            key: "Enter",
                        },
                    );
                    show_inline(
                        ui,
                        TooltipContent::Text(
                            "A long explanation wraps once it reaches the tooltip's maximum \
                             width of 280 points.",
                        ),
                    );
                });
            });
        });
        setup_egui(&mut harness);
        harness.run();

        let key = harness.get_by_label("Container").rect();
        let value = harness.get_by_label(" sidecar").rect();
        assert_eq!(key.top(), value.top(), "one line box per row");
        let wrapped = harness
            .get_by_label(
                "A long explanation wraps once it reaches the tooltip's maximum width of 280 points.",
            )
            .rect();
        assert!(wrapped.width() <= max_content_width());
        harness.ui_harness(
            "tooltip/rich_tooltips_set_keys_and_shortcuts_in_mono_on_the_text_baseline",
        );
    }

    #[test]
    fn a_long_key_value_wraps_within_the_tooltip_width_under_its_own_start() {
        const MESSAGE: &str = "failed to pull and unpack image \"registry.example/private/api:v2\": \
                               pull access denied, repository does not exist or may require \
                               authorization";
        let mut harness = Harness::builder().build_ui(|ui| {
            ui.add_space(8.0);
            ui.horizontal_top(|ui| {
                ui.add_space(8.0);
                show_inline(
                    ui,
                    TooltipContent::KeyValues(&[("Message", MESSAGE), ("Ready", "No")]),
                );
            });
        });
        setup_egui(&mut harness);
        harness.run();

        let key = harness.get_by_label("Message").rect();
        let message = harness.get_by_label(&format!(" {MESSAGE}")).rect();
        let next_row = harness.get_by_label("Ready").rect();
        assert_eq!(
            message.left(),
            key.right(),
            "the value starts after its key"
        );
        assert!(
            message.right() - key.left() <= max_content_width() + 1.0,
            "the value wraps inside the tooltip's max width"
        );
        assert!(
            message.height() > 2.0 * key.height(),
            "it wraps onto several lines"
        );
        assert!(
            next_row.top() >= message.bottom(),
            "the next row starts below it"
        );
    }

    const LONG_NAME: &str = "kube-controller-manager-kind-control-plane";

    /// How many accessibility nodes read `text` after hovering its elided
    /// label in a column `width` points wide.
    fn nodes_after_hovering(width: f32) -> usize {
        let mut harness = Harness::builder().build_ui(move |ui| {
            ui.set_max_width(width);
            let color = ui.tokens().text.primary;
            elided_label(ui, type_scale::UI, LONG_NAME, color);
        });
        setup_egui(&mut harness);
        harness.run();
        harness.get_by_label(LONG_NAME).hover();
        harness.run();
        harness.get_all_by_label(LONG_NAME).count()
    }

    #[test]
    fn truncation_tooltips_show_only_when_the_text_is_clipped() {
        assert_eq!(
            nodes_after_hovering(120.0),
            2,
            "clipped text explains itself"
        );
        assert_eq!(
            nodes_after_hovering(400.0),
            1,
            "visible text needs no tooltip"
        );
    }
}
