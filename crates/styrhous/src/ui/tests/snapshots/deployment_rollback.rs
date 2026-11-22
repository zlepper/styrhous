//! Deployment rollback dialog scenarios (GitHub issue #91).

use super::*;
use crate::cluster_connection_manager::{
    DeploymentRevision, DeploymentRevisionHistory, DeploymentRollbackOutcome, TemplateChange,
};
use crate::ui::state::DeploymentRevisionsLoad;

type AppHarness = Harness<'static, MyEguiApp<MockWorker>>;

fn deployments_state() -> UiState {
    let deployment = fixture_api_resource("apps", "Deployment", "deployments");
    let mut state = oracle_resource_table_state();
    let cluster = state.clusters.get_mut(&2).expect("kind fixture exists");
    cluster.selected_api_resource = Some(deployment.clone());
    cluster.resource_cache.insert(
        (deployment, Some("kube-system".to_owned())),
        ResourceWatchState {
            resources: BTreeMap::from([(
                "deployment-uid".to_owned(),
                MinimalResource {
                    uid: "deployment-uid".to_owned(),
                    name: "web".to_owned(),
                    namespace: Some("kube-system".to_owned()),
                    creation_timestamp: None,
                    controller_owner: None,
                    labels: Default::default(),
                    annotations: Default::default(),
                    cells: BTreeMap::new(),
                    log_containers: Vec::new(),
                },
            )]),
            is_synced: true,
            error: None,
            revision: 0,
        },
    );
    state
}

fn change(field: &str, current: Option<&str>, target: Option<&str>) -> TemplateChange {
    TemplateChange {
        container: "web".to_owned(),
        field: field.to_owned(),
        current: current.map(str::to_owned),
        target: target.map(str::to_owned),
    }
}

fn revision(
    revision: i64,
    image: &str,
    days_old: i64,
    changes: Vec<TemplateChange>,
) -> DeploymentRevision {
    DeploymentRevision {
        revision,
        replica_set: format!("web-{revision}"),
        created: Some(OffsetDateTime::now_utc() - time::Duration::days(days_old)),
        images: vec![image.to_owned()],
        is_current: changes.is_empty(),
        changes,
        other_changes: false,
    }
}

fn history(paused: bool) -> DeploymentRevisionHistory {
    DeploymentRevisionHistory {
        paused,
        revisions: vec![
            revision(3, "nginx:1.27", 1, Vec::new()),
            revision(
                2,
                "nginx:1.26",
                5,
                vec![
                    change("env LOG_LEVEL", Some("debug"), Some("info")),
                    change("image", Some("nginx:1.27"), Some("nginx:1.26")),
                ],
            ),
            revision(
                1,
                "nginx:1.25",
                20,
                vec![
                    change("image", Some("nginx:1.27"), Some("nginx:1.25")),
                    change("requests.cpu", Some("200m"), None),
                ],
            ),
        ],
    }
}

/// The Deployments table of the kind fixture cluster.
fn deployments_table() -> AppHarness {
    let mut harness = application_harness::<MockWorker>();
    harness.seed_ui_state(deployments_state());
    harness.get_by_label("Apps & Containers").click();
    harness.run();
    harness.get_by_label("Deployments").click();
    harness.run();
    harness
}

/// Chooses "Roll back…" for `web` and returns the revisions request id.
fn choose_roll_back(harness: &mut AppHarness) -> u64 {
    harness.get_by_label("More actions for web").click();
    harness.run();
    harness.get_by_label("Roll back…").click();
    harness.run();
    harness.event(egui::Event::PointerGone);
    harness.run();
    let request = harness
        .state()
        .worker
        .last_command::<GetDeploymentRevisions>()
        .expect("revisions requested");
    assert_eq!(
        (
            request.cluster_key,
            request.namespace.as_str(),
            request.resource_name.as_str()
        ),
        (2, "kube-system", "web")
    );
    request.request_id
}

fn deliver_history(harness: &mut AppHarness, request_id: u64, history: DeploymentRevisionHistory) {
    harness.deliver_worker_result(DeploymentRevisionsFetched {
        cluster_key: 2,
        request_id,
        history,
    });
    harness.event(egui::Event::PointerGone);
    harness.run();
}

/// Opens the dialog and loads `history` into it.
fn loaded_dialog(history: DeploymentRevisionHistory) -> AppHarness {
    let mut harness = deployments_table();
    let request_id = choose_roll_back(&mut harness);
    deliver_history(&mut harness, request_id, history);
    harness
}

