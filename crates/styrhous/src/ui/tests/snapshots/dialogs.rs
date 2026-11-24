//! The confirmation dialogs of the design boards (WP3d): A06 restarts a
//! Deployment's rollout and A07 force deletes a stuck Pod by typing its name.

use super::super::design_fixtures::{
    KIND_KIND, STUCK_FINALIZERS, STUCK_POD, deliver_design_deployments, deliver_design_stuck_pod,
    deliver_design_unread_failure, design_harness,
};
use super::*;
use components::test_support::primary_click;

/// The Deployments of `kube-system`, with Workloads expanded as on A06.
fn deployments_harness() -> AppHarness {
    let mut harness = design_harness(BOARD_SIZE);
    deliver_design_unread_failure(&mut harness);
    click_label(&mut harness, "Dismiss");
    click_label(&mut harness, "Deployments");
    deliver_design_deployments(&mut harness);
    click_label(&mut harness, "Workloads");
    harness
}

/// Open the restart confirmation of the `coredns` Deployment from its row menu.
fn open_restart_confirmation(harness: &mut AppHarness) {
    click_label(harness, "More actions for coredns");
    click_label(harness, "Restart rollout");
    harness.get_by_label("Restart rollout?");
}

/// The Pods of `kube-system` with `job-nightly-report-28xk2` stuck deleting,
/// and its force delete confirmation open, as on A07, past its delay.
fn open_force_delete_confirmation() -> AppHarness {
    let mut harness = design_harness(BOARD_SIZE);
    deliver_design_unread_failure(&mut harness);
    click_label(&mut harness, "Dismiss");
    click_label(&mut harness, "Workloads");
    deliver_design_stuck_pod(&mut harness);
    click_label(&mut harness, &format!("More actions for {STUCK_POD}"));
    click_label(&mut harness, "Force delete (remove finalizers)");
    harness.get_by_label("Force delete resource?");
    // The confirmation delay is wall-clock time; skip past it.
    harness
        .state_mut()
        .ui_state
        .clusters
        .get_mut(&KIND_KIND)
        .and_then(|cluster| cluster.pending_force_delete.as_mut())
        .expect("force deletion is pending")
        .confirmation_available_at = std::time::Instant::now();
    harness.run();
    harness
}

fn type_name(harness: &mut AppHarness, name: &str) {
    harness.event(egui::Event::Text(name.into()));
    harness.run();
}

/// A06: restarting the `coredns` rollout, the Deployments table dimmed behind.
#[test]
fn a06_restart_rollout_confirmation() {
    let mut harness = deployments_harness();
    open_restart_confirmation(&mut harness);
    rest_pointer(&mut harness);
    harness.ui_harness("boards/a06_restart_rollout");
}

/// A07: the typed name matches, so Remove finalizers is ready.
#[test]
fn a07_force_delete_with_typed_name() {
    let mut harness = open_force_delete_confirmation();
    type_name(&mut harness, STUCK_POD);
    rest_pointer(&mut harness);
    harness.ui_harness("boards/a07_force_delete");
}

#[test]
fn restart_rollout_sends_the_command_and_closes_the_dialog() {
    let mut harness = deployments_harness();
    open_restart_confirmation(&mut harness);
    // The confirm button has focus when the dialog opens.
    harness.key_press(egui::Key::Enter);
    harness.run();

    assert!(
        harness
            .state()
            .worker
            .last_command::<RestartDeployment>()
            .is_some_and(|command| command.cluster_key == KIND_KIND
                && command.namespace == "kube-system"
                && command.resource_name == "coredns")
    );
    assert!(harness.query_by_label("Restart rollout?").is_none());
}

