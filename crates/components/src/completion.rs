//! Completion on the code surface (C15): the raised popover of schema
//! suggestions and the documentation popover beside it.
//!
//! The completion popover is [`size::COMPLETION_WIDTH`] wide: a header
//! "Expects value · type", mono 12 option rows [`size::COMPLETION_ROW`]
//! tall (the highlighted one on `accent-tint` in `accent-text`) and a
//! key-hint footer. Its documentation sits beside it with a kind badge, the
//! name and a description. [`completion_position`] places it below the
//! caret line, above it in the bottom third of the surface, shifted left to
//! stay inside it.

use egui::{Rect, Response, Sense, Ui, Vec2, pos2, vec2};

use crate::badge::small_kind_badge;
use crate::design::theme::{self, ThemeExt, ThemeScope};
use crate::design::type_scale::{self, TextRole};
use crate::design::{radius, size, space};
use crate::frames::{POPOVER_PADDING, show_anchored_popover};
use crate::popover_footer::popover_footer;
use crate::tooltip::TooltipExt;

/// Option rows and the documentation's name: mono 12 in 20 point lines.
const OPTION: TextRole = type_scale::MONO_ROW;
/// The documentation's name: mono 12/20 · 500.
const DOC_NAME: TextRole = type_scale::MONO_ROW_STRONG;
/// Gap between the header's words.
const HEADER_GAP: f32 = space::S6;
/// Gap between the documentation's badge and name.
const DOC_GAP: f32 = space::S8;
/// Gap between the documentation's name row and its description.
const DOC_DESCRIPTION_GAP: f32 = space::S6;

/// What the completion offers at the caret.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompletionExpects {
    /// A mapping key: "Expects key".
    Key,
    /// A value: "Expects value".
    Value,
    /// The schema does not say: "Suggestions".
    Anything,
}

impl CompletionExpects {
    fn text(self) -> &'static str {
        match self {
            Self::Key => "Expects key",
            Self::Value => "Expects value",
            Self::Anything => "Suggestions",
        }
    }
}

/// What the user did in a [`Completion`].
#[derive(Debug)]
pub struct CompletionResponse {
    /// The option clicked this frame.
    pub clicked: Option<usize>,
    /// The option under the pointer and its row.
    pub hovered: Option<(usize, Rect)>,
    /// The highlighted option's row, for placing its documentation.
    pub active_row: Option<Rect>,
    /// The popover.
    pub rect: Rect,
}

/// The completion popover, added at the cursor (put it in an
/// [`egui::Area`] at [`completion_position`]).
#[must_use = "call `show` to add the popover"]
pub struct Completion<'a> {
    expects: CompletionExpects,
    type_label: Option<&'a str>,
    options: &'a [&'a str],
    active: Option<usize>,
    id: egui::Id,
    max_rows_height: f32,
    scroll_to_active: bool,
    option_label: Option<&'a str>,
}