fn confirm(harness: &mut AppHarness, label: &str) -> u64 {
    harness.get_by_label(label).click();
    harness.run();
    harness.event(egui::Event::PointerGone);
    harness.run();
    harness
        .state()
        .worker
        .last_command::<RollbackDeployment>()
        .expect("rollback sent")
        .operation_id
}

fn rolled_back(
    cluster_key: i32,
    operation_id: u64,
    outcome: DeploymentRollbackOutcome,
) -> DeploymentRolledBack {
    DeploymentRolledBack {
        cluster_key,
        operation_id,
        revision: 2,
        outcome,
    }
}

fn history_entries(harness: &AppHarness) -> Vec<(String, Option<String>)> {
    harness.state().ui_state.clusters[&2]
        .operation_history
        .iter()
        .map(|entry| (entry.title.clone(), entry.details.clone()))
        .collect()
}

fn is_disabled(harness: &AppHarness, label: &str) -> bool {
    harness.get_by_label(label).accesskit_node().is_disabled()
}

fn dialog_is_open(harness: &AppHarness) -> bool {
    harness.state().ui_state.clusters[&2]
        .pending_deployment_rollback
        .is_some()
}

#[test]
fn deployment_rollback_loads_revisions_shows_changes_and_rolls_back() {
    let mut harness = deployments_table();
    let request_id = choose_roll_back(&mut harness);
    harness.get_by_label("Loading revisions…");
    assert!(is_disabled(&harness, "Roll back"));
    harness.ui_harness(
        "deployment_rollback/deployment_rollback_loads_revisions_shows_changes_and_rolls_back/loading",
    );

    deliver_history(&mut harness, request_id, history(false));
    harness.get_by_label("nginx:1.26");
    harness.ui_harness(
        "deployment_rollback/deployment_rollback_loads_revisions_shows_changes_and_rolls_back/previous_revision",
    );

    harness.get_by_label("Revision").click();
    harness.run();
    harness
        .get_by_label_contains("Revision 1 · nginx:1.25")
        .click();
    harness.run();
    harness.event(egui::Event::PointerGone);
    harness.run();
    harness.get_by_label("200m");
    harness.ui_harness(
        "deployment_rollback/deployment_rollback_loads_revisions_shows_changes_and_rolls_back/older_revision",
    );

    assert_eq!(confirm(&mut harness, "Roll back to revision 1"), 1);
    let rollback = harness
        .state()
        .worker
        .last_command::<RollbackDeployment>()
        .expect("rollback sent");
    assert_eq!(
        (
            rollback.cluster_key,
            rollback.namespace.as_str(),
            rollback.resource_name.as_str(),
            rollback.revision
        ),
        (2, "kube-system", "web", 1)
    );
    assert!(!dialog_is_open(&harness));

    for _ in 0..2 {
        // The duplicate result must be ignored.
        harness.deliver_worker_result(DeploymentRolledBack {
            revision: 1,
            ..rolled_back(2, 1, DeploymentRollbackOutcome::RolledBack)
        });
    }
    assert_eq!(
        history_entries(&harness),
        vec![(
            "Deployment rolled back".to_owned(),
            Some("Rolling out revision 1".to_owned())
        )]
    );
    assert!(
        harness
            .query_by_label_contains("web • kube-system")
            .is_some()
    );
}

#[test]
fn deployment_rollback_blocks_a_second_rollback_until_the_first_finishes_and_reports_both() {
    let mut harness = loaded_dialog(history(false));
    let first = confirm(&mut harness, "Roll back to revision 2");

    let request_id = choose_roll_back(&mut harness);
    deliver_history(&mut harness, request_id, history(false));
    harness.get_by_label("A rollback of this Deployment is still running.");
    assert!(is_disabled(&harness, "Roll back to revision 2"));

    harness.deliver_worker_result(rolled_back(
        2,
        first,
        DeploymentRollbackOutcome::AlreadyAtRevision,
    ));
    assert!(!is_disabled(&harness, "Roll back to revision 2"));
    let second = confirm(&mut harness, "Roll back to revision 2");
    assert_ne!(first, second);
    harness.deliver_worker_result(DeploymentRollbackFailed {
        cluster_key: 2,
        operation_id: second,
        revision: 2,
        error: "Revision 2 of Deployment web no longer exists".to_owned(),
    });

    assert_eq!(
        history_entries(&harness),
        vec![
            (
                "Deployment already at that revision".to_owned(),
                Some("The pod template already matches revision 2".to_owned())
            ),
            (
                "Couldn’t roll back to revision 2".to_owned(),
                Some("Revision 2 of Deployment web no longer exists".to_owned())
            ),
        ]
    );
}

