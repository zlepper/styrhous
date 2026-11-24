//! Diagnostics on the code surface (C15): the Problems panel and the
//! tooltip of a diagnostic.
//!
//! The Problems panel sits between the code surface and the status bar: a
//! [`size::PROBLEMS_HEADER`] header (chevron, "Problems", count, collapse)
//! over [`size::PROBLEMS_ROW`] rows (severity icon, "Ln n", message) that
//! scroll past [`size::PROBLEMS_MAX_HEIGHT`]. Clicking a row reports it so
//! the editor can jump to its range; the active row is `control-fill`.
//! Diagnostics that a pending validation may replace are dimmed.
//!
//! Squiggles under diagnostic ranges are painted by the code surface
//! ([`crate::code_surface::paint_squiggle`]).

use egui::{Align, CornerRadius, Layout, Rect, Response, Sense, Ui, UiBuilder, Vec2, pos2, vec2};

use crate::badge::CountBadge;
use crate::button::{Button, ButtonSize, ButtonVariant};
use crate::design::theme::ThemeExt;
use crate::design::type_scale::{self, TextRole, Weight};
use crate::design::{size, space, stroke};
use crate::icons::Icon;
use crate::interaction::{PointingHand, clickable_scope};
use crate::tooltip::TooltipExt;

/// "Problems" in the header (12/16 · 600).
const TITLE: TextRole = type_scale::META.with_weight(Weight::SemiBold);
/// The "Ln n" column: mono 12 in 20 point lines.
const LINE: TextRole = type_scale::MONO_ROW;
/// Gap between the items of the header and of a row.
const GAP: f32 = space::S8;
/// Horizontal padding of the header and rows.
const PADDING_X: f32 = space::S12;
/// Opacity of diagnostics a pending validation may replace.
const STALE_OPACITY: f32 = 0.5;

/// How severe a diagnostic is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

impl Severity {
    fn icon(self) -> Icon {
        match self {
            Self::Error => Icon::XCircle,
            Self::Warning => Icon::ExclamationTriangle,
        }
    }
}

/// One row of the Problems panel.
#[derive(Clone, Copy, Debug)]
pub struct Problem<'a> {
    pub severity: Severity,
    /// The 1-based line, when known.
    pub line: Option<usize>,
    pub message: &'a str,
}

/// What the user did in the Problems panel.
#[derive(Debug, Default)]
pub struct ProblemsResponse {
    /// The row clicked this frame: jump to its range.
    pub clicked: Option<usize>,
}

/// The Problems panel, filling the available width.
#[must_use = "call `show` to add the panel"]
pub struct ProblemsPanel<'p, 'a> {
    id: egui::Id,
    problems: &'p [Problem<'a>],
    active: Option<usize>,
    stale: bool,
    row_tooltip: Option<&'a str>,
    corner_radius: CornerRadius,
    layout: RowLayout,
}

/// The horizontal metrics of the panel's header and rows.
#[derive(Clone, Copy)]
struct RowLayout {
    padding_x: f32,
    line_column: f32,
}