impl<'a> Completion<'a> {
    pub fn new(expects: CompletionExpects, options: &'a [&'a str]) -> Self {
        Self {
            expects,
            type_label: None,
            options,
            active: None,
            id: egui::Id::new("completion"),
            max_rows_height: f32::INFINITY,
            scroll_to_active: false,
            option_label: None,
        }
    }

    /// Distinguish this popover's scroll state from other completions.
    pub fn id_salt(mut self, id_salt: impl std::hash::Hash + std::fmt::Debug) -> Self {
        self.id = egui::Id::new(("completion", id_salt));
        self
    }

    /// Scroll the options past `height` points.
    pub fn max_rows_height(mut self, height: f32) -> Self {
        self.max_rows_height = height;
        self
    }

    /// Read each option as "`label`: option" (what choosing it does, such as
    /// "Insert YAML completion").
    pub fn option_label(mut self, label: &'a str) -> Self {
        self.option_label = Some(label);
        self
    }

    /// Scroll the highlighted option into view (after the keyboard moved it).
    pub fn scroll_to_active(mut self, scroll: bool) -> Self {
        self.scroll_to_active = scroll;
        self
    }

    /// The schema type after the header: "string · enum".
    pub fn type_label(mut self, type_label: &'a str) -> Self {
        self.type_label = Some(type_label);
        self
    }

    /// Highlight the option at `index`.
    pub fn active(mut self, index: Option<usize>) -> Self {
        self.active = index;
        self
    }

    /// The popover's height for `options` rows, their area at most
    /// `max_rows_height` tall.
    pub fn height(options: usize, max_rows_height: f32) -> f32 {
        let header = header_height();
        let rows = (options.max(1) as f32 * size::COMPLETION_ROW).min(max_rows_height);
        2.0 * POPOVER_PADDING + header + rows + size::POPOVER_FOOTER
    }

    pub fn show(self, ui: &mut Ui) -> CompletionResponse {
        let mut result = CompletionResponse {
            clicked: None,
            hovered: None,
            active_row: None,
            rect: Rect::NOTHING,
        };
        let frame = popover(ui, size::COMPLETION_WIDTH, |ui| {
            let theme = ui.tokens();
            // Header: caption and type, padded 4 8 6 inside the popover.
            let (header, _) =
                ui.allocate_exact_size(vec2(ui.available_width(), header_height()), Sense::hover());
            let mut x = header.left() + space::S8;
            let top = header.top() + space::S4;
            let caption = type_scale::CAPTION.galley(
                ui.ctx(),
                self.expects.text(),
                theme.accent.text,
                f32::INFINITY,
            );
            let caption_width = caption.rect.width();
            type_scale::text_at(
                ui,
                ui.id().with("expects"),
                pos2(x, top),
                caption,
                self.expects.text(),
                theme.accent.text,
            );
            x += caption_width + HEADER_GAP;
            if let Some(type_label) = self.type_label {
                let galley =
                    type_scale::META.galley(ui.ctx(), type_label, theme.text.muted, f32::INFINITY);
                type_scale::text_at(
                    ui,
                    ui.id().with("type"),
                    pos2(x, top),
                    galley,
                    type_label,
                    theme.text.muted,
                );
            }

            if self.options.is_empty() {
                let (row, _) = ui.allocate_exact_size(
                    vec2(ui.available_width(), size::COMPLETION_ROW),
                    Sense::hover(),
                );
                let galley = type_scale::UI.galley(
                    ui.ctx(),
                    "No completions",
                    theme.text.muted,
                    f32::INFINITY,
                );
                let pos = pos2(
                    row.left() + size::OPTION_PADDING_X,
                    row.center().y - galley.rect.height() / 2.0,
                );
                type_scale::text_at(
                    ui,
                    ui.id().with("empty"),
                    pos,
                    galley,
                    "No completions",
                    theme.text.muted,
                );
            } else {
                crate::scroll::vertical()
                    .id_salt(self.id.with("options"))
                    .max_height(self.max_rows_height)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = Vec2::ZERO;
                        for (index, option) in self.options.iter().enumerate() {
                            let active = self.active == Some(index);
                            let name = match self.option_label {
                                Some(prefix) => format!("{prefix}: {option}"),
                                None => (*option).to_owned(),
                            };
                            let (row, response) = option_row(ui, option, &name, active);
                            if response.clicked() {
                                result.clicked = Some(index);
                            }
                            if response.hovered() {
                                result.hovered = Some((index, row));
                            }
                            if active {
                                if self.scroll_to_active {
                                    response.scroll_to_me(Some(egui::Align::Center));
                                }
                                result.active_row = Some(row);
                            }
                        }
                    });
            }
            let hints: &[(&[&str], &str)] = if self.options.is_empty() {
                &[(&["Esc"], "dismiss")]
            } else {
                &[
                    (&["↑", "↓"], "move"),
                    (&["Enter"], "apply"),
                    (&["Esc"], "dismiss"),
                ]
            };
            popover_footer(ui, hints);
        });
        result.rect = frame;
        result
    }
}

fn header_height() -> f32 {
    space::S4 + type_scale::CAPTION.line_height() + space::S6
}

