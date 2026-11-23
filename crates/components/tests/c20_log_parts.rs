//! The C20 "Log viewer parts" component board
//! (`docs/design-oracles/harbour/src/C20.html`) rebuilt from the real
//! components, compared with its oracle by
//! `scripts/design-oracles/compare.sh C20`.

use components::design::theme::{ThemeExt, Tone};
use components::design::{radius, size, space};
use components::icons::Icon;
use components::test_support::board::{
    BoardHeader, Run, WIDE_DESCRIPTION, board_with_description_width, placed, row, rules_list,
    section_caption, snapshot_board,
};
use components::{
    CodeMark, CodeSpan, ControlState, FloatingToolbar, LevelBadge, LogHeader, LogLevel, LogLine,
    LogStream, SearchField, SearchStatus, Segmented, SourcePill, ToggleButton, VolumeBar,
    VolumeBars, VolumeStrip,
};
use egui::{Align, Ui, pos2, vec2};

const HEADER: BoardHeader<'static> = BoardHeader {
    eyebrow: "C20 · components::{LogHeader, VolumeStrip, SourcePill, LogLine, FloatingToolbar}",
    title: "Log viewer parts",
    description: "The log window reads top-down: what you are looking at, how busy it was, then \
                  the lines. Every line carries its source and level as pills so interleaved \
                  replicas stay apart; tools float at the bottom, out of the text.",
};

const LEFT_WIDTH: f32 = 860.0;
const RIGHT_LEFT: f32 = 968.0;
const RIGHT_WIDTH: f32 = 520.0;
/// Space above each caption after the first (`margin-top: 22px`).
const SECTION_GAP: f32 = 22.0;
/// Padding above and below the lines of the log panel (`padding: 10px 0`).
const PANEL_PADDING: f32 = 10.0;
/// The search of the floating toolbar.
const TOOLBAR_SEARCH_WIDTH: f32 = 300.0;
/// The visible window of the volume strip, in bars.
const WINDOW: (f32, f32) = (80.0, 20.0);
/// Gap between the badges and pills of the right column (`.row` gap 8).
const PILL_GAP: f32 = space::S8;

/// The bars of the volume strip: height in points (of 36) and tone.
const BARS: [(f32, Option<Tone>); 100] = [
    (13.0, None),
    (15.0, None),
    (9.0, None),
    (18.0, None),
    (21.0, None),
    (10.0, None),
    (8.0, None),
    (8.0, None),
    (6.0, None),
    (18.0, None),
    (15.0, None),
    (7.0, None),
    (13.0, None),
    (22.0, None),
    (17.0, None),
    (14.0, None),
    (11.0, None),
    (9.0, None),
    (14.0, None),
    (12.0, None),
    (6.0, None),
    (14.0, None),
    (14.0, None),
    (12.0, None),
    (11.0, None),
    (15.0, None),
    (15.0, None),
    (17.0, None),
    (8.0, None),
    (16.0, None),
    (18.0, None),
    (22.0, None),
    (13.0, None),
    (11.0, None),
    (13.0, None),
    (21.0, None),
    (14.0, None),
    (8.0, None),
    (15.0, None),
    (6.0, None),
    (25.0, Some(Tone::Danger)),
    (25.0, Some(Tone::Danger)),
    (22.0, None),
    (12.0, None),
    (19.0, None),
    (19.0, None),
    (15.0, None),
    (19.0, None),
    (20.0, None),
    (11.0, None),
    (13.0, None),
    (15.0, None),
    (14.0, None),
    (7.0, None),
    (8.0, None),
    (7.0, None),
    (20.0, None),
    (14.0, None),
    (22.0, None),
    (21.0, None),
    (26.0, Some(Tone::Warning)),
    (10.0, None),
    (12.0, None),
    (8.0, None),
    (19.0, None),
    (12.0, None),
    (20.0, None),
    (14.0, None),
    (11.0, None),
    (17.0, None),
    (19.0, None),
    (16.0, None),
    (12.0, None),
    (16.0, None),
    (9.0, None),
    (7.0, None),
    (13.0, None),
    (24.0, Some(Tone::Danger)),
    (13.0, None),
    (9.0, None),
    (16.0, None),
    (11.0, None),
    (15.0, None),
    (20.0, None),
    (6.0, None),
    (7.0, None),
    (17.0, None),
    (8.0, None),
    (15.0, None),
    (16.0, None),
    (6.0, None),
    (16.0, None),
    (15.0, None),
    (16.0, None),
    (10.0, None),
    (19.0, None),
    (8.0, None),
    (15.0, None),
    (12.0, None),
    (20.0, None),
];

const RULES: [&[Run]; 5] = [
    &[Run::Text(
        "Source colours come from category / log-source roles in a stable order per pod; the \
         same replica keeps its colour across windows.",
    )],
    &[Run::Text(
        "Matches are underlined in accent-fill; the active match is filled. Error lines get the log-\n\
         row-error tint; warnings only colour their badge.",
    )],
    &[Run::Text(
        "The volume strip marks errors and warnings; the outlined window is the part on screen \
         — drag it to scroll.",
    )],
    &[Run::Text(
        "Toolbar floats 22 px above the window bottom, centred; it never covers the last line \
         because the panel reserves 96 px.",
    )],
    &[Run::Text(
        "Single-source windows drop the source pill and column.",
    )],
];