impl<'p, 'a> ProblemsPanel<'p, 'a> {
    pub fn new(
        id_salt: impl std::hash::Hash + std::fmt::Debug,
        problems: &'p [Problem<'a>],
    ) -> Self {
        Self {
            id: egui::Id::new(("problems", id_salt)),
            problems,
            active: None,
            stale: false,
            row_tooltip: None,
            corner_radius: CornerRadius::ZERO,
            layout: RowLayout {
                padding_x: PADDING_X,
                line_column: size::PROBLEMS_LINE_COLUMN,
            },
        }
    }

    /// Highlight the row at `index` (the diagnostic at the caret).
    pub fn active(mut self, index: Option<usize>) -> Self {
        self.active = index;
        self
    }

    /// Dim the rows: a validation is pending and may replace them. Stale
    /// rows cannot be followed to their range; say why with
    /// [`Self::row_tooltip`].
    pub fn stale(mut self, stale: bool) -> Self {
        self.stale = stale;
        self
    }

    /// The tooltip of every row ("Jump to the highlighted YAML location").
    pub fn row_tooltip(mut self, tooltip: &'a str) -> Self {
        self.row_tooltip = Some(tooltip);
        self
    }

    /// Inset the header and rows by `padding_x` and give the "Ln n" column `line_column`
    /// (the editor's panel is roomier than the component board's).
    pub fn spacing(mut self, padding_x: f32, line_column: f32) -> Self {
        self.layout = RowLayout {
            padding_x,
            line_column,
        };
        self
    }

    /// Round the panel (when it is shown as a card of its own).
    pub fn corner_radius(mut self, corner_radius: impl Into<CornerRadius>) -> Self {
        self.corner_radius = corner_radius.into();
        self
    }

    /// Show the panel, open or collapsed to its header as `open` says; the
    /// header and its collapse button toggle it.
    pub fn show(self, ui: &mut Ui, open: &mut bool) -> ProblemsResponse {
        let mut result = ProblemsResponse::default();
        let theme = ui.tokens();
        let width = ui.available_width();
        let rows_height = if *open {
            (self.problems.len() as f32 * size::PROBLEMS_ROW).min(size::PROBLEMS_MAX_HEIGHT)
        } else {
            0.0
        };
        let height = size::PROBLEMS_HEADER + rows_height;
        let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
        if ui.is_rect_visible(rect) {
            ui.painter()
                .rect_filled(rect, self.corner_radius, theme.surface.panel);
        }
        let header = Rect::from_min_size(rect.min, vec2(width, size::PROBLEMS_HEADER));
        if self.show_header(ui, header, *open) {
            *open = !*open;
        }
        if *open {
            let rows = Rect::from_min_max(pos2(rect.left(), header.bottom()), rect.max);
            let mut rows_ui = ui.new_child(
                UiBuilder::new()
                    .id_salt(self.id.with("rows"))
                    .max_rect(rows)
                    .layout(Layout::top_down(Align::Min)),
            );
            rows_ui.spacing_mut().item_spacing = Vec2::ZERO;
            if self.stale {
                rows_ui.multiply_opacity(STALE_OPACITY);
            }
            crate::scroll::vertical()
                .id_salt(self.id.with("scroll"))
                .max_height(size::PROBLEMS_MAX_HEIGHT)
                .auto_shrink([false, true])
                .show(&mut rows_ui, |ui| {
                    for (index, problem) in self.problems.iter().enumerate() {
                        let active = self.active == Some(index);
                        let (mut row, elided) =
                            problem_row(ui, index, problem, active, self.layout);
                        // One tooltip: the full message when elided, then the hint.
                        let tooltip = match (elided, self.row_tooltip) {
                            (true, Some(hint)) => Some(format!("{}\n{hint}", problem.message)),
                            (true, None) => Some(problem.message.to_owned()),
                            (false, hint) => hint.map(str::to_owned),
                        };
                        if let Some(tooltip) = tooltip {
                            row = row.tooltip(tooltip);
                        }
                        if self.stale {
                            row = row.on_hover_cursor(egui::CursorIcon::Default);
                        }
                        if row.clicked() && !self.stale {
                            result.clicked = Some(index);
                        }
                    }
                });
        }
        result
    }

    /// Paint the header; returns whether it asked to toggle the panel.
    fn show_header(&self, ui: &mut Ui, header: Rect, open: bool) -> bool {
        let theme = ui.tokens();
        let corner = CornerRadius {
            sw: 0,
            se: 0,
            ..self.corner_radius
        };
        // The collapse button at the right end of an open panel is not part
        // of the toggle.
        let toggle_right = if open {
            header.right() - self.layout.padding_x - ButtonSize::Sm.height() - GAP
        } else {
            header.right()
        };
        let toggle_rect = Rect::from_min_max(header.min, pos2(toggle_right, header.bottom()));
        let toggle = ui
            .interact(toggle_rect, self.id.with("header"), Sense::click())
            .with_pointing_hand();
        let name = format!("Problems {}", self.problems.len());
        toggle.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::CollapsingHeader, true, open, &name)
        });
        if ui.is_rect_visible(header) {
            ui.painter()
                .rect_filled(header, corner, theme.surface.sunken);
            ui.painter().hline(
                header.x_range(),
                header.bottom() - size::HAIRLINE / 2.0,
                stroke::divider(&theme),
            );
        }
        let content = Rect::from_min_max(
            pos2(header.left() + self.layout.padding_x, header.top()),
            pos2(
                header.right() - self.layout.padding_x,
                header.bottom() - size::HAIRLINE,
            ),
        );
        let mut row = ui.new_child(
            UiBuilder::new()
                .id_salt(self.id.with("header-row"))
                .max_rect(content)
                .layout(Layout::left_to_right(Align::Center)),
        );
        row.spacing_mut().item_spacing = vec2(GAP, 0.0);
        let chevron = if open {
            Icon::ChevronDown
        } else {
            Icon::ChevronRight
        };
        let (icon, _) = row.allocate_exact_size(Vec2::splat(size::ICON_SM), Sense::hover());
        chevron.paint(&row, icon, theme.text.muted);
        crate::card::centred_label(&mut row, TITLE, "Problems", theme.text.primary);
        // The count is part of the header's name ("Problems 100"); the badge
        // only shows it, capped at "99+".
        let badge = CountBadge::new(self.problems.len());
        let (rect, _) = row.allocate_exact_size(badge.size(&row), Sense::hover());
        badge.paint(&row, rect, theme.surface.sunken);
        if !open {
            return toggle.clicked();
        }
        let mut trailing = row.new_child(
            UiBuilder::new()
                .max_rect(content)
                .layout(Layout::right_to_left(Align::Center)),
        );
        let collapse = Button::icon(Icon::XMark, "Collapse")
            .variant(ButtonVariant::Ghost)
            .size(ButtonSize::Sm)
            .show(&mut trailing);
        toggle.clicked() || collapse.clicked()
    }
}

