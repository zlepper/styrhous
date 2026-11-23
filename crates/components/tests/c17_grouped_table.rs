//! The C17 "Grouped table" component board
//! (`docs/design-oracles/harbour/src/C17.html`) rebuilt from the real
//! components, compared with its oracle by
//! `scripts/design-oracles/compare.sh C17`.

use std::collections::HashSet;

use components::design::size;
use components::design::theme::Tone;
use components::test_support::board::{
    BoardHeader, Run, STATE_GAP, WIDE_DESCRIPTION, board_with_description_width, placed, row,
    rules_list, section_caption, snapshot_board, state,
};
use components::{
    CellAlign, GroupHeader, MoreButton, ProblemTick, ProblemTicks, SortDirection, SortState, Table,
    TableGroup, TableRowBuilder,
};
use egui::{Align, Rect, Ui, pos2, vec2};

const HEADER: BoardHeader<'static> = BoardHeader {
    eyebrow: "C17 · components::{GroupedTable, GroupHeader, ProblemTicks}",
    title: "Grouped table & problem ticks",
    description: "Rows stay 36 px tiles on a 40 px pitch (C08). Grouping inserts a 34 px header \
                  per group; problem rows colour their status and restart count; the scrollbar \
                  marks every problem so you can jump to it.",
};

const TABLE_WIDTH: f32 = 1060.0;
/// The board's scrollbar ticks beside the table.
const TICKS: Rect = Rect {
    min: pos2(1128.0, 216.0),
    max: pos2(1128.0 + size::PROBLEM_TRACK, 216.0 + 470.0),
};
const TICK_THUMB: (f32, f32) = (0.18, 0.20);
const TABLE_TICKS: [(f32, Tone); 5] = [
    (0.08, Tone::Warning),
    (0.22, Tone::Danger),
    (0.48, Tone::Warning),
    (0.51, Tone::Danger),
    (0.74, Tone::Warning),
];
const PARTS_TOP: f32 = 720.0;
/// The group header column and the problem ticks column (`.row` gap 60).
const HEADERS_WIDTH: f32 = 440.0;
const HEADER_DEMO_WIDTH: f32 = 420.0;
const TICKS_LEFT: f32 = 548.0;
const TICKS_WIDTH: f32 = 520.0;
const TICKS_DEMO_HEIGHT: f32 = 160.0;
const TICKS_DEMO_THUMB: (f32, f32) = (0.30, 0.25);
const TICKS_DEMO: [(f32, Tone); 2] = [(0.20, Tone::Warning), (0.55, Tone::Danger)];
/// Gap between the ticks demo and its rules (`.row` gap 20).
const TICKS_TO_RULES: f32 = 20.0;
const RULES_WIDTH: f32 = 494.0;