#[test]
fn deployment_rollback_can_be_cancelled_and_escape_closes_the_revision_picker_first() {
    let mut harness = loaded_dialog(history(false));
    harness.get_by_label("Cancel").click();
    harness.run();
    assert!(!dialog_is_open(&harness));

    let request_id = choose_roll_back(&mut harness);
    deliver_history(&mut harness, request_id, history(false));
    harness.get_by_label("Revision").click();
    harness.run();
    assert!(
        harness
            .query_by_label_contains("Revision 1 · nginx:1.25")
            .is_some()
    );
    harness.key_press(egui::Key::Escape);
    harness.run();
    assert!(
        harness
            .query_by_label_contains("Revision 1 · nginx:1.25")
            .is_none(),
        "the first Escape closes only the picker"
    );
    assert!(dialog_is_open(&harness));

    harness.key_press(egui::Key::Escape);
    harness.run();
    assert!(!dialog_is_open(&harness));
    assert!(
        harness
            .state()
            .worker
            .last_command::<RollbackDeployment>()
            .is_none()
    );
}

#[test]
fn deployment_rollback_of_a_paused_deployment_is_unavailable() {
    let mut harness = loaded_dialog(history(true));
    harness.get_by_label("Resume the Deployment's rollout before rolling it back.");
    assert!(is_disabled(&harness, "Roll back to revision 2"));
    harness.ui_harness(
        "deployment_rollback/deployment_rollback_of_a_paused_deployment_is_unavailable/paused",
    );
}

#[test]
fn deployment_rollback_scrolls_a_long_diff_and_flags_other_differences() {
    let image = |tag: &str| {
        format!("123456789012.dkr.ecr.eu-west-1.amazonaws.com/payments/checkout-api:{tag}")
    };
    let mut changes = vec![change(
        "image",
        Some(&image("2026.11.20-1")),
        Some(&image("2026.10.02-7")),
    )];
    changes.extend((0..20).map(|index| {
        change(
            &format!("env FEATURE_FLAG_{index:02}"),
            Some("enabled"),
            Some("disabled"),
        )
    }));
    let mut long_diff = revision(1, &image("2026.10.02-7"), 40, changes);
    long_diff.other_changes = true;
    let mut command_only = revision(2, &image("2026.11.01-3"), 10, Vec::new());
    command_only.is_current = false;
    command_only.other_changes = true;
    let mut current = revision(3, &image("2026.11.20-1"), 1, Vec::new());
    current.images.push("envoyproxy/envoy:v1.32.1".to_owned());
    let mut harness = loaded_dialog(DeploymentRevisionHistory {
        paused: false,
        revisions: vec![current, command_only, long_diff],
    });

    harness.get_by_label("Images, environment values and resources are unchanged.");
    harness.get_by_label("Other pod template fields differ as well.");

    harness.get_by_label("Revision").click();
    harness.run();
    harness.get_by_label_contains("Revision 1 · ").click();
    harness.run();
    harness.event(egui::Event::PointerGone);
    harness.run();
    assert!(
        harness.get_by_label("Roll back to revision 1").rect().max.y
            <= harness.ctx.content_rect().max.y,
        "the actions stay on screen however long the diff is"
    );
    harness.ui_harness(
        "deployment_rollback/deployment_rollback_scrolls_a_long_diff_and_flags_other_differences/long_diff",
    );
}

#[test]
fn deployment_rollback_without_earlier_revisions_or_with_a_failed_load_cannot_confirm() {
    let mut harness = deployments_table();
    let first_request = choose_roll_back(&mut harness);
    let mut only_current = history(false);
    only_current.revisions.truncate(1);
    deliver_history(&mut harness, first_request, only_current);
    harness.get_by_label("This Deployment has no earlier revision to roll back to.");
    assert!(is_disabled(&harness, "Roll back"));

    harness.key_press(egui::Key::Escape);
    harness.run();
    assert!(!dialog_is_open(&harness));
    let second_request = choose_roll_back(&mut harness);
    assert_ne!(first_request, second_request);

    // A reply for the dialog that was closed must not fill the new one.
    deliver_history(&mut harness, first_request, history(false));
    harness.get_by_label("Loading revisions…");
    harness.deliver_worker_result(DeploymentRevisionsFetchFailed {
        cluster_key: 2,
        request_id: second_request,
        error: "deployments.apps \"web\" is forbidden".to_owned(),
    });
    harness.event(egui::Event::PointerGone);
    harness.run();
    harness.get_by_label("Couldn’t load revisions: deployments.apps \"web\" is forbidden");
    assert!(is_disabled(&harness, "Roll back"));
    harness.ui_harness(
        "deployment_rollback/deployment_rollback_without_earlier_revisions_or_with_a_failed_load_cannot_confirm/load_failed",
    );
}

