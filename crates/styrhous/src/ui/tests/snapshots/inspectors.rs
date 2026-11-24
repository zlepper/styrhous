//! The resource inspectors (WP3c): A05, the pod inspector with its history of
//! owners behind it, and A13, the Helm release inspector.

use super::super::design_fixtures::{
    COREDNS_POD, KIND_KIND, KUBE_SYSTEM, coredns_pod_detail, coredns_pod_events,
    deliver_design_unread_failure, demo_helm_releases, design_harness, design_now,
    open_coredns_inspector_stack,
};
use super::*;
use components::test_support::primary_click;

/// A05: the pod `coredns-66bc5c9577-ffw2s` inspector over Deployment `coredns`
/// and its ReplicaSet, which fan out behind it with their real content.
#[test]
fn a05_pod_inspector_with_history() {
    let mut harness = design_harness(BOARD_SIZE);
    deliver_design_unread_failure(&mut harness);
    click_label(&mut harness, "Dismiss");
    click_label(&mut harness, "Workloads");
    open_coredns_inspector_stack(&mut harness);
    harness.event(egui::Event::PointerGone);
    harness.run_steps(4);
    harness.ui_harness("boards/a05_pod_inspector_with_history");
}

/// A13: the Helm release `demo` inspector, opened from the Helm releases list.
#[test]
fn a13_helm_release_inspector() {
    let mut harness = design_harness(BOARD_SIZE);
    deliver_design_unread_failure(&mut harness);
    click_label(&mut harness, "Dismiss");
    click_label(&mut harness, "Helm releases");
    harness.deliver_worker_result(HelmReleasesReplaced {
        cluster_key: KIND_KIND,
        namespace: KUBE_SYSTEM.into(),
        releases: demo_helm_releases(),
    });
    click_label(&mut harness, "Inspect Helm release demo");
    harness.event(egui::Event::PointerGone);
    harness.run_steps(4);
    harness.ui_harness("boards/a13_helm_release_inspector");
}

/// Choosing a section of the rail scrolls the body there and lights it: the
/// pod's Events section says its normal events are hidden.
#[test]
fn a05_pod_inspector_rail_scrolls_to_the_events_section() {
    let mut harness = design_harness(BOARD_SIZE);
    deliver_design_unread_failure(&mut harness);
    click_label(&mut harness, "Dismiss");
    open_coredns_inspector_stack(&mut harness);
    harness.get_by_label("Events, 2").click();
    harness.run_steps(6);
    harness.event(egui::Event::PointerGone);
    harness.run_steps(2);

    harness.get_by_label("No warning events. 2 normal events hidden.");
    assert!(
        harness.get_by_label("Events, 2").accesskit_node().toggled()
            == Some(egui::accesskit::Toggled::True),
        "the rail lights the section scrolled to"
    );
    harness.ui_harness("boards/a05_pod_inspector_events_section");
}

/// L opens the pod's logs and V its YAML, while no text field has the keys.
#[test]
fn pod_inspector_keys_open_logs_and_yaml() {
    let mut harness = design_harness(BOARD_SIZE);
    open_coredns_inspector_stack(&mut harness);

    harness.key_press(egui::Key::L);
    harness.run();
    let logs = harness
        .state()
        .ui_state
        .log_windows
        .values()
        .next()
        .expect("L opens a log window");
    assert_eq!(logs.targets[0].pod_name, COREDNS_POD);

    harness.key_press(egui::Key::V);
    harness.run();
    assert!(
        harness
            .state()
            .ui_state
            .yaml_editors
            .values()
            .any(|editor| editor.resource_name == COREDNS_POD),
        "V opens the YAML editor"
    );
}

/// Warning events show in a table; the normal ones wait behind a link.
#[test]
fn pod_inspector_shows_warning_events_and_hides_normal_ones_until_asked() {
    let mut harness = design_harness(BOARD_SIZE);
    open_coredns_inspector_stack(&mut harness);
    let mut events = coredns_pod_events();
    events.push(ResourceEvent {
        uid: "uid-event-BackOff".into(),
        type_: "Warning".into(),
        reason: "BackOff".into(),
        message: "Back-off restarting failed container".into(),
        source: Some("kubelet".into()),
        count: 3,
        last_timestamp: Some(design_now()),
    });
    harness.deliver_worker_result(ResourceEventsReplaced {
        cluster_key: KIND_KIND,
        history_entry_id: 3,
        events,
    });
    harness.get_by_label("Events, 3, 1 problem").click();
    harness.run_steps(6);

    harness.get_by_label("BackOff");
    assert!(harness.query_by_label("Scheduled").is_none());
    harness
        .get_by_label_contains("Show 2 normal events")
        .click();
    harness.run();
    harness.get_by_label("Scheduled");
    harness.get_by_label("Hide normal events").click();
    harness.run();
    assert!(harness.query_by_label("Scheduled").is_none());
}

fn current_inspector_name(harness: &AppHarness) -> String {
    harness
        .state()
        .ui_state
        .global_blades
        .navigator()
        .and_then(|navigator| navigator.current().resource_detail())
        .map(|entry| entry.resource_name.clone())
        .expect("an inspector is open")
}