#[test]
fn escape_and_a_click_on_the_scrim_cancel_the_restart() {
    let mut harness = deployments_harness();
    open_restart_confirmation(&mut harness);
    harness.key_press(egui::Key::Escape);
    harness.run();
    assert!(harness.query_by_label("Restart rollout?").is_none());

    open_restart_confirmation(&mut harness);
    primary_click(&mut harness, egui::pos2(900.0, 900.0));
    harness.run();
    assert!(harness.query_by_label("Restart rollout?").is_none());
    assert!(
        harness
            .state()
            .worker
            .last_command::<RestartDeployment>()
            .is_none(),
        "cancelling restarts nothing"
    );
}

#[test]
fn force_delete_waits_for_the_typed_name_and_lists_the_finalizers() {
    let mut harness = open_force_delete_confirmation();
    for finalizer in STUCK_FINALIZERS {
        harness.get_by_label(finalizer);
    }
    assert!(
        harness
            .get_by_label("Remove finalizers")
            .accesskit_node()
            .is_disabled(),
        "nothing typed yet"
    );
    harness.get_by_label(&format!("Type {STUCK_POD} to confirm"));
    // The delay is wall-clock time; skip past it.
    harness
        .state_mut()
        .ui_state
        .clusters
        .get_mut(&KIND_KIND)
        .and_then(|cluster| cluster.pending_force_delete.as_mut())
        .expect("force deletion is pending")
        .confirmation_available_at = std::time::Instant::now();
    type_name(&mut harness, "job-nightly");
    assert!(
        harness
            .get_by_label("Remove finalizers")
            .accesskit_node()
            .is_disabled(),
        "a partial name does not confirm"
    );

    type_name(&mut harness, "-report-28xk2");
    click_label(&mut harness, "Remove finalizers");
    assert!(
        harness
            .state()
            .worker
            .last_command::<ForceDeleteResource>()
            .is_some_and(|command| command.resource_name == STUCK_POD)
    );
    assert!(harness.cluster(KIND_KIND).pending_force_delete.is_none());
}

#[test]
fn escape_in_the_confirmation_field_cancels_the_force_delete() {
    let mut harness = open_force_delete_confirmation();
    type_name(&mut harness, "job-");
    harness.key_press(egui::Key::Escape);
    harness.run();
    assert!(harness.cluster(KIND_KIND).pending_force_delete.is_none());
    assert!(
        harness
            .state()
            .worker
            .last_command::<ForceDeleteResource>()
            .is_none()
    );
}

#[test]
fn enter_in_the_confirmation_field_confirms_only_the_exact_name() {
    let mut harness = open_force_delete_confirmation();
    // Right length, wrong last character.
    type_name(&mut harness, "job-nightly-report-28xk3");
    harness.key_press(egui::Key::Enter);
    harness.run();
    assert!(
        harness
            .state()
            .worker
            .last_command::<ForceDeleteResource>()
            .is_none()
    );
    assert!(harness.cluster(KIND_KIND).pending_force_delete.is_some());

    // Enter surrendered the field's focus; click it and finish the name.
    let field = harness
        .get_by_role_and_label(
            egui::accesskit::Role::TextInput,
            &format!("Type {STUCK_POD} to confirm"),
        )
        .rect()
        .right_center()
        - egui::vec2(20.0, 0.0);
    primary_click(&mut harness, field);
    harness.run();
    harness.key_press(egui::Key::Backspace);
    harness.run();
    type_name(&mut harness, "2");
    harness.key_press(egui::Key::Enter);
    harness.run();
    assert!(
        harness
            .state()
            .worker
            .last_command::<ForceDeleteResource>()
            .is_some_and(|command| command.resource_name == STUCK_POD)
    );
    assert!(harness.cluster(KIND_KIND).pending_force_delete.is_none());
}

#[test]
fn a_click_on_the_scrim_cancels_the_force_delete() {
    let mut harness = open_force_delete_confirmation();
    primary_click(&mut harness, egui::pos2(1200.0, 900.0));
    harness.run();
    assert!(harness.cluster(KIND_KIND).pending_force_delete.is_none());
}
