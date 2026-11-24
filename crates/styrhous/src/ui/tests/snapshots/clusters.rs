//! The cluster switcher (A16) and the landing page without a cluster (A01).

use super::super::design_fixtures::KIND_KIND;
use super::super::design_fixtures::{
    deliver_design_outcomes, design_harness, seed_design_clusters,
};
use super::*;
use egui_kittest::kittest::By;

fn press_command(harness: &mut AppHarness, key: egui::Key) {
    harness.key_press_modifiers(egui::Modifiers::COMMAND, key);
    harness.run();
}

fn switcher_harness() -> AppHarness {
    let mut harness = design_harness(BOARD_SIZE);
    click_label(&mut harness, "Workloads");
    deliver_design_outcomes(&mut harness);
    seed_design_clusters(&mut harness);
    harness
}

/// A16: Ctrl+K opens the switcher over the Pods board, kind-kind (the
/// current cluster) highlighted and its readouts in the strip.
#[test]
fn a16_cluster_switcher() {
    let mut harness = switcher_harness();
    // A16 shows prod-eu as unreachable: no node count was ever seen of it.
    harness
        .state_mut()
        .ui_state
        .cluster_roster
        .known_nodes
        .remove("prod-eu");
    press_command(&mut harness, egui::Key::K);
    rest_pointer(&mut harness);
    harness.ui_harness("boards/a16_cluster_switcher");
}

/// A01: the landing page, every context a tile with its ⌘ key.
#[test]
fn a01_choose_a_cluster() {
    let mut harness = design_harness(BOARD_SIZE);
    seed_design_clusters(&mut harness);
    harness.state_mut().ui_state.selected_cluster = None;
    harness.run();
    rest_pointer(&mut harness);
    harness.ui_harness("boards/a01_choose_a_cluster");
}

fn tile_centre(harness: &AppHarness, name: &str) -> egui::Pos2 {
    harness
        .get(
            By::new()
                .role(egui::accesskit::Role::Button)
                .label_contains(name),
        )
        .rect()
        .center()
}

fn has_tile(harness: &AppHarness, name: &str) -> bool {
    harness
        .query(
            By::new()
                .role(egui::accesskit::Role::Button)
                .label_contains(name),
        )
        .is_some()
}

fn is_open(harness: &AppHarness) -> bool {
    harness.state().ui_state.cluster_switcher.is_some()
}

fn connects_to(harness: &AppHarness, cluster_key: i32) -> bool {
    harness.state().worker.commands.iter().any(|command| {
        command_is::<ConnectToCluster>(command).is_some_and(|c| c.cluster_key == cluster_key)
    })
}

/// The cluster card and Ctrl+K open the switcher (Ctrl+K again and Esc
/// close it); the card no longer lists the contexts itself.
#[test]
fn the_card_and_ctrl_k_open_the_switcher_and_esc_closes_it() {
    let mut harness = switcher_harness();
    assert!(!is_open(&harness));
    click_label(&mut harness, "Switch cluster");
    assert!(is_open(&harness));
    harness.get_by_label("Find a cluster");
    assert!(
        harness
            .query_by_role_and_label(egui::accesskit::Role::MenuItem, "dev-eu (Disconnected)")
            .is_none()
    );
    harness.key_press(egui::Key::Escape);
    harness.run();
    assert!(!is_open(&harness));

    press_command(&mut harness, egui::Key::K);
    assert!(is_open(&harness));
    press_command(&mut harness, egui::Key::K);
    assert!(!is_open(&harness));
}

/// Typing narrows the clusters to those matching, Enter switches to the
/// highlighted one, connecting it and remembering it as the latest visit.
#[test]
fn typing_filters_and_enter_switches_and_connects() {
    let mut harness = switcher_harness();
    press_command(&mut harness, egui::Key::K);
    harness.event(egui::Event::Text("legacy".into()));
    harness.run();
    harness.get_by_label("Matching “legacy”");
    assert!(
        harness
            .query_all_by_label_contains("dev-eu")
            .next()
            .is_none()
    );
    harness.key_press(egui::Key::Enter);
    harness.run();
    assert!(!is_open(&harness));
    assert_eq!(harness.state().ui_state.selected_cluster, Some(3));
    assert!(connects_to(&harness, 3), "a disconnected context connects");
    assert_eq!(
        harness.state().ui_state.cluster_roster.recent[0],
        "kind-legacy"
    );
}