fn header(ui: &mut Ui) {
    section_caption(ui, "Header");
    let fill = ui.tokens().category.second;
    LogHeader::new(
        "Deployment",
        "ranker-v7 · 6 replicas",
        "container ranker · 412k lines",
        LogStream::Streaming { live: 5, total: 6 },
    )
    .kind_fill(fill)
    .show(ui, |ui| {
        let mut instance = 0;
        Segmented::new(&mut instance)
            .option(0, "Current")
            .option(1, "Previous")
            .show(ui);
    });
}

fn sources() -> [SourcePill<'static>; 3] {
    [
        SourcePill::live("q2kxm", 1_204),
        SourcePill::reconnecting("7hnwd"),
        SourcePill::offline("mm2vb"),
    ]
}

fn volume_strip(ui: &mut Ui) {
    section_caption(ui, "Volume strip · sources and visible window");
    let bars: Vec<VolumeBar> = BARS
        .iter()
        .map(|(height, tone)| VolumeBar {
            height: height / size::VOLUME_BARS,
            tone: *tone,
        })
        .collect();
    let mut strip = VolumeStrip::new(
        "Volume · 15 min · 41 errors",
        VolumeBars::new(&bars).window(WINDOW.0, WINDOW.1),
    );
    for source in sources() {
        strip = strip.source(source);
    }
    let _ = strip.show(ui);
}

fn log_lines(ui: &mut Ui) {
    section_caption(ui, "Log lines · time · source · level · message");
    let theme = ui.tokens();
    let lines = [
        LogLine::new([
            CodeSpan::text("feature-cache: read "),
            CodeSpan::text("timeout").mark(CodeMark::Underline),
            CodeSpan::text(" after 250ms"),
        ])
        .time("15:20:00.000")
        .source("q2kxm", 0)
        .level(LogLevel::Error),
        LogLine::new([CodeSpan::text("ranker: falling back to cached scores")])
            .time("15:20:07.113")
            .source("7hnwd", 1)
            .level(LogLevel::Warn),
        LogLine::new([
            CodeSpan::text("grpc: deadline exceeded ("),
            CodeSpan::text("timeout").mark(CodeMark::ActiveMatch),
            CodeSpan::text(" 300ms)"),
        ])
        .time("15:20:14.226")
        .source("q2kxm", 0)
        .level(LogLevel::Error)
        .current(true),
        LogLine::new([CodeSpan::text("ranker: served 412 candidates in 288ms")])
            .time("15:20:21.339")
            .source("7hnwd", 1)
            .level(LogLevel::Info),
        LogLine::new([CodeSpan::text("ranker: cache hit ratio 0.71")])
            .time("15:20:35.565")
            .source("q2kxm", 0)
            .level(LogLevel::Debug),
    ];
    let height = 2.0 * PANEL_PADDING + lines.len() as f32 * size::LOG_ROW;
    let (panel, _) = ui.allocate_exact_size(vec2(LEFT_WIDTH, height), egui::Sense::hover());
    ui.painter()
        .rect_filled(panel, radius::CARD, theme.code.surface);
    let mut lines_ui =
        ui.new_child(egui::UiBuilder::new().max_rect(panel.shrink2(vec2(0.0, PANEL_PADDING))));
    lines_ui.set_clip_rect(panel);
    lines_ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
    for line in lines {
        line.show(&mut lines_ui);
    }
}

fn floating_toolbar(ui: &mut Ui) {
    section_caption(ui, "Floating toolbar");
    FloatingToolbar::show(ui, |ui| {
        let mut query = "timeout".to_owned();
        let mut regex = false;
        SearchField::pill(&mut query)
            .navigation(true)
            .height(size::CONTROL_LG)
            .width(TOOLBAR_SEARCH_WIDTH)
            .regex(&mut regex)
            .status(SearchStatus::Matches {
                active: Some(2),
                total: 41,
            })
            .accessibility_label("Search logs")
            .force_state(ControlState::Focused)
            .show(ui);
        ToggleButton::text("Only matches", true).show(ui);
        ToggleButton::text("Line no.", false).show(ui);
        ToggleButton::text("Time", true).show(ui);
        ToggleButton::text("Source", true).show(ui);
        ToggleButton::text("Follow", false)
            .leading_small_icon(Icon::ChevronDoubleDown)
            .show(ui);
    });
}

fn parts(ui: &mut Ui) {
    header(ui);
    ui.add_space(SECTION_GAP);
    volume_strip(ui);
    ui.add_space(SECTION_GAP);
    log_lines(ui);
    ui.add_space(SECTION_GAP);
    floating_toolbar(ui);
}

fn reference(ui: &mut Ui) {
    section_caption(ui, "Level badges");
    row(ui, size::LOG_PILL, PILL_GAP, Align::Center, |ui| {
        for level in [
            LogLevel::Error,
            LogLevel::Warn,
            LogLevel::Info,
            LogLevel::Debug,
        ] {
            LevelBadge(level).show(ui);
        }
    });
    ui.add_space(SECTION_GAP);
    section_caption(ui, "Source pill states");
    row(ui, size::SOURCE_PILL, PILL_GAP, Align::Center, |ui| {
        for source in sources() {
            source.show(ui);
        }
    });
    ui.add_space(SECTION_GAP);
    section_caption(ui, "Rules");
    rules_list(ui, RIGHT_WIDTH, &RULES);
}

fn c20_scene(ui: &mut Ui) {
    board_with_description_width(ui, HEADER, WIDE_DESCRIPTION, |ui| {
        let origin = ui.max_rect().min;
        placed(ui, origin, LEFT_WIDTH, parts);
        placed(ui, pos2(RIGHT_LEFT, origin.y), RIGHT_WIDTH, reference);
    });
}

#[test]
fn c20_log_parts_board() {
    snapshot_board("c20_log_parts", c20_scene);
}
