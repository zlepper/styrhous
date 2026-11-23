//! The C15 "Code surface" component board
//! (`docs/design-oracles/harbour/src/C15.html`) rebuilt from the real
//! components, compared with its oracle by `scripts/design-oracles/compare.sh C15`.

use components::design::theme::ThemeExt;
use components::design::{radius, size, stroke};
use components::test_support::board::{
    BoardHeader, board, cell, column, row, section, snapshot_board,
};
use components::tooltip::{self, TooltipContent};
use components::{
    CodeColor, CodeLine, CodeMark, CodeSpan, CodeSurface, Completion, CompletionDoc,
    CompletionExpects, Problem, ProblemsPanel, Severity,
};
use egui::{Align, Layout, Sense, StrokeKind, Ui, UiBuilder, Vec2, vec2};

const HEADER: BoardHeader<'static> = BoardHeader {
    eyebrow: "C15 · components::{CodeSurface, Completion, Diagnostics}",
    title: "Code surface",
    description: "Shared monospaced surface for logs and YAML: 13/20 IBM Plex Mono on \
                  code-surface, 56 px gutter, one-row highlight model. Popovers on the code \
                  surface use the same raised popover as the rest of the app.",
};

const LEFT_WIDTH: f32 = 760.0;
const COLUMN_GAP: f32 = 32.0;
const SECTION_GAP: f32 = 20.0;
/// Vertical padding of the log surfaces.
const LOG_PADDING: f32 = 12.0;
/// The diagnostics panel.
const PANEL_WIDTH: f32 = 640.0;
/// The editor surface.
const EDITOR: Vec2 = vec2(600.0, 380.0);
/// Where the editor's overlays sit, from its top-left corner.
const COMPLETION_AT: Vec2 = vec2(96.0, 166.0);
const DOC_AT: Vec2 = vec2(368.0, 166.0);
const TOOLTIP_AT: Vec2 = vec2(126.0, 96.0);

fn log_line<'a>(
    number: usize,
    time: &'a str,
    level: (&'a str, CodeColor),
    message: Vec<CodeSpan<'a>>,
) -> CodeLine<'a> {
    let mut spans = vec![
        CodeSpan::new(time, CodeColor::Muted),
        CodeSpan::text("  "),
        CodeSpan::new(level.0, level.1),
        CodeSpan::text(" "),
    ];
    spans.extend(message);
    CodeLine::new(spans).number(number)
}

fn log_lines(ui: &mut Ui) {
    section(
        ui,
        "Log lines — gutter, timestamp, level, match, active match, selection, caret, hover, loading row",
        |ui| {
            let info = ("INFO ", CodeColor::Info);
            let lines = [
                log_line(
                    1,
                    "2026-11-22T15:22:18Z",
                    info,
                    vec![CodeSpan::text("server: listening on 0.0.0.0:8080")],
                ),
                log_line(
                    2,
                    "2026-11-22T15:22:18Z",
                    info,
                    vec![
                        CodeSpan::text("http").mark(CodeMark::Match),
                        CodeSpan::text(": GET /healthz 200 2ms"),
                    ],
                ),
                log_line(
                    3,
                    "2026-11-22T15:22:19Z",
                    ("WARN ", CodeColor::Warning),
                    vec![CodeSpan::text(
                        "cache: refreshing stale entry widgets:featured",
                    )],
                )
                .highlighted(true),
                log_line(
                    4,
                    "2026-11-22T15:22:19Z",
                    info,
                    vec![
                        CodeSpan::text("http").mark(CodeMark::ActiveMatch),
                        CodeSpan::text(": POST /v1/widgets 201 38ms"),
                    ],
                )
                .highlighted(true),
                log_line(
                    5,
                    "2026-11-22T15:22:20Z",
                    ("ERROR", CodeColor::Error),
                    vec![
                        CodeSpan::text("worker: job 42 failed: "),
                        CodeSpan::text("context deadline exceeded").mark(CodeMark::Selection),
                    ],
                ),
                {
                    let line = log_line(
                        6,
                        "2026-11-22T15:22:20Z",
                        ("DEBUG", CodeColor::Debug),
                        vec![CodeSpan::text("metrics: flushed 18 samples")],
                    );
                    let end = line.text().chars().count();
                    line.caret(end)
                },
                CodeLine::new([CodeSpan::new("Loading…", CodeColor::Muted)]).number(7),
            ];
            CodeSurface::new(&lines)
                .corner_radius(radius::CARD)
                .padding_bottom(LOG_PADDING)
                .show(ui);
        },
    );
}

fn interleaved_sources(ui: &mut Ui) {
    section(
        ui,
        "Interleaved sources — label column uses a per-source colour",
        |ui| {
            let line = |source: usize, label: &'static str, message: &'static str| {
                CodeLine::new([
                    CodeSpan::new(label, CodeColor::Source(source)),
                    CodeSpan::text(message),
                ])
            };
            let lines = [
                line(0, "api-0     ", " server ready"),
                line(1, "worker-0  ", " job accepted"),
                line(0, "api-0     ", " application-provided timestamp: 15:22:18"),
            ];
            CodeSurface::new(&lines)
                .without_gutter()
                .corner_radius(radius::CARD)
                .padding_bottom(LOG_PADDING)
                .show(ui);
        },
    );
}