/// The app does not guess where a cluster runs: a context's tile is its name,
/// its node count and its state, and there are no provider chips. Only a
/// cluster discovery found says which cloud account it came from.
#[test]
fn tiles_do_not_guess_a_provider_or_locality() {
    let mut harness = switcher_harness();
    press_command(&mut harness, egui::Key::K);
    assert!(has_tile(&harness, "kind-kind, 3 nodes"));
    assert!(has_tile(&harness, "dev-eu, 38 nodes"));
    assert!(has_tile(&harness, "payments, EKS · NOT IN KUBECONFIG, add"));
    let buttons: Vec<String> = harness
        .query_all(By::new().role(egui::accesskit::Role::Button))
        .filter_map(|node| node.accesskit_node().label())
        .map(|label| label.to_string())
        .collect();
    for guess in ["LOCAL", "WESTEUROPE", "EU-WEST-1", "Kind 2", "EKS 1"] {
        assert!(
            !buttons.iter().any(|label| label.contains(guess)),
            "nothing says {guess}: {buttons:?}"
        );
    }
}

/// Ctrl+P pins the highlighted cluster (the pinned ones take ⌘1…), and
/// Ctrl+digit switches to a pinned cluster.
#[test]
fn pinning_and_the_digit_keys_reach_a_pinned_cluster() {
    let mut harness = switcher_harness();
    press_command(&mut harness, egui::Key::K);
    // The current cluster (kind-kind) is highlighted and pinned: unpin it.
    press_command(&mut harness, egui::Key::P);
    assert_eq!(harness.state().ui_state.cluster_roster.pinned, ["prod-eu"]);
    press_command(&mut harness, egui::Key::Num1);
    assert!(!is_open(&harness));
    assert_eq!(harness.state().ui_state.selected_cluster, Some(4));

    // With the switcher closed the keys still reach the pinned clusters.
    harness.state_mut().ui_state.cluster_roster.pinned = vec!["kind-legacy".into()];
    press_command(&mut harness, egui::Key::Num1);
    assert_eq!(harness.state().ui_state.selected_cluster, Some(3));
}

/// Enter on a cluster found in a cloud account asks the worker to add it.
#[test]
fn enter_on_a_found_cluster_adds_it_to_the_kubeconfig() {
    let mut harness = switcher_harness();
    press_command(&mut harness, egui::Key::K);
    let found = tile_centre(&harness, "payments");
    harness.event(egui::Event::PointerMoved(found));
    primary_click(&mut harness, found);
    harness.run();
    assert!(!is_open(&harness));
    let command = harness
        .state()
        .worker
        .commands
        .iter()
        .find_map(command_is::<AddEksCluster>)
        .expect("choosing a found cluster adds it");
    assert_eq!(command.name, "payments");
    assert_eq!(command.region, "eu-west-1");
    assert!(
        harness
            .state()
            .ui_state
            .managed_cluster_discovery
            .importing
            .is_some()
    );
}

/// Opening in a new window is not built yet and says so.
#[test]
fn opening_in_a_new_window_says_it_is_not_implemented() {
    let mut harness = switcher_harness();
    press_command(&mut harness, egui::Key::K);
    press_command(&mut harness, egui::Key::Enter);
    assert!(is_open(&harness), "the switcher stays open");
    harness.get_by_label("Not yet implemented");
}

/// The landing page: a tile switches to its context, so do the ⌘ keys, and
/// its buttons open cluster discovery and the kubeconfig settings.
#[test]
fn the_landing_page_switches_clusters_and_finds_more() {
    let mut harness = design_harness(BOARD_SIZE);
    seed_design_clusters(&mut harness);
    harness.state_mut().ui_state.selected_cluster = None;
    harness.run();
    harness.get_by_label("Choose a cluster");

    press_command(&mut harness, egui::Key::Num3);
    assert_eq!(harness.state().ui_state.selected_cluster, Some(3));
    assert!(connects_to(&harness, 3));

    harness.state_mut().ui_state.selected_cluster = None;
    harness.run();
    let tile = tile_centre(&harness, "dev-eu");
    primary_click(&mut harness, tile);
    harness.run();
    assert_eq!(harness.state().ui_state.selected_cluster, Some(1));

    harness.state_mut().ui_state.selected_cluster = None;
    harness.run();
    click_label(&mut harness, "Find clusters");
    harness.get_by_label("Cluster discovery");
}