#[test]
fn deployment_rollback_replies_are_scoped_to_their_cluster_and_session() {
    let mut harness = deployments_table();
    let stale_request = choose_roll_back(&mut harness);
    let mut state = std::mem::take(&mut harness.state_mut().ui_state);
    let cluster = state.clusters.get_mut(&2).expect("kind fixture exists");
    let stale_operation = cluster
        .deployment_rollbacks
        .start(crate::ui::state::DeploymentKey {
            namespace: "kube-system".to_owned(),
            resource_name: "web".to_owned(),
        });
    // Reconnect: the session state is rebuilt, then the workspace reloads.
    cluster.reset_for_connection();
    let reloaded = deployments_state()
        .clusters
        .remove(&2)
        .expect("kind fixture exists");
    cluster.connection = ClusterConnectionState::Connected;
    cluster.namespaces_load = crate::ui::state::ClusterLoadState::Ready;
    cluster.reachable = true;
    cluster.api_discovery.complete = true;
    cluster.namespaces = reloaded.namespaces;
    cluster.selected_namespaces = reloaded.selected_namespaces;
    cluster.resources.selected_api_resource = reloaded.resources.selected_api_resource;
    cluster.resources.resource_navigation = reloaded.resources.resource_navigation;
    cluster.resources.resource_cache = reloaded.resources.resource_cache;
    harness.seed_ui_state(state);

    let request = choose_roll_back(&mut harness);
    assert_ne!(
        request, stale_request,
        "request ids continue across reconnects"
    );
    for (cluster_key, request_id) in [(2, stale_request), (3, request)] {
        harness.deliver_worker_result(DeploymentRevisionsFetched {
            cluster_key,
            request_id,
            history: history(false),
        });
    }
    harness.get_by_label("Loading revisions…");
    assert!(matches!(
        harness.state().ui_state.clusters[&2]
            .pending_deployment_rollback
            .as_ref()
            .map(|pending| &pending.revisions),
        Some(DeploymentRevisionsLoad::Loading)
    ));

    deliver_history(&mut harness, request, history(false));
    let operation = confirm(&mut harness, "Roll back to revision 2");
    assert_ne!(operation, stale_operation);
    for (cluster_key, operation_id) in [(2, stale_operation), (3, operation)] {
        harness.deliver_worker_result(rolled_back(
            cluster_key,
            operation_id,
            DeploymentRollbackOutcome::RolledBack,
        ));
    }
    assert!(history_entries(&harness).is_empty());
    assert_eq!(
        harness.state().ui_state.clusters[&2]
            .deployment_rollbacks
            .ids(),
        vec![operation]
    );
}

#[test]
fn deployment_inspector_offers_roll_back() {
    let deployment = fixture_api_resource("apps", "Deployment", "deployments");
    let detail = ResourceDetail {
        api_resource: deployment.clone(),
        name: "web".into(),
        namespace: Some("kube-system".into()),
        uid: "deployment-uid".into(),
        resource_version: "1".into(),
        is_deleting: false,
        finalizers: Vec::new(),
        creation_timestamp: None,
        owners: Vec::new(),
        labels: BTreeMap::new(),
        annotations: BTreeMap::new(),
        payload: ResourceDetailPayload::Generic,
    };
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    open_typed_detail(&mut harness, deployment, detail);

    let request_id = choose_roll_back(&mut harness);
    deliver_history(&mut harness, request_id, history(false));
    harness.get_by_label("Roll back web?");
    harness.key_press(egui::Key::Escape);
    harness.run();
    assert!(!dialog_is_open(&harness));
    assert!(
        harness.query_by_label("More actions for web").is_some(),
        "Escape dismisses only the dialog, not the inspector beneath it"
    );
}