/// One option row: mono text on the highlight when `active`.
fn option_row(ui: &mut Ui, option: &str, name: &str, active: bool) -> (Rect, Response) {
    let theme = ui.tokens();
    let (row, response) = ui.allocate_exact_size(
        vec2(ui.available_width(), size::COMPLETION_ROW),
        Sense::click(),
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, active, name)
    });
    let color = if active {
        ui.painter()
            .rect_filled(row, radius::OPTION, theme.accent.tint);
        theme.accent.text
    } else {
        theme.text.primary
    };
    let width = row.width() - 2.0 * size::OPTION_PADDING_X;
    let galley = OPTION.galley_elided(ui.ctx(), option, color, width, 1);
    let elided = galley.elided;
    let pos = pos2(
        row.left() + size::OPTION_PADDING_X,
        row.center().y - galley.rect.height() / 2.0,
    );
    type_scale::paint_text_as(
        ui,
        &response,
        pos,
        galley,
        color,
        egui::accesskit::Role::ListBoxOption,
    );
    let response = if elided {
        response.tooltip(option)
    } else {
        response
    };
    (row, response)
}

/// The documentation popover of a completion option.
#[must_use = "call `show` to add the popover"]
pub struct CompletionDoc<'a> {
    kind: &'a str,
    name: &'a str,
    description: &'a str,
}

impl<'a> CompletionDoc<'a> {
    /// `name`, a `kind` badge ("enum", "field") and its `description`.
    pub fn new(kind: &'a str, name: &'a str, description: &'a str) -> Self {
        Self {
            kind,
            name,
            description,
        }
    }

    pub fn show(self, ui: &mut Ui) -> Rect {
        popover(ui, size::COMPLETION_DOC_WIDTH, |ui| {
            let theme = ui.tokens();
            egui::Frame::new()
                .inner_margin(space::S8 as i8)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = DOC_GAP;
                        ui.set_height(OPTION.line_height());
                        small_kind_badge(ui, self.kind);
                        DOC_NAME.label(ui, self.name, theme.text.primary);
                    });
                    ui.add_space(DOC_DESCRIPTION_GAP);
                    type_scale::META.label(ui, self.description, theme.text.secondary);
                });
        })
    }
}

/// A raised popover `width` wide at the cursor, its contents in
/// [`ThemeScope::Popover`] with zero item spacing; returns its rect.
fn popover(ui: &mut Ui, width: f32, add_contents: impl FnOnce(&mut Ui)) -> Rect {
    let theme = ui.tokens();
    show_anchored_popover(ui, &theme, |ui| {
        theme::scope(ui, ThemeScope::Popover, |ui| {
            ui.set_width(width - 2.0 * POPOVER_PADDING);
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            add_contents(ui);
        });
    })
    .response
    .rect
}

/// Where to put a completion popover of `popover` size for a caret at
/// `caret` on a code `surface`: below the caret line, above it when the
/// caret is in the bottom third or there is no room below, shifted to stay
/// inside the surface.
pub fn completion_position(surface: Rect, caret: Rect, popover: Vec2) -> egui::Pos2 {
    let line_bottom = caret.bottom() + size::CODE_CARET_INSET;
    let line_top = caret.top() - size::CODE_CARET_INSET;
    let below = line_bottom + size::POPOVER_GAP;
    let in_bottom_third = caret.center().y > surface.top() + surface.height() * 2.0 / 3.0;
    let fits_below = below + popover.y <= surface.bottom();
    let y = if in_bottom_third || !fits_below {
        line_top - size::POPOVER_GAP - popover.y
    } else {
        below
    };
    let clamp = |value: f32, min: f32, max: f32| value.min(max).max(min);
    pos2(
        clamp(caret.left(), surface.left(), surface.right() - popover.x),
        clamp(y, surface.top(), surface.bottom() - popover.y),
    )
}