fn diagnostics_panel(ui: &mut Ui) {
    section(ui, "Diagnostics panel", |ui| {
        let problems = [
            Problem {
                severity: Severity::Error,
                line: Some(4),
                message: "“settings” is not an allowed value",
            },
            Problem {
                severity: Severity::Warning,
                line: Some(7),
                message: "Unknown field “mode” for ConfigMap.data — values must be strings",
            },
        ];
        let height = size::PROBLEMS_HEADER + 2.0 * size::PROBLEMS_ROW + 2.0 * size::HAIRLINE;
        let (rect, _) = ui.allocate_exact_size(vec2(PANEL_WIDTH, height), Sense::hover());
        let mut inner = ui.new_child(UiBuilder::new().max_rect(rect.shrink(size::HAIRLINE)));
        let mut open = true;
        let _ = ProblemsPanel::new("c15", &problems)
            .active(Some(0))
            .corner_radius(radius::CARD - size::HAIRLINE as u8)
            .show(&mut inner, &mut open);
        ui.painter().rect_stroke(
            rect,
            radius::CARD,
            stroke::divider(&ui.tokens()),
            StrokeKind::Inside,
        );
    });
}

fn editor(ui: &mut Ui) {
    section(
        ui,
        "Editor — syntax, diagnostic underline, error gutter, current line, completion",
        |ui| {
            let key = |text| CodeSpan::new(text, CodeColor::SyntaxKey);
            let colon = || CodeSpan::new(":", CodeColor::SyntaxPunctuation);
            let string = |text| CodeSpan::new(text, CodeColor::SyntaxString);
            let space = |text| CodeSpan::text(text);
            let lines = [
                CodeLine::new([key("apiVersion"), colon(), space(" "), string("v1")]).number(1),
                CodeLine::new([key("kind"), colon(), space(" "), string("ConfigMap")]).number(2),
                CodeLine::new([key("metadata"), colon()]).number(3),
                CodeLine::new([
                    space("  "),
                    key("name"),
                    colon(),
                    space(" "),
                    string("settings").squiggle(),
                ])
                .number(4)
                .error(true),
                CodeLine::new([
                    space("  "),
                    key("namespace"),
                    colon(),
                    space(" "),
                    string("kube-system"),
                ])
                .number(5),
                CodeLine::new([key("data"), colon()]).number(6),
                CodeLine::new([
                    space("  "),
                    key("mode"),
                    colon(),
                    space(" "),
                    string("Read"),
                ])
                .number(7)
                .highlighted(true)
                .caret(12),
            ];
            let surface = cell(ui, EDITOR, Layout::top_down(Align::Min), 0.0, |ui| {
                CodeSurface::new(&lines)
                    .gutter_rule()
                    .corner_radius(radius::CARD)
                    .min_height(EDITOR.y)
                    .show(ui)
                    .rect
            });
            // Popovers and the tooltip float above the surface, as in the app.
            let at = |ui: &mut Ui, offset: Vec2, add: &mut dyn FnMut(&mut Ui)| {
                egui::Area::new(egui::Id::new(("c15-overlay", offset.x as u32)))
                    .order(egui::Order::Foreground)
                    .fixed_pos(surface.min + offset)
                    .show(ui.ctx(), |ui| add(ui));
            };
            at(ui, COMPLETION_AT, &mut |ui| {
                let options = ["ReadOnly", "ReadWrite", "ReadWriteOnce"];
                let _ = Completion::new(CompletionExpects::Value, &options)
                    .type_label("string · enum")
                    .active(Some(0))
                    .show(ui);
            });
            at(ui, DOC_AT, &mut |ui| {
                CompletionDoc::new(
                    "enum",
                    "ReadOnly",
                    "Mount the volume read-only. Writes fail with EROFS.",
                )
                .show(ui);
            });
            at(ui, TOOLTIP_AT, &mut |ui| {
                tooltip::show_inline(
                    ui,
                    TooltipContent::Text(
                        "“settings” is not an allowed value · expected one of: ReadOnly, ReadWrite",
                    ),
                );
            });
        },
    );
}

fn c15_scene(ui: &mut Ui) {
    board(ui, HEADER, |ui| {
        row(ui, 0.0, COLUMN_GAP, Align::Min, |ui| {
            cell(
                ui,
                vec2(LEFT_WIDTH, ui.available_height()),
                Layout::top_down(Align::Min),
                SECTION_GAP,
                |ui| {
                    log_lines(ui);
                    interleaved_sources(ui);
                    diagnostics_panel(ui);
                },
            );
            column(ui, 0.0, editor);
        });
    });
}

#[test]
fn c15_code_surface_board() {
    snapshot_board("c15_code_surface", c15_scene);
}
