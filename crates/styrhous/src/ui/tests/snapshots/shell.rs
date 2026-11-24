//! The application shell (WP3a): the floating sidebar, the licence banner and
//! the workspace frame, on the design boards A02, A04 and A15.

use super::super::design_fixtures::{
    KIND_KIND, deliver_design_node_metrics, deliver_design_outcomes, deliver_design_unread_failure,
    design_harness, design_namespaces_harness, nodes, pods,
};
use super::*;
use components::design::size;

/// A15's narrow window.
const NARROW_BOARD: egui::Vec2 = egui::vec2(1024.0, 1024.0);

fn expand_workloads(harness: &mut AppHarness) {
    click_label(harness, "Workloads");
}

/// A02: Pods of `kube-system` on `kind-kind`, Workloads expanded, two
/// operation toasts.
#[test]
fn a02_pods_table_with_toasts() {
    let mut harness = design_harness(BOARD_SIZE);
    expand_workloads(&mut harness);
    deliver_design_outcomes(&mut harness);
    rest_pointer(&mut harness);
    harness.ui_harness("boards/a02_pods_table_with_toasts");
}

/// A03: Pods of three namespaces under their group headers, the namespace
/// selector open and searched for "mo".
#[test]
fn a03_namespace_selector_open() {
    let mut harness = design_namespaces_harness(BOARD_SIZE);
    deliver_design_unread_failure(&mut harness);
    click_label(&mut harness, "Dismiss");
    expand_workloads(&mut harness);
    harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Namespace scope")
        .click();
    harness.run();
    harness
        .input_mut()
        .events
        .push(egui::Event::Text("mo".into()));
    harness.run();
    harness.ui_harness("boards/a03_namespace_selector_open");
}

/// A04: the Nodes of `kind-kind`, opened from the pinned kinds, with the row
/// menu of `kind-worker` open on its Shell submenu of debug images.
#[test]
fn a04_nodes() {
    let mut harness = design_harness(BOARD_SIZE);
    deliver_design_unread_failure(&mut harness);
    click_label(&mut harness, "Dismiss");
    expand_workloads(&mut harness);
    click_label(&mut harness, "Nodes");
    assert_eq!(
        harness.cluster(KIND_KIND).selected_api_resource,
        Some(nodes())
    );
    deliver_design_node_metrics(&mut harness);
    click_label(&mut harness, "More actions for kind-worker");
    click_label(&mut harness, "Shell");
    // The board hovers the first image.
    let busybox = center_of(&harness, "Busybox (General profile)");
    harness.event(egui::Event::PointerMoved(busybox));
    harness.run();
    harness.ui_harness("boards/a04_nodes");
}

/// A15: A02 in a 1024 point window. The sidebar stays; the top bar compacts.
#[test]
fn a15_pods_narrow_window() {
    let mut harness = design_harness(NARROW_BOARD);
    deliver_design_unread_failure(&mut harness);
    click_label(&mut harness, "Dismiss");
    expand_workloads(&mut harness);
    assert_eq!(
        harness.cluster(KIND_KIND).selected_api_resource,
        Some(pods())
    );
    rest_pointer(&mut harness);
    harness.ui_harness("boards/a15_pods_narrow_window");
}

fn center_of(harness: &AppHarness, label: &str) -> egui::Pos2 {
    harness.get_by_label(label).rect().center()
}

fn top_of(harness: &AppHarness, label: &str) -> f32 {
    harness.get_by_label(label).rect().top()
}

/// The card of a context that is still connecting says so, and an
/// unreachable one explains why on hover.
#[test]
fn cluster_card_reports_a_connecting_or_unreachable_cluster() {
    let mut harness = design_harness(BOARD_SIZE);
    harness
        .state_mut()
        .ui_state
        .clusters
        .get_mut(&KIND_KIND)
        .expect("kind-kind exists")
        .connection = ClusterConnectionState::Connecting;
    harness.run();
    assert_eq!(
        harness
            .get_by_label("Switch cluster")
            .accesskit_node()
            .description()
            .as_deref(),
        Some("kind-kind, CONNECTING · 2 OF 4")
    );

    harness
        .state_mut()
        .ui_state
        .clusters
        .get_mut(&KIND_KIND)
        .expect("kind-kind exists")
        .connection = ClusterConnectionState::Failed("connection refused".into());
    harness.run();
    harness.get_by_label("Switch cluster").hover();
    harness.run_steps(30);
    assert!(harness.query_all_by_label("connection refused").count() > 0);
}

