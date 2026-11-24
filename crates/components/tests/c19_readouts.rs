//! The C19 "Readouts and section rail" component board
//! (`docs/design-oracles/harbour/src/C19.html`) rebuilt from the real
//! components, compared with its oracle by
//! `scripts/design-oracles/compare.sh C19`.

use components::design::theme::{self, ThemeExt, ThemeScope, Tone};
use components::design::{radius, size};
use components::frames::{overlay_frame, show_elevated};
use components::icons::Icon;
use components::test_support::board::{
    BoardHeader, Run, WIDE_DESCRIPTION, board_with_description_width, placed, row, rules_list,
    section_caption, snapshot_board,
};
use components::{
    BladeNavigation, Button, ButtonSize, ButtonVariant, InspectorHeader, InspectorStatus, Readout,
    ReadoutRow, ReadoutSize, ReadoutTone, Section, SectionRail,
};
use egui::{Align, Ui, pos2, vec2};

const HEADER: BoardHeader<'static> = BoardHeader {
    eyebrow: "C19 · components::{Readout, InspectorHeader, SectionRail}",
    title: "Readouts & section rail",
    description: "Readouts lift the few numbers you check first out of the property grid: \
                  caption in condensed caps, value in mono. Inspectors put them in the header \
                  and add a section rail so long resources stay navigable.",
};

const READOUTS_WIDTH: f32 = 820.0;
/// Space above each caption after the first (`margin-top: 22px`).
const SECTION_GAP: f32 = 22.0;
const BLADE: egui::Vec2 = vec2(720.0, 150.0);
const RIGHT_LEFT: f32 = 940.0;
const RIGHT_WIDTH: f32 = 548.0;
/// The rail column and the rules column (`.row` gap 32).
const RAIL_TO_RULES: f32 = 32.0;
const RAIL_HEIGHT: f32 = 300.0;
const RULES_WIDTH: f32 = 330.0;

const INSPECTOR_ROW: [Readout<'static>; 6] = [
    Readout::new("Ready", "5 / 6"),
    Readout::new("Restarts", "15").tone(ReadoutTone::Warning),
    Readout::new("Node", "ip-10-0-41-17").tone(ReadoutTone::Link),
    Readout::new("CPU", "412m").unit("/ 2"),
    Readout::new("Memory", "6.1").unit("/ 8 Gi"),
    Readout::new("Age", "6 h"),
];
const GAUGES: [Readout<'static>; 4] = [
    Readout::new("Pods", "4,812"),
    Readout::new("Failing", "98").tone(ReadoutTone::Problem),
    Readout::new("Nodes", "86"),
    Readout::new("Latency", "38").unit("ms"),
];
const TONES: [Readout<'static>; 5] = [
    Readout::new("Neutral", "220d"),
    Readout::new("OK", "Deployed").tone(ReadoutTone::Ok),
    Readout::new("Warning", "15").tone(ReadoutTone::Warning),
    Readout::new("Problem", "Failed").tone(ReadoutTone::Problem),
    Readout::new("Link", "kind-worker").tone(ReadoutTone::Link),
];
const HEADER_READOUTS: [Readout<'static>; 5] = [
    Readout::new("Ready", "5 / 6"),
    Readout::new("Restarts", "15").tone(ReadoutTone::Warning),
    Readout::new("CPU", "412m"),
    Readout::new("Memory", "6.1Gi"),
    Readout::new("Age", "6h"),
];

const SECTIONS: [Section<'static>; 7] = [
    Section::new("Overview"),
    Section::new("Usage"),
    Section::new("Containers").count(6),
    Section::new("Environment").count(63),
    Section::new("Volumes").count(14),
    Section::new("Events").count(37).alerts(3),
    Section::new("YAML"),
];

const RULES: [&[Run]; 5] = [
    &[Run::Text(
        "One row of 4–6 readouts, never wrapping; values truncate with a tooltip.",
    )],
    &[Run::Text(
        "Only live or decisive values: readiness, restarts, usage, age. Identity fields stay \
         in the property grid.",
    )],
    &[Run::Text(
        "Tone the value, not the tile: warning-text, danger-text, success-text, accent-text \
         for links.",
    )],
    &[Run::Text(
        "Section rail lists the inspector’s sections with counts; clicking scrolls, scrolling \
         moves the selection. Inspectors with fewer than four sections skip the rail.",
    )],
    &[
        Run::Text("Primary header action shows its key ("),
        Run::Key("L"),
        Run::Text(", "),
        Run::Key("S"),
        Run::Text(")."),
    ],
];

fn inspector_header(ui: &mut Ui) {
    let theme = ui.tokens();
    show_elevated(ui, overlay_frame(&theme), theme.shadow.overlay, |ui| {
        ui.set_min_size(BLADE);
        ui.set_max_size(BLADE);
        theme::scope(ui, ThemeScope::Blade, |ui| {
            let fill = ui.tokens().category.fourth;
            let _ = InspectorHeader::new(
                "ranker-v7-5jqmbprhg-ldnm9",
                BladeNavigation::new(true, false),
            )
            .kind("Pod", Some(fill))
            .status(InspectorStatus {
                tone: Tone::Warning,
                state: "Degraded",
                reason: Some("warmup container crash-looping"),
                link: None,
            })
            .readouts(&HEADER_READOUTS)
            .show(ui, |ui| {
                Button::primary("Logs")
                    .size(ButtonSize::Sm)
                    .shortcut("L")
                    .show(ui);
                Button::secondary("Shell")
                    .size(ButtonSize::Sm)
                    .shortcut("S")
                    .show(ui);
                Button::icon(Icon::EllipsisHorizontal, "More actions")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Sm)
                    .show(ui);
            });
        });
    });
}

fn readouts(ui: &mut Ui) {
    section_caption(ui, "Readout row · inspector size");
    ReadoutRow::new(&INSPECTOR_ROW).show(ui);
    ui.add_space(SECTION_GAP);
    section_caption(ui, "Gauge size · overviews and the cluster strip");
    ReadoutRow::new(&GAUGES).size(ReadoutSize::Gauge).show(ui);
    ui.add_space(SECTION_GAP);
    section_caption(ui, "Value tones");
    ReadoutRow::new(&TONES).show(ui);
    ui.add_space(SECTION_GAP);
    section_caption(ui, "Inspector header");
    inspector_header(ui);
}

fn rail_and_rules(ui: &mut Ui) {
    row(ui, 0.0, RAIL_TO_RULES, Align::Min, |ui| {
        ui.vertical(|ui| {
            ui.set_width(size::SECTION_RAIL);
            section_caption(ui, "Section rail");
            let mut current = 3;
            let _ = SectionRail::new("On this pod", &SECTIONS, &mut current)
                .corner_radius(radius::CARD)
                .show(ui, RAIL_HEIGHT);
        });
        ui.vertical(|ui| {
            ui.set_width(RULES_WIDTH);
            section_caption(ui, "Rules");
            rules_list(ui, RULES_WIDTH, &RULES);
        });
    });
}

fn c19_scene(ui: &mut Ui) {
    board_with_description_width(ui, HEADER, WIDE_DESCRIPTION, |ui| {
        let origin = ui.max_rect().min;
        placed(ui, origin, READOUTS_WIDTH, readouts);
        placed(ui, pos2(RIGHT_LEFT, origin.y), RIGHT_WIDTH, rail_and_rules);
    });
}

#[test]
fn c19_readouts_board() {
    snapshot_board("c19_readouts", c19_scene);
}