/// A query nothing matches leaves the switcher open and empty, and Enter
/// switches nothing.
#[test]
fn a_query_with_no_match_shows_nothing_to_switch_to() {
    let mut harness = switcher_harness();
    press_command(&mut harness, egui::Key::K);
    harness.event(egui::Event::Text("zzz".into()));
    harness.run();
    assert!(!has_tile(&harness, "kind-kind"));
    harness.key_press(egui::Key::Enter);
    harness.run();
    assert!(is_open(&harness));
    assert_eq!(harness.state().ui_state.selected_cluster, Some(KIND_KIND));
}

/// Clicking the scrim closes the switcher without switching.
#[test]
fn clicking_the_scrim_closes_the_switcher() {
    let mut harness = switcher_harness();
    press_command(&mut harness, egui::Key::K);
    primary_click(&mut harness, egui::pos2(900.0, 900.0));
    harness.run();
    assert!(!is_open(&harness));
    assert_eq!(harness.state().ui_state.selected_cluster, Some(KIND_KIND));
}

/// Without contexts the landing page points at discovery, and the switcher
/// and the ⌘ keys do nothing harmful.
#[test]
fn an_empty_kubeconfig_has_a_landing_page_and_an_empty_switcher() {
    let mut harness = application_harness::<MockWorker>();
    harness.set_size(BOARD_SIZE);
    harness.run();
    harness.get_by_label("No contexts in your kubeconfig. Find clusters in your cloud accounts.");
    press_command(&mut harness, egui::Key::Num1);
    press_command(&mut harness, egui::Key::K);
    assert!(is_open(&harness));
    harness.key_press(egui::Key::Enter);
    harness.run();
    assert_eq!(harness.state().ui_state.selected_cluster, None);
}

/// More clusters than the window has room for: the header, strip and footer
/// stay while the tiles scroll, and the arrow keys keep the highlighted tile
/// in view.
#[test]
fn the_switcher_scrolls_its_tiles_in_a_short_window() {
    let mut harness = design_harness(egui::vec2(1536.0, 560.0));
    seed_design_clusters(&mut harness);
    {
        let ui = &mut harness.state_mut().ui_state;
        for index in 0..18 {
            ui.next_cluster_key += 1;
            let key = ui.next_cluster_key;
            ui.clusters.insert(
                key,
                super::super::super::state::ClusterState::for_test(
                    key,
                    format!("staging-{index:02}"),
                ),
            );
        }
    }
    press_command(&mut harness, egui::Key::K);
    rest_pointer(&mut harness);
    harness.get_by_label("Find a cluster");
    harness.get_by_label("Close");
    harness.ui_harness("clusters/switcher_many_clusters_short_window");

    for _ in 0..8 {
        harness.key_press(egui::Key::ArrowDown);
        harness.run();
    }
    let highlighted = harness
        .get(
            By::new()
                .role(egui::accesskit::Role::Button)
                .predicate(|node| {
                    node.toggled() == Some(egui::accesskit::Toggled::True)
                        && node.label().is_some_and(|label| label.contains(" · "))
                }),
        )
        .rect();
    let window = harness.get_by_label("Find a cluster").rect();
    assert!(
        highlighted.bottom() <= 560.0 && highlighted.top() > window.bottom(),
        "the highlighted tile {highlighted:?} scrolled into view below the search"
    );
}

/// A failed cluster with a node count seen before shows it, and says
/// "unreachable" only when no count is known.
#[test]
fn a_failed_cluster_keeps_its_last_seen_node_count() {
    let mut harness = switcher_harness();
    press_command(&mut harness, egui::Key::K);
    assert!(has_tile(&harness, "prod-eu, 62 nodes"));
    harness
        .state_mut()
        .ui_state
        .cluster_roster
        .known_nodes
        .remove("prod-eu");
    harness.run();
    assert!(has_tile(&harness, "prod-eu, unreachable"));
}

/// Enter on a found cluster adds it; once it is in the kubeconfig the window
/// switches to it.
#[test]
fn a_cluster_added_from_the_switcher_is_switched_to() {
    let mut harness = switcher_harness();
    press_command(&mut harness, egui::Key::K);
    let found = tile_centre(&harness, "payments");
    primary_click(&mut harness, found);
    harness.run();
    harness.deliver_worker_result(crate::worker::ImportedKubernetesClusters(vec![Cluster {
        name: "payments".into(),
        is_current: false,
        presentation: Default::default(),
        server: None,
    }]));
    let selected = harness.state().ui_state.selected_cluster.expect("selected");
    assert_eq!(harness.cluster(selected).name, "payments");
    assert!(connects_to(&harness, selected));
}