/// `G` focuses the kind filter, typing narrows the list to matching kinds,
/// and Enter opens the best match.
#[test]
fn kind_filter_focuses_on_g_and_opens_the_best_match_on_enter() {
    let mut harness = design_harness(BOARD_SIZE);
    harness.key_press(egui::Key::G);
    harness.run();
    assert!(
        harness
            .get_by_label("Filter kinds")
            .accesskit_node()
            .is_focused(),
        "G focuses the kind filter"
    );
    harness.event(egui::Event::Text("set".into()));
    harness.run();
    assert_eq!(harness.state().ui_state.kind_filter, "set");
    harness.get_by_label("StatefulSets");
    harness.get_by_label("DaemonSets");
    assert!(harness.query_by_label("Pods").is_none());
    assert!(harness.query_by_label("Workloads").is_none());
    rest_pointer(&mut harness);
    harness.ui_harness("shell/kind_filter_matches");

    harness.key_press(egui::Key::Enter);
    harness.run();
    assert_eq!(
        harness
            .cluster(KIND_KIND)
            .selected_api_resource
            .as_ref()
            .map(|resource| resource.name.as_str()),
        Some("statefulsets")
    );
    assert!(harness.state().ui_state.kind_filter.is_empty());
    harness.get_by_label("Workloads");

    // Enter on a filter that matches nothing opens nothing and keeps the text.
    harness.key_press(egui::Key::G);
    harness.run();
    harness.event(egui::Event::Text("zzz".into()));
    harness.run();
    harness.key_press(egui::Key::Enter);
    harness.run();
    assert_eq!(harness.state().ui_state.kind_filter, "zzz");
    assert_eq!(
        harness
            .cluster(KIND_KIND)
            .selected_api_resource
            .as_ref()
            .map(|resource| resource.name.as_str()),
        Some("statefulsets")
    );
    harness.state_mut().ui_state.kind_filter.clear();
    harness.run();

    // Enter on an empty filter opens nothing.
    harness.key_press(egui::Key::G);
    harness.run();
    harness.key_press(egui::Key::Enter);
    harness.run();
    assert_eq!(
        harness
            .cluster(KIND_KIND)
            .selected_api_resource
            .as_ref()
            .map(|resource| resource.name.as_str()),
        Some("statefulsets")
    );
}

/// A kind's context menu pins it under Pinned (out of its group) or unpins
/// it back into its group.
#[test]
fn pinning_a_kind_moves_it_between_pinned_and_its_group() {
    let mut harness = design_harness(BOARD_SIZE);
    expand_workloads(&mut harness);
    let jobs = center_of(&harness, "Jobs");
    secondary_click(&mut harness, jobs);
    harness.run();
    click_label(&mut harness, "Pin to sidebar");
    assert!(
        harness
            .state()
            .ui_state
            .pinned_kinds
            .kinds
            .iter()
            .any(|kind| kind.name == "jobs")
    );
    assert!(
        top_of(&harness, "Jobs") < top_of(&harness, "KUBERNETES"),
        "a pinned kind shows under Pinned"
    );
    assert_eq!(harness.get_all_by_label("Jobs").count(), 1);

    let nodes_item = center_of(&harness, "Nodes");
    secondary_click(&mut harness, nodes_item);
    harness.run();
    click_label(&mut harness, "Unpin from sidebar");
    assert!(
        harness.query_by_label("Nodes").is_none(),
        "an unpinned kind goes back into its collapsed group"
    );
    click_label(&mut harness, "Cluster");
    assert!(top_of(&harness, "Nodes") > top_of(&harness, "Cluster"));
}

/// Counts show what the app's watches have listed in the selected
/// namespaces; a kind not opened this session has none.
#[test]
fn kind_counts_show_only_what_watches_have_listed() {
    let mut harness = design_harness(BOARD_SIZE);
    let description = |harness: &AppHarness, label: &str| -> Option<String> {
        harness
            .get_by_label(label)
            .accesskit_node()
            .description()
            .map(|description| description.to_string())
    };
    assert_eq!(description(&harness, "Pods").as_deref(), Some("12"));
    assert_eq!(description(&harness, "Deployments").as_deref(), Some("4"));
    assert_eq!(description(&harness, "Nodes").as_deref(), Some("3"));
    assert_eq!(description(&harness, "Helm releases").as_deref(), Some("2"));
    assert_eq!(description(&harness, "Namespaces").as_deref(), Some("5"));
    assert_eq!(description(&harness, "Events").as_deref(), Some("42"));
    // Groups count their kinds.
    assert_eq!(description(&harness, "Workloads").as_deref(), Some("6"));
    click_label(&mut harness, "Network");
    assert_eq!(description(&harness, "Services"), None);
}

