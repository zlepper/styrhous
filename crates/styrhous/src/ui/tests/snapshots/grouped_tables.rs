//! Grouped resource tables (WP3b): group headers and collapsing, the filter
//! chips, stepping through problem rows, and the narrow window's columns.

use super::super::design_fixtures::{design_namespaces_harness, pods};
use super::*;
use components::test_support::primary_click;

type AppHarness = Harness<'static, MyEguiApp<MockWorker>>;

/// A window short enough that the third group's rows are off screen.
const SHORT_WINDOW: egui::Vec2 = egui::vec2(1536.0, 420.0);
/// A window big enough for every row, but not the board.
const COMPACT_BOARD: egui::Vec2 = egui::vec2(1280.0, 560.0);

fn namespaces_harness(size: egui::Vec2) -> AppHarness {
    let mut harness = design_namespaces_harness(size);
    harness.run();
    harness
}

fn has_label(harness: &AppHarness, label: &str) -> bool {
    harness.query_by_label(label).is_some()
}

/// The accessible label of the name cell of a pod's row.
fn row(name: &str) -> String {
    format!("Open details for {name}")
}

#[test]
fn the_group_chip_switches_between_namespace_headers_and_one_list() {
    let mut harness = namespaces_harness(COMPACT_BOARD);
    for namespace in ["kube-system", "monitoring", "payments"] {
        harness.get_by_label(&format!("Collapse {namespace}"));
    }

    click_label(&mut harness, "Group: namespace");
    harness.get_by_label("Group: none");
    assert!(!has_label(&harness, "Collapse monitoring"));
    assert!(
        has_label(&harness, &row("kindnet-9qrlh")),
        "every row still shows"
    );
    let key = ResourceTableKey::workspace(&pods());
    assert!(
        !harness
            .state()
            .resource_table_preferences
            .groups_by_namespace(&key),
        "the choice is persisted with the table"
    );

    click_label(&mut harness, "Group: none");
    harness.get_by_label("Collapse monitoring");
}

#[test]
fn the_problems_chip_hides_healthy_rows_and_its_count_stays_live() {
    let mut harness = namespaces_harness(COMPACT_BOARD);
    assert!(has_label(&harness, &row("kindnet-9qrlh")));

    click_label(&mut harness, "Problems only 2");
    harness.get_by_label("Problems only 2");
    assert!(!has_label(&harness, &row("kindnet-9qrlh")));
    harness.get_by_label(&row("api-7b948f-pod"));
    harness.get_by_label(&row("job-nightly-report-28xk2"));
    assert!(
        !has_label(&harness, "Collapse monitoring"),
        "a group without a problem has nothing to show"
    );
    harness.get_by_label("Collapse payments");

    harness.key_press(egui::Key::P);
    harness.run();
    assert!(
        has_label(&harness, &row("kindnet-9qrlh")),
        "P toggles the chip"
    );
}

#[test]
fn problems_only_with_nothing_wrong_offers_to_show_every_row() {
    let mut harness = namespaces_harness(COMPACT_BOARD);
    click_label(&mut harness, "Problems only 2");
    // Searching for a healthy pod leaves no problem row.
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::F);
    harness.run();
    harness.event(egui::Event::Text("kindnet".into()));
    harness.run();
    harness.get_by_label("No problems");

    click_label(&mut harness, "Clear filter");
    harness.get_by_label(&row("kindnet-9qrlh"));
    harness.get_by_label("Problems only 0");
    assert!(
        !harness.state().ui_state.clusters[&super::super::design_fixtures::KIND_KIND]
            .resource_searches[&pods()]
            .problems_only
    );
}

#[test]
fn a_collapsed_group_keeps_its_header_and_hides_its_rows() {
    let mut harness = namespaces_harness(COMPACT_BOARD);
    assert!(has_label(&harness, &row("kindnet-9qrlh")));

    click_label(&mut harness, "Collapse monitoring");
    harness.get_by_label("Expand monitoring");
    assert!(!has_label(&harness, &row("kindnet-9qrlh")));
    harness.get_by_label(&row("coredns-66bc5c9577-ffw2s"));
    harness.get_by_label(&row("api-7b948f-pod"));
    harness.ui_harness("grouped_tables/a_collapsed_group_keeps_its_header");

    click_label(&mut harness, "Expand monitoring");
    harness.get_by_label(&row("kindnet-9qrlh"));
}

#[test]
fn the_bracket_keys_step_through_problem_rows_and_scroll_to_them() {
    let mut harness = namespaces_harness(SHORT_WINDOW);
    assert!(
        !has_label(&harness, &row("api-7b948f-pod")),
        "the payments group starts below the window"
    );

    harness.key_press(egui::Key::CloseBracket);
    harness.run();
    harness.key_press(egui::Key::CloseBracket);
    harness.run();
    let api = harness.get_by_label(&row("api-7b948f-pod")).rect();
    assert!(
        api.center().y < SHORT_WINDOW.y,
        "the second problem scrolled into view, not below the window: {api:?}"
    );

    harness.key_press(egui::Key::OpenBracket);
    harness.run();
    harness.get_by_label(&row("job-nightly-report-28xk2"));
}

#[test]
fn clicking_the_problem_track_scrolls_to_the_nearest_problem() {
    let mut harness = namespaces_harness(SHORT_WINDOW);
    assert!(!has_label(&harness, &row("api-7b948f-pod")));

    let track = harness.get_by_label("2 problems").rect();
    primary_click(
        &mut harness,
        egui::pos2(track.center().x, track.bottom() - 2.0),
    );
    harness.run();
    let api = harness.get_by_label(&row("api-7b948f-pod")).rect();
    assert!(api.center().y < SHORT_WINDOW.y, "{api:?}");
}

#[test]
fn a_narrow_window_leaves_out_the_detail_columns_and_the_grouping_chip() {
    let wide = namespaces_harness(COMPACT_BOARD);
    for column in ["CPU column", "Node column", "Status column"] {
        wide.get_by_label(column);
    }
    wide.get_by_label("Group: namespace");

    let narrow = namespaces_harness(egui::vec2(1024.0, 560.0));
    narrow.get_by_label("Status column");
    narrow.get_by_label("Restarts column");
    for column in [
        "CPU column",
        "Memory column",
        "Node column",
        "Containers column",
    ] {
        assert!(!has_label(&narrow, column), "{column} is left out");
    }
    assert!(!has_label(&narrow, "Group: namespace"));
    narrow.get_by_label("Problems only 2");
}

#[test]
fn the_narrow_window_column_menu_offers_only_the_columns_it_shows() {
    let mut narrow = namespaces_harness(egui::vec2(1024.0, 560.0));
    let header = narrow.get_by_label("Status column").rect().center();
    secondary_click(&mut narrow, header);
    narrow.run_steps(2);
    narrow.get_by_role_and_label(egui::accesskit::Role::CheckBox, "Status");
    assert!(
        narrow
            .query_by_role_and_label(egui::accesskit::Role::CheckBox, "CPU")
            .is_none(),
        "a column the window leaves out cannot be toggled"
    );
}

#[test]
fn typing_p_in_the_search_field_types_it_instead_of_toggling_problems_only() {
    let mut harness = namespaces_harness(COMPACT_BOARD);
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::F);
    harness.run();
    harness.event(egui::Event::Text("p".into()));
    harness.key_press(egui::Key::P);
    harness.run();
    let search = &harness.state().ui_state.clusters[&super::super::design_fixtures::KIND_KIND]
        .resource_searches[&pods()];
    assert_eq!(search.query, "p");
    assert!(!search.problems_only, "the key went to the field");
}
