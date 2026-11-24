//! The C18 "Cluster switcher" component board
//! (`docs/design-oracles/harbour/src/C18.html`) rebuilt from the real
//! components, compared with its oracle by
//! `scripts/design-oracles/compare.sh C18`.

use components::design::size;
use components::design::theme::Tone;
use components::test_support::board::{
    Run, STATE_GAP, WIDE_DESCRIPTION, board_with_keyed_description, placed, section_caption,
    snapshot_board, state,
};
use components::{
    ClusterEntry, ClusterSummary, ClusterSwitcher, ClusterSwitcherState, ClusterTile, Readout,
    ReadoutTone,
};
use egui::{Ui, pos2};

const EYEBROW: &str = "C18 · components::{ClusterSwitcher, ClusterTile}";
const TITLE: &str = "Cluster switcher";
const DESCRIPTION: [Run; 3] = [
    Run::Text("Opened from the cluster card or "),
    Run::Key("⌘K"),
    Run::Text(
        ". Type to filter, Enter switches. The strip at the bottom \
         reads out the highlighted cluster before you switch.",
    ),
];

const SWITCHER_WIDTH: f32 = 1000.0;
const TILES_LEFT: f32 = 1100.0;
const TILES_WIDTH: f32 = 388.0;
const TILE_WIDTH: f32 = 360.0;
/// The latency history of kind-kind, in points of 28.
const LATENCY: [f32; 20] = [
    8.0, 10.0, 7.0, 12.0, 9.0, 11.0, 8.0, 14.0, 10.0, 9.0, 7.0, 12.0, 10.0, 8.0, 11.0, 9.0, 13.0,
    10.0, 8.0, 12.0,
];

fn kind_kind(summary: bool) -> ClusterEntry<'static> {
    ClusterEntry {
        name: "kind-kind",
        initials: "KK",
        category: 1,
        meta: None,
        detail: "3 nodes",
        status: Some(Tone::Success),
        key: Some("⌘1"),
        summary: summary.then(|| ClusterSummary {
            readouts: vec![
                Readout::new("Nodes", "3"),
                Readout::new("Pods", "18"),
                Readout::new("Failing", "2").tone(ReadoutTone::Problem),
                Readout::new("Namespaces", "5"),
                Readout::new("CRDs", "8"),
                Readout::new("Latency", "4 ms"),
            ],
            latency: LATENCY
                .iter()
                .map(|height| height / size::CLUSTER_LATENCY.y)
                .collect(),
        }),
    }
}

fn prod_eu(key: bool) -> ClusterEntry<'static> {
    ClusterEntry {
        name: "prod-eu",
        initials: "PR",
        category: 3,
        meta: None,
        detail: "unreachable",
        status: Some(Tone::Danger),
        key: key.then_some("⌘2"),
        summary: None,
    }
}

fn payments() -> ClusterEntry<'static> {
    ClusterEntry {
        name: "payments",
        initials: "PA",
        category: 4,
        meta: Some("EKS · NOT IN KUBECONFIG"),
        detail: "add",
        status: None,
        key: None,
        summary: None,
    }
}

fn switcher(ui: &mut Ui) {
    let mut state = ClusterSwitcherState::default();
    let recent = vec![
        ClusterEntry {
            name: "dev-eu",
            initials: "DE",
            category: 0,
            meta: None,
            detail: "38 nodes",
            status: Some(Tone::Success),
            key: None,
            summary: None,
        },
        ClusterEntry {
            name: "kind-legacy",
            initials: "KL",
            category: 2,
            meta: None,
            detail: "1 node",
            status: Some(Tone::Neutral),
            key: None,
            summary: None,
        },
    ];
    let _ = ClusterSwitcher::new(&mut state)
        .width(SWITCHER_WIDTH)
        .section("Pinned", vec![kind_kind(true), prod_eu(true)])
        .section("Recent", recent)
        .section("Found in cloud accounts", vec![payments()])
        .notice("Found 1 new cluster in AWS")
        .force_search_focus()
        .show(ui);
}

fn tiles(ui: &mut Ui) {
    section_caption(ui, "Cluster tile");
    let states = [
        (kind_kind(false), false, "Rest · pinned key"),
        (kind_kind(false), true, "Highlighted (keyboard) / current"),
        (prod_eu(false), false, "Unreachable — still selectable"),
        (payments(), false, "Found by discovery (A10)"),
    ];
    for (index, (entry, highlighted, caption)) in states.into_iter().enumerate() {
        if index > 0 {
            ui.add_space(STATE_GAP);
        }
        state(ui, size::CLUSTER_TILE, caption, |ui| {
            ui.push_id(index, |ui| {
                ClusterTile::new(&entry)
                    .highlighted(highlighted)
                    .show(ui, TILE_WIDTH);
            });
        });
    }
}

fn c18_scene(ui: &mut Ui) {
    board_with_keyed_description(ui, EYEBROW, TITLE, &DESCRIPTION, WIDE_DESCRIPTION, |ui| {
        let origin = ui.max_rect().min;
        placed(ui, origin, SWITCHER_WIDTH, switcher);
        placed(ui, pos2(TILES_LEFT, origin.y), TILES_WIDTH, tiles);
    });
}

#[test]
fn c18_cluster_switcher_board() {
    snapshot_board("c18_cluster_switcher", c18_scene);
}