/// Settings and Notifications show selected while their blade is open.
#[test]
fn footer_items_show_selected_while_their_blade_is_open() {
    let mut harness = design_harness(BOARD_SIZE);
    let selected = |harness: &AppHarness, label: &str| {
        harness
            .get_all_by_label(label)
            .find(|node| node.rect().right() <= size::WORKSPACE_LEFT)
            .expect("the sidebar has the item")
            .accesskit_node()
            .data()
            .toggled()
            == Some(egui::accesskit::Toggled::True)
    };
    assert!(!selected(&harness, "Settings"));
    click_label(&mut harness, "Settings");
    harness.run_steps(2);
    assert!(selected(&harness, "Settings"));
    assert!(!selected(&harness, "Notifications"));
}

/// The unread count rides on Notifications as a danger badge and in its
/// description; a three-digit count grows the badge left.
#[test]
fn notifications_count_unread_outcomes() {
    let mut harness = design_harness(BOARD_SIZE);
    for index in 0..120 {
        harness.state_mut().ui_state.record_operation_outcome(
            KIND_KIND,
            crate::ui::state::OperationOutcome::Success,
            "Deployment scaled",
            Some("kube-system"),
            format!("deployment-{index}"),
            None,
        );
    }
    harness.run();
    assert_eq!(
        harness
            .get_by_label("Notifications")
            .accesskit_node()
            .description()
            .as_deref(),
        Some("120 unread")
    );
    harness.ui_harness("shell/notifications_count_unread_outcomes");
}

/// Settings carries a lamp and says why in its tooltip while an update or the
/// licence needs attention.
#[test]
fn settings_shows_its_update_status() {
    let mut harness = design_harness(BOARD_SIZE);
    harness
        .state_mut()
        .updater
        .set_status_for_test(UpdateStatus::Checking);
    harness.run();
    harness.get_by_label("Settings").hover();
    harness.run_steps(30);
    harness.ui_harness("shell/settings_shows_its_update_status");
}

/// With every group open the kind list scrolls under the fade, and the
/// footer stays put.
#[test]
fn footer_stays_clear_of_an_overflowing_kind_list() {
    let mut harness = design_harness(BOARD_SIZE);
    for group in [
        "Workloads",
        "Network",
        "Config & secrets",
        "Storage",
        "Cluster",
        "Access control",
    ] {
        harness
            .state_mut()
            .ui_state
            .set_resource_navigation_node_expanded(format!("section:{group}"), true);
    }
    harness.run();
    let settings = harness.get_by_label("Settings").rect();
    assert_eq!(
        settings.bottom(),
        BOARD_SIZE.y - size::SIDEBAR_INSET - size::SIDEBAR_PADDING,
        "the footer stays at the bottom of the sidebar"
    );
    harness.ui_harness("shell/footer_stays_clear_of_an_overflowing_kind_list");
}

fn grace_lease() -> VerifiedLease {
    super::global_blades::fixed_lease("grace", "subscription_past_due")
}

/// A11: the licence warning spans the window above the sidebar and the
/// workspace; blades open below it; dismissing it hides it for the session.
#[test]
fn license_banner_spans_the_window_until_dismissed() {
    let mut harness = design_harness(BOARD_SIZE);
    harness
        .state_mut()
        .licensing
        .set_status_for_test(LicenseStatus::Licensed(grace_lease()));
    harness.run();
    let sidebar = harness
        .get_by_role_and_label(egui::accesskit::Role::Navigation, "Workspace")
        .rect();
    assert_eq!(
        sidebar.top(),
        36.0 + 12.0,
        "the sidebar sits under the banner"
    );
    rest_pointer(&mut harness);
    harness.ui_harness("shell/license_banner_spans_the_window");

    click_label(&mut harness, "Manage license");
    harness.run_steps(4);
    let blade_top = harness
        .get_by_label("License device display name")
        .rect()
        .top();
    assert!(blade_top > 36.0, "blades open below the banner");
    harness.key_press(egui::Key::Escape);
    harness.run_steps(4);

    click_label(&mut harness, "Dismiss");
    assert!(
        harness
            .query_by_role_and_label(egui::accesskit::Role::Link, "Manage license")
            .is_none()
    );
    let sidebar = harness
        .get_by_role_and_label(egui::accesskit::Role::Navigation, "Workspace")
        .rect();
    assert_eq!(sidebar.top(), 12.0);

    // Once the warning is gone, the same warning returning is shown again.
    harness
        .state_mut()
        .licensing
        .set_status_for_test(LicenseStatus::Unconfigured);
    harness.run();
    harness
        .state_mut()
        .licensing
        .set_status_for_test(LicenseStatus::Licensed(grace_lease()));
    harness.run();
    harness.get_by_role_and_label(egui::accesskit::Role::Link, "Manage license");
}
