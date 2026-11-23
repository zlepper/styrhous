//! Kind Helm release action scenarios.

use super::*;
use crate::api_resource::ApiResource;
use crate::helm_release::StorageDriver;
use crate::ui::MyEguiApp;
use crate::worker::Worker;

const RELEASE_NAME: &str = "rollback-demo";
const CHART: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/charts/rollback-demo"
);

/// Runs `helm` against the Kind cluster the application under test targets.
fn helm(namespace: &str, storage: StorageDriver, args: &[&str]) {
    let output = std::process::Command::new("helm")
        .args(args)
        .args(["--namespace", namespace, "--kube-context", "kind-kind"])
        .env("HELM_DRIVER", storage.helm_driver_name())
        .output()
        .expect("helm must be installed for the Helm integration tests");
    assert!(
        output.status.success(),
        "helm {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_helm_rollback_integration() {
    roll_back_through_the_inspector("helm-rollback", StorageDriver::Secret);
}

/// Releases stored in ConfigMaps are only visible to `helm` with `HELM_DRIVER=configmap`.
#[test]
fn test_helm_rollback_configmap_storage_integration() {
    roll_back_through_the_inspector("helm-rollback-cm", StorageDriver::ConfigMap);
}

/// Rolls a real Helm release back through the inspector, which runs
/// `helm rollback` and lets the release watch pick up the new revision.
fn roll_back_through_the_inspector(test_name: &str, storage: StorageDriver) {
    let fixture = IntegrationNamespaceFixture::create(test_name, "anchor", "unused");
    let namespace = fixture.namespace.clone();
    helm(&namespace, storage, &["install", RELEASE_NAME, CHART]);
    helm(
        &namespace,
        storage,
        &["upgrade", RELEASE_NAME, CHART, "--set", "greeting=second"],
    );
    let settings_name = format!("{RELEASE_NAME}-settings");

    let (mut harness, cluster_key) = connected_kind_harness();
    wait_for_cluster_data(&mut harness, cluster_key);
    select_namespace(&mut harness, cluster_key, &namespace);
    select_resource(&mut harness, "Helm", "Helm Release");
    assert_eq!(
        harness.state().ui_state.clusters[&cluster_key].selected_api_resource,
        Some(ApiResource::helm_releases())
    );
    let latest_revision = |app: &MyEguiApp<Worker>| {
        let watch = app.ui_state.clusters[&cluster_key]
            .helm_release_cache
            .get(&namespace)
            .filter(|watch| watch.is_synced)?;
        watch
            .releases
            .iter()
            .filter(|release| release.name == RELEASE_NAME)
            .max_by_key(|release| release.revision)
            .map(|release| (release.revision, release.storage))
    };
    // Which of the release watch, the `helm` process and the operation outcome a wait is
    // still missing.
    let helm_state = |app: &MyEguiApp<Worker>| {
        let cluster = &app.ui_state.clusters[&cluster_key];
        let watch = cluster.helm_release_cache.get(&namespace);
        Some(format!(
            "release watch synced={:?}, backend errors={:?}, latest revision={:?}, \
             rollbacks in flight={}, operations={:?}",
            watch.map(|watch| watch.is_synced),
            watch.map(|watch| &watch.backend_errors),
            latest_revision(app),
            cluster.helm_rollbacks.ids().len(),
            cluster
                .operation_history
                .iter()
                .map(|entry| entry.title.as_str())
                .collect::<Vec<_>>(),
        ))
    };
    wait_for_with_terminal_and_timeout_diagnostic(
        &mut harness,
        "the Helm release watch to show revision 2",
        |app| latest_revision(app).filter(|(revision, _)| *revision == 2),
        |_| None,
        helm_state,
        10_000,
    );
    assert_eq!(latest_revision(harness.state()), Some((2, storage)));

    let inspect_label = format!("Inspect Helm release {RELEASE_NAME}");
    wait_for_settled_clickable(&mut harness, &inspect_label, 5_000);
    harness.get_by_label(&inspect_label).click();
    wait_for_settled_clickable(&mut harness, "Select revision 1", 5_000);
    harness.get_by_label("Select revision 1").click();
    wait_for_settled_clickable(&mut harness, "Roll back to revision 1", 5_000);
    harness.get_by_label("Roll back to revision 1").click();
    wait_for_settled_clickable(&mut harness, "Roll back", 5_000);
    harness.get_by_label("Roll back").click();
    harness.run_steps(1);
    let cluster = &harness.state().ui_state.clusters[&cluster_key];
    assert!(
        cluster.pending_helm_rollback.is_none() && !cluster.helm_rollbacks.ids().is_empty(),
        "the rollback confirmation click was not applied"
    );

    wait_for_kubernetes_with_diagnostic(
        &mut harness,
        "the rollback to restore revision 1's values",
        |remaining| {
            kubernetes_request(
                &fixture.runtime,
                remaining,
                fixture.configmaps.get(&settings_name),
            )
            .map(|configmap| {
                let greeting = configmap
                    .data
                    .and_then(|data| data.get("greeting").cloned());
                (greeting.as_deref() == Some("first")).then_some(())
            })
        },
        |app| latest_operation_failure(app, cluster_key),
        60_000,
    );
    wait_for_with_terminal_and_timeout_diagnostic(
        &mut harness,
        "the rollback outcome and revision 3 to reach the UI",
        |app| {
            let cluster = &app.ui_state.clusters[&cluster_key];
            (latest_revision(app) == Some((3, storage))
                && cluster.helm_rollbacks.ids().is_empty()
                && cluster
                    .operation_history
                    .iter()
                    .any(|entry| entry.title == "Helm release rolled back"))
            .then_some(())
        },
        |app| latest_operation_failure(app, cluster_key),
        helm_state,
        10_000,
    );
}