/// One Problems row: a button read as its line and message.
fn problem_row(
    ui: &mut Ui,
    index: usize,
    problem: &Problem<'_>,
    active: bool,
    layout: RowLayout,
) -> (Response, bool) {
    let line = problem
        .line
        .map(|line| format!("Ln {line}"))
        .unwrap_or_default();
    let name = if line.is_empty() {
        problem.message.to_owned()
    } else {
        format!("{line} {}", problem.message)
    };
    let size = vec2(ui.available_width(), size::PROBLEMS_ROW);
    let direction = Layout::left_to_right(Align::Center);
    let inner = clickable_scope(ui, size, direction, &name, |row, _| {
        let rect = row.max_rect();
        problem_row_contents(row, rect, index, problem, &line, active, layout)
    });
    (inner.response, inner.inner)
}

/// The icon, line and message of a Problems row filling `rect`; returns
/// whether the message was elided.
fn problem_row_contents(
    row: &mut Ui,
    rect: Rect,
    index: usize,
    problem: &Problem<'_>,
    line: &str,
    active: bool,
    layout: RowLayout,
) -> bool {
    let theme = row.tokens();
    if active && row.is_rect_visible(rect) {
        row.painter().rect_filled(rect, 0, theme.control.fill);
    }
    row.add_space(layout.padding_x);
    row.spacing_mut().item_spacing = vec2(GAP, 0.0);
    let (icon, _) = row.allocate_exact_size(Vec2::splat(size::ICON_SM), Sense::hover());
    let icon_color = match problem.severity {
        Severity::Error => theme.danger.fill,
        Severity::Warning => theme.warning.fill,
    };
    problem.severity.icon().paint(row, icon, icon_color);
    let column = vec2(layout.line_column, LINE.line_height());
    let (column, _) = row.allocate_exact_size(column, Sense::hover());
    if !line.is_empty() {
        let galley = LINE.galley(row.ctx(), line, theme.text.muted, f32::INFINITY);
        type_scale::text_at(
            row,
            row.id().with(("line", index)),
            column.min,
            galley,
            line,
            theme.text.muted,
        );
    }
    let width = (rect.right() - layout.padding_x - row.cursor().left()).max(0.0);
    let galley =
        type_scale::META.galley_elided(row.ctx(), problem.message, theme.text.primary, width, 1);
    let elided = galley.elided;
    let (text, label) = type_scale::allocate_text(row, galley.rect.size(), problem.message);
    type_scale::paint_text(row, &label, text.min, galley, theme.text.primary);
    elided
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::setup_egui;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    fn problems() -> [Problem<'static>; 2] {
        [
            Problem {
                severity: Severity::Error,
                line: Some(4),
                message: "first",
            },
            Problem {
                severity: Severity::Warning,
                line: None,
                message: "second",
            },
        ]
    }

    fn harness(stale: bool) -> Harness<'static, (Option<usize>, bool)> {
        let mut harness = Harness::new_ui_state(
            move |ui, (clicked, open): &mut (Option<usize>, bool)| {
                let problems = problems();
                let response = ProblemsPanel::new("test", &problems)
                    .stale(stale)
                    .show(ui, open);
                if response.clicked.is_some() {
                    *clicked = response.clicked;
                }
            },
            (None, true),
        );
        setup_egui(&mut harness);
        harness.run();
        harness
    }

    #[test]
    fn clicking_a_row_reports_it_unless_the_rows_are_stale() {
        let mut harness = harness(false);
        harness.get_by_label("Ln 4 first").click();
        harness.run();
        assert_eq!(harness.state().0, Some(0));
        assert_eq!(
            harness.get_by_label("second").rect().height(),
            size::PROBLEMS_ROW
        );

        let mut harness = harness_stale();
        harness.get_by_label("Ln 4 first").click();
        harness.run();
        assert_eq!(harness.state().0, None);
    }

    fn harness_stale() -> Harness<'static, (Option<usize>, bool)> {
        harness(true)
    }

    #[test]
    fn every_row_exposes_its_own_line_label() {
        let mut harness = Harness::new_ui(|ui| {
            let problems = [4, 7, 19].map(|line| Problem {
                severity: Severity::Error,
                line: Some(line),
                message: "message",
            });
            let mut open = true;
            let _ = ProblemsPanel::new("labels", &problems).show(ui, &mut open);
        });
        setup_egui(&mut harness);
        harness.run();
        for line in [4, 7, 19] {
            harness.get_by_label(&format!("Ln {line}"));
        }
    }

    #[test]
    fn the_header_and_its_collapse_button_toggle_the_panel() {
        let mut harness = harness(false);
        harness.get_by_label("Collapse").click();
        harness.run();
        assert!(!harness.state().1);
        assert!(harness.query_by_label("Ln 4 first").is_none());
        harness.get_by_label("Problems 2").click();
        harness.run();
        assert!(harness.state().1);
    }
}