/// Where to put the documentation popover of `doc` size beside a completion
/// popover at `completion`: to its right, or to its left when there is no
/// room inside `surface`; top-aligned with it.
pub fn completion_doc_position(surface: Rect, completion: Rect, doc: Vec2) -> egui::Pos2 {
    let right = completion.right() + size::COMPLETION_DOC_GAP;
    let x = if right + doc.x <= surface.right() {
        right
    } else {
        completion.left() - size::COMPLETION_DOC_GAP - doc.x
    };
    pos2(x, completion.top())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn surface() -> Rect {
        Rect::from_min_size(pos2(0.0, 0.0), vec2(600.0, 380.0))
    }

    fn caret(x: f32, y: f32) -> Rect {
        Rect::from_min_size(pos2(x, y + size::CODE_CARET_INSET), size::CODE_CARET)
    }

    #[test]
    fn completion_opens_below_the_caret_line_and_flips_in_the_bottom_third() {
        let popover = vec2(size::COMPLETION_WIDTH, 150.0);
        let below = completion_position(surface(), caret(100.0, 132.0), popover);
        assert_eq!(below, pos2(100.0, 152.0 + size::POPOVER_GAP));
        let above = completion_position(surface(), caret(100.0, 300.0), popover);
        assert_eq!(above.y, 300.0 - size::POPOVER_GAP - 150.0);
    }

    #[test]
    fn completion_shifts_left_to_stay_inside_the_surface() {
        let popover = vec2(size::COMPLETION_WIDTH, 150.0);
        for (x, y) in [(590.0, 20.0), (590.0, 360.0), (0.0, 20.0), (0.0, 360.0)] {
            let pos = completion_position(surface(), caret(x, y), popover);
            assert!(
                pos.x >= 0.0 && pos.x + popover.x <= 600.0,
                "corner ({x}, {y})"
            );
        }
    }

    #[test]
    fn the_option_rows_scroll_past_their_maximum_height() {
        let chrome = Completion::height(0, f32::INFINITY) - size::COMPLETION_ROW;
        assert_eq!(
            Completion::height(3, f32::INFINITY),
            chrome + 3.0 * size::COMPLETION_ROW
        );
        assert_eq!(Completion::height(128, 260.0), chrome + 260.0);
    }

    #[test]
    fn documentation_sits_right_of_the_completion_or_left_without_room() {
        let doc = vec2(size::COMPLETION_DOC_WIDTH, 82.0);
        let left = Rect::from_min_size(pos2(40.0, 10.0), vec2(size::COMPLETION_WIDTH, 150.0));
        assert_eq!(
            completion_doc_position(surface(), left, doc).x,
            left.right() + size::COMPLETION_DOC_GAP
        );
        let right = Rect::from_min_size(pos2(330.0, 10.0), vec2(size::COMPLETION_WIDTH, 150.0));
        assert_eq!(
            completion_doc_position(surface(), right, doc).x,
            right.left() - size::COMPLETION_DOC_GAP - doc.x
        );
    }

    #[test]
    fn completion_reports_hover_and_click_and_elides_long_options() {
        use crate::test_support::setup_egui;
        use egui_kittest::Harness;
        use egui_kittest::kittest::Queryable;
        let long = "a-very-long-option-name-that-cannot-fit-the-completion-popover-width";
        let mut harness = Harness::new_ui_state(
            move |ui, seen: &mut (Option<usize>, Option<usize>)| {
                let options = ["ReadOnly", "ReadWrite", long];
                let response = Completion::new(CompletionExpects::Value, &options)
                    .active(Some(0))
                    .option_label("Insert")
                    .show(ui);
                if let Some((index, _)) = response.hovered {
                    seen.0 = Some(index);
                }
                if response.clicked.is_some() {
                    seen.1 = response.clicked;
                }
            },
            (None, None),
        );
        setup_egui(&mut harness);
        harness.run();
        harness.get_by_label("Insert: ReadWrite").hover();
        harness.run();
        assert_eq!(harness.state().0, Some(1));
        harness.get_by_label(&format!("Insert: {long}")).click();
        harness.run();
        assert_eq!(harness.state().1, Some(2));
        harness.get_by_label(&format!("Insert: {long}")).hover();
        harness.run();
        assert!(
            harness.query_by_label(long).is_some(),
            "the full option is the tooltip"
        );
    }
}
