//! The C16 "Top bar" component board (`docs/design-oracles/harbour/src/C16.html`)
//! rebuilt from the real components, compared with its oracle by
//! `scripts/design-oracles/compare.sh C16`.

use components::design::size;
use components::test_support::board::{
    BoardHeader, STATE_GAP, WIDE_DESCRIPTION, board_with_description_width, placed,
    section_caption, snapshot_board, state,
};
use components::{ControlState, FilterChip, LiveMeta, ScopePill, SearchPill, TopBar};
use egui::{Ui, pos2};

const HEADER: BoardHeader<'static> = BoardHeader {
    eyebrow: "C16 · components::{TopBar, SearchPill, ScopePill, FilterChip}",
    title: "Top bar & filter chips",
    description: "The list’s controls in one 38 px row: search, scope, grouping and the \
                  problems filter, then live counts. Keys are shown, not hidden in menus.",
};

/// The wide demo's width; any window of at least 1100 points is wide.
const WIDE: f32 = 1204.0;
/// The narrow demo's width, standing for a window under 1100 points.
const NARROW: f32 = 720.0;
/// The states columns (`.row` gap 48) and their widths.
const STATES_TOP: f32 = 300.0;
const COLUMNS: [(f32, f32); 3] = [(48.0, 320.0), (416.0, 260.0), (724.0, 320.0)];
/// Extra space above the Live meta caption (`margin-top: 8px`).
const LIVE_META_OFFSET: f32 = 8.0;
const NARROW_TOP: f32 = 820.0;
/// Gap between the narrow demo and its caption.
const NARROW_CAPTION_GAP: f32 = 6.0;
/// The live meta's row (`.tmeta`, a 20 point line box).
const META_ROW: f32 = 20.0;

const NAMESPACES: [&str; 3] = ["kube-system", "monitoring", "default"];

/// A scope pill's combobox with the board's namespaces.
fn scope(ui: &mut Ui, pill: ScopePill) {
    let names: Vec<String> = NAMESPACES.iter().map(|name| (*name).to_owned()).collect();
    pill.combobox()
        .filter_by(|name: &String| name.as_str())
        .show_items(ui, &names, |list, name| {
            list.item(name, false);
        });
}

fn top_bar(ui: &mut Ui, width: f32, salt: &str) {
    let bar = TopBar::new(width);
    let mut query = String::new();
    ui.set_width(width);
    bar.show(
        ui,
        |ui| {
            SearchPill::new(&mut query)
                .id_salt(("search", salt))
                .placeholder(if bar.is_narrow() {
                    "Find a pod"
                } else {
                    "Find a pod, label or /regex/"
                })
                .width(bar.search_width())
                .show(ui);
            scope(
                ui,
                ScopePill::namespaces(["kube-system"], Some(true))
                    .id_salt(("scope", salt))
                    .width(bar.scope_width()),
            );
            if bar.shows_grouping() {
                FilterChip::grouping(Some("namespace")).show(ui);
            }
            FilterChip::problems(2, false).show(ui);
        },
        Some(LiveMeta::new("12 pods")),
    );
}

fn search_pills(ui: &mut Ui) {
    section_caption(ui, "Search pill");
    ui.add_space(STATE_GAP);
    let height = size::TOP_BAR_CONTROL;
    let mut query = String::new();
    state(ui, height, "Rest · / focuses", |ui| {
        SearchPill::new(&mut query).id_salt("rest").show(ui);
    });
    ui.add_space(STATE_GAP);
    let mut typed = "app=ranker".to_owned();
    state(ui, height, "Typing · label selector", |ui| {
        SearchPill::new(&mut typed)
            .id_salt("typing")
            .force_state(ControlState::Focused)
            .show(ui);
    });
    ui.add_space(STATE_GAP);
    state(ui, height, "Disabled · no cluster", |ui| {
        ui.add_enabled_ui(false, |ui| {
            SearchPill::new(&mut query).id_salt("disabled").show(ui);
        });
    });
}

fn scope_pills(ui: &mut Ui) {
    section_caption(ui, "Scope pill");
    ui.add_space(STATE_GAP);
    let height = size::TOP_BAR_CONTROL;
    let states = [
        (
            ScopePill::namespaces(["kube-system"], Some(true)),
            "One namespace · watch lamp",
        ),
        (
            ScopePill::namespaces(["kube-system", "monitoring"], Some(true))
                .force_state(ControlState::Focused),
            "Several · open (popover C05)",
        ),
        (
            ScopePill::all_namespaces(Some(182)),
            "All namespaces + count",
        ),
        (ScopePill::cluster_wide(), "Cluster-scoped kinds"),
    ];
    for (index, (pill, caption)) in states.into_iter().enumerate() {
        if index > 0 {
            ui.add_space(STATE_GAP);
        }
        state(ui, height, caption, |ui| {
            scope(ui, pill.id_salt(("scope-state", index)));
        });
    }
}

fn filter_chips(ui: &mut Ui) {
    section_caption(ui, "Filter chips");
    ui.add_space(STATE_GAP);
    let height = size::CONTROL_MD;
    let states = [
        (FilterChip::grouping(Some("namespace")), "Grouping on"),
        (FilterChip::grouping(None), "Grouping off"),
        (FilterChip::problems(98, false), "Problems filter · off"),
        (FilterChip::problems(98, true), "Problems filter · on"),
    ];
    for (chip, caption) in states {
        state(ui, height, caption, |ui| {
            chip.show(ui);
        });
        ui.add_space(STATE_GAP);
    }
    ui.add_space(LIVE_META_OFFSET);
    section_caption(ui, "Live meta");
    ui.add_space(STATE_GAP);
    state(
        ui,
        META_ROW,
        "Live meta · [ ] jumps between problem rows",
        |ui| {
            LiveMeta::new("4812 pods").show(ui);
        },
    );
}

fn c16_scene(ui: &mut Ui) {
    board_with_description_width(ui, HEADER, WIDE_DESCRIPTION, |ui| {
        let origin = ui.max_rect().min;
        section_caption(ui, "Top bar");
        top_bar(ui, WIDE, "wide");

        let states = [search_pills, scope_pills, filter_chips];
        for ((left, width), add) in COLUMNS.into_iter().zip(states) {
            placed(ui, pos2(left, STATES_TOP), width, add);
        }

        placed(ui, pos2(origin.x, NARROW_TOP), NARROW, |ui| {
            section_caption(ui, "Narrow");
            top_bar(ui, NARROW, "narrow");
            ui.add_space(NARROW_CAPTION_GAP);
            components::design::type_scale::META.label(
                ui,
                "Narrow window (< 1100 px): chips collapse to Problems only, meta hides",
                components::design::theme::ThemeExt::tokens(ui).text.muted,
            );
        });
    });
}

#[test]
fn c16_top_bar_board() {
    snapshot_board("c16_top_bar", c16_scene);
}