struct Pod {
    name: &'static str,
    ready: &'static str,
    status: (&'static str, Tone),
    restarts: &'static str,
    cpu: &'static str,
    memory: &'static str,
    age: &'static str,
}

const fn pod(
    name: &'static str,
    ready: &'static str,
    status: (&'static str, Tone),
    restarts: &'static str,
    usage: (&'static str, &'static str),
    age: &'static str,
) -> Pod {
    Pod {
        name,
        ready,
        status,
        restarts,
        cpu: usage.0,
        memory: usage.1,
        age,
    }
}

const RUNNING: (&str, Tone) = ("Running", Tone::Success);
const SEARCH: [Pod; 4] = [
    pod(
        "search-indexer-kxwgpzt2x-dzb9t",
        "1/1",
        RUNNING,
        "0",
        ("121m", "416Mi"),
        "41d",
    ),
    pod(
        "search-query-wtq2gk2gv-q6b47",
        "0/1",
        ("Pending", Tone::Warning),
        "3",
        ("390m", "1234Mi"),
        "1d",
    ),
    pod(
        "search-indexer-9ltz6q58r-q68xs",
        "1/1",
        RUNNING,
        "0",
        ("70m", "1823Mi"),
        "3d",
    ),
    pod(
        "search-indexer-5kmrlvmwn-b8rxn",
        "0/1",
        ("CrashLoopBackOff", Tone::Danger),
        "41",
        ("317m", "1230Mi"),
        "2h",
    ),
];
const ML_SERVING: [Pod; 3] = [
    pod(
        "ranker-v7-zl6p85962-vb9s7",
        "1/1",
        RUNNING,
        "0",
        ("334m", "130Mi"),
        "2h",
    ),
    pod(
        "ranker-v7-5jqmbprhg-ldnm9",
        "5/6",
        ("Degraded", Tone::Warning),
        "15",
        ("412m", "6.1Gi"),
        "6h",
    ),
    pod(
        "embedder-tkqwfxkb6-jrlh7",
        "1/1",
        RUNNING,
        "0",
        ("392m", "1656Mi"),
        "38m",
    ),
];
/// The hovered and the inspected row, counted across groups.
const HOVERED: usize = 2;
const INSPECTED: usize = 5;

fn problem(pod: &Pod) -> Option<Tone> {
    matches!(pod.status.1, Tone::Warning | Tone::Danger).then_some(pod.status.1)
}

fn pod_cell(ui: &mut Ui, pod: &Pod, column: usize) {
    match column {
        0 => TableRowBuilder::name(ui, pod.name),
        1 => TableRowBuilder::number(ui, pod.ready),
        2 => {
            TableRowBuilder::problem_status(ui, pod.status.0, pod.status.1);
        }
        3 => TableRowBuilder::problem_number(ui, pod.restarts, problem(pod)),
        4 => TableRowBuilder::number(ui, pod.cpu),
        5 => TableRowBuilder::number(ui, pod.memory),
        6 => TableRowBuilder::number(ui, pod.age),
        _ => {
            MoreButton::new(format!("More actions for {}", pod.name)).show(ui, |_| {});
        }
    }
}

fn grouped_table(ui: &mut Ui) {
    section_caption(ui, "Grouped by namespace");
    let height = size::TABLE_HEADER
        + 3.0 * size::GROUP_HEADER
        + (SEARCH.len() + ML_SERVING.len()) as f32 * size::ROW_PITCH;
    let rect = Rect::from_min_size(ui.cursor().min, vec2(TABLE_WIDTH, height));
    let mut table_ui = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    let groups = vec![
        TableGroup::new(
            GroupHeader::new("search", 642)
                .unit("pod", "pods")
                .failing(7)
                .window("rows 389–397 of 642"),
            &SEARCH[..],
        ),
        TableGroup::new(
            GroupHeader::new("ml-serving", 1_210)
                .unit("pod", "pods")
                .failing(31)
                .window("first 20 of 1,210"),
            &ML_SERVING[..],
        ),
        TableGroup::new(
            GroupHeader::new("payments", 48).unit("pod", "pods"),
            &[][..],
        )
        .expanded(false),
    ];
    let sort = SortState::new("name", SortDirection::Ascending);
    let mut selected: HashSet<&str> = HashSet::new();
    Table::new("grouped-pods")
        .column("name", "Name", |col| {
            col.sortable().fill_remaining().initial_width(0.0)
        })
        .column("ready", "Ready", |col| col.numeric().initial_width(72.0))
        .column("status", "Status", |col| col.initial_width(150.0))
        .column("restarts", "Restarts", |col| {
            col.numeric().initial_width(84.0)
        })
        .column("cpu", "CPU", |col| col.numeric().initial_width(76.0))
        .column("memory", "Memory", |col| col.numeric().initial_width(84.0))
        .column("age", "Age", |col| col.numeric().initial_width(64.0))
        .column("actions", "", |col| {
            col.align(CellAlign::Center)
                .initial_width(size::TABLE_ACTIONS_COLUMN)
        })
        .selectable()
        .force_row_hovered(HOVERED)
        .inspected_row(Some(INSPECTED))
        .show_grouped(
            &mut table_ui,
            groups,
            Some(&mut selected),
            |pod: &Pod| Some(pod.name),
            Some(&sort),
            problem,
            pod_cell,
        );
}

fn ticks(ticks: &[(f32, Tone)]) -> Vec<ProblemTick> {
    ticks
        .iter()
        .map(|(position, tone)| ProblemTick {
            position: *position,
            tone: *tone,
        })
        .collect()
}

fn group_headers(ui: &mut Ui) {
    section_caption(ui, "Group header");
    ui.add_space(STATE_GAP);
    let states = [
        (
            GroupHeader::new("search", 642)
                .unit("pod", "pods")
                .failing(7)
                .window("rows 389–397"),
            "Expanded · failing count in danger-text",
        ),
        (
            GroupHeader::new("payments", 48)
                .unit("pod", "pods")
                .expanded(false),
            "Collapsed · no problems",
        ),
        (
            GroupHeader::new("kube-system", 1).unit("pod", "pods"),
            "Singular unit for one row",
        ),
    ];
    for (index, (header, caption)) in states.into_iter().enumerate() {
        if index > 0 {
            ui.add_space(STATE_GAP);
        }
        state(ui, size::GROUP_HEADER, caption, |ui| {
            ui.set_width(HEADER_DEMO_WIDTH);
            ui.push_id(index, |ui| {
                let _ = header.show(ui);
            });
        });
    }
}

const RULES: [&[Run]; 3] = [
    &[Run::Text(
        "Track: 6 px, surface-raised; thumb divider-strong.",
    )],
    &[Run::Text(
        "Tick: 3 px, warning-fill (degraded, pending, restarts) or danger-fill (failed, crash-\n\
         looping).",
    )],
    &[
        Run::Text("Click a tick to scroll there; "),
        Run::Key("["),
        Run::Text(" "),
        Run::Key("]"),
        Run::Text(" step through them."),
    ],
];

fn problem_ticks(ui: &mut Ui) {
    section_caption(ui, "Problem ticks");
    row(ui, 0.0, TICKS_TO_RULES, Align::Min, |ui| {
        let demo = ticks(&TICKS_DEMO);
        let _ = ProblemTicks::new(&demo)
            .thumb(TICKS_DEMO_THUMB.0, TICKS_DEMO_THUMB.1)
            .show(ui, TICKS_DEMO_HEIGHT);
        ui.vertical(|ui| rules_list(ui, RULES_WIDTH, &RULES));
    });
}

fn c17_scene(ui: &mut Ui) {
    board_with_description_width(ui, HEADER, WIDE_DESCRIPTION, |ui| {
        let left = ui.max_rect().left();
        grouped_table(ui);
        let table_ticks = ticks(&TABLE_TICKS);
        let _ = ProblemTicks::new(&table_ticks)
            .thumb(TICK_THUMB.0, TICK_THUMB.1)
            .show_in(ui, TICKS);
        placed(ui, pos2(left, PARTS_TOP), HEADERS_WIDTH, group_headers);
        placed(ui, pos2(TICKS_LEFT, PARTS_TOP), TICKS_WIDTH, problem_ticks);
    });
}

#[test]
fn c17_grouped_table_board() {
    snapshot_board("c17_grouped_table", c17_scene);
}