/// A pod with containers offers Logs and Shell; one without containers shows
/// them disabled, and a Deployment has neither.
#[test]
fn logs_and_shell_follow_the_pods_containers() {
    let mut harness = design_harness(BOARD_SIZE);
    open_coredns_inspector_stack(&mut harness);
    for label in ["Logs", "Shell"] {
        assert!(
            !harness.get_by_label(label).accesskit_node().is_disabled(),
            "{label} is enabled for a pod with a container"
        );
    }

    let mut bare = coredns_pod_detail();
    if let ResourceDetailPayload::Pod(pod) = &mut bare.payload {
        pod.containers.clear();
        pod.log_containers.clear();
    }
    harness.deliver_worker_result(ResourceDetailUpdated {
        cluster_key: KIND_KIND,
        history_entry_id: 3,
        detail: Box::new(bare),
    });
    for label in ["Logs", "Shell"] {
        assert!(
            harness.get_by_label(label).accesskit_node().is_disabled(),
            "{label} is disabled without containers"
        );
    }
    harness.key_press(egui::Key::L);
    harness.run();
    assert!(harness.state().ui_state.log_windows.is_empty());

    harness.get_by_label("Go back two blades").click();
    harness.run_steps(4);
    assert_eq!(current_inspector_name(&harness), "coredns");
    assert!(harness.query_by_label("Logs").is_none());
    assert!(harness.query_by_label("Shell").is_none());
}

/// The node in the status line opens the node's inspector.
#[test]
fn the_status_line_node_opens_the_node_inspector() {
    let mut harness = design_harness(BOARD_SIZE);
    open_coredns_inspector_stack(&mut harness);
    let link = harness
        .get_all_by_label("kind-control-plane")
        .find(|node| node.rect().top() < 70.0)
        .expect("the status line names the node");
    link.click();
    harness.run();

    assert!(
        harness
            .state()
            .worker
            .commands
            .last()
            .is_some_and(|command| {
                command
                    .as_ref()
                    .as_any()
                    .downcast_ref::<StartResourceDetailWatch>()
                    .is_some_and(|command| {
                        command.api_resource.kind == "Node"
                            && command.resource_name == "kind-control-plane"
                    })
            })
    );
}

/// Clicking anywhere on the visible part of a back blade (a real pointer click,
/// on its content rather than a control) returns to that blade, and Escape closes
/// the stack.
#[test]
fn a_back_blade_click_goes_back_and_escape_closes_the_stack() {
    // On A05's 1536 x 1024 board the ReplicaSet shows between x 568 and 808,
    // the Deployment behind it between x 355 and 568.
    const REPLICA_SET_CONTENT: egui::Pos2 = egui::pos2(700.0, 600.0);
    const DEPLOYMENT_CONTENT: egui::Pos2 = egui::pos2(450.0, 600.0);
    let mut harness = design_harness(BOARD_SIZE);
    open_coredns_inspector_stack(&mut harness);
    harness.get_by_label("Go back one blade");
    primary_click(&mut harness, REPLICA_SET_CONTENT);
    harness.run_steps(4);
    assert_eq!(
        current_inspector_name(&harness),
        super::super::design_fixtures::COREDNS_REPLICA_SET
    );

    harness.key_press(egui::Key::Escape);
    harness.run_steps(4);
    assert!(harness.state().ui_state.global_blades.navigator().is_none());

    let mut harness = design_harness(BOARD_SIZE);
    open_coredns_inspector_stack(&mut harness);
    primary_click(&mut harness, DEPLOYMENT_CONTENT);
    harness.run_steps(4);
    assert_eq!(current_inspector_name(&harness), "coredns");
}

/// The inline YAML stops at 300 lines and says so.
#[test]
fn long_yaml_is_cut_at_the_line_limit_and_says_so() {
    let mut harness = design_harness(BOARD_SIZE);
    open_coredns_inspector_stack(&mut harness);
    let mut detail = coredns_pod_detail();
    detail.yaml = Some(
        (1..=400)
            .map(|line| format!("key{line}: value"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    harness.deliver_worker_result(ResourceDetailUpdated {
        cluster_key: KIND_KIND,
        history_entry_id: 3,
        detail: Box::new(detail),
    });
    harness
        .get_all_by_label("YAML")
        .rfind(|node| node.rect().left() < 990.0)
        .expect("the YAML rail entry")
        .click();
    harness.run_steps(6);

    harness.get_by_label("Showing the first 300 of 400 lines.");
    // The back blades have their own Edit YAML buttons (their bounds ignore the
    // visual transform): the front blade is the last in the tree.
    fn edit_yaml(harness: &AppHarness) -> egui_kittest::Node<'_> {
        harness
            .get_all_by_label("Edit YAML")
            .next_back()
            .expect("the front blade's Edit YAML button")
    }
    edit_yaml(&harness).scroll_to_me();
    harness.run_steps(3);
    edit_yaml(&harness).click();
    harness.run();
    assert!(
        harness
            .state()
            .ui_state
            .yaml_editors
            .values()
            .any(|editor| editor.resource_name == COREDNS_POD)
    );
}

/// V, like the Values button, jumps to the release's Values section.
#[test]
fn the_helm_v_key_jumps_to_the_values_section() {
    let mut harness = design_harness(BOARD_SIZE);
    click_label(&mut harness, "Helm releases");
    harness.deliver_worker_result(HelmReleasesReplaced {
        cluster_key: KIND_KIND,
        namespace: KUBE_SYSTEM.into(),
        releases: demo_helm_releases(),
    });
    click_label(&mut harness, "Inspect Helm release demo");
    let values_lit = |harness: &AppHarness| {
        harness
            .get_all_by_label("Values")
            .find(|node| node.rect().left() < 990.0 && node.rect().left() > 800.0)
            .expect("the Values rail entry")
            .accesskit_node()
            .toggled()
            == Some(egui::accesskit::Toggled::True)
    };
    assert!(!values_lit(&harness));
    harness.key_press(egui::Key::V);
    harness.run_steps(6);
    assert!(values_lit(&harness));
}
