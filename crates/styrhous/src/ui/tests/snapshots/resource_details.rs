//! Resource-detail and inspector-action scenarios.

use super::*;

#[test]
fn deployment_inspector_exposes_the_shared_restart_action() {
    let deployment = fixture_api_resource("apps", "Deployment", "deployments");
    let detail = ResourceDetail {
        api_resource: deployment.clone(),
        name: "coredns".into(),
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

    harness
        .get_by_label("More actions for coredns")
        .click_accesskit();
    harness.run();
    harness.get_by_label("Restart rollout").click_accesskit();
    harness.run();
    assert!(
        harness.state().ui_state.clusters[&2]
            .pending_deployment_restart
            .as_ref()
            .is_some_and(|pending| {
                pending.resource_name == "coredns" && pending.namespace == "kube-system"
            })
    );
}

#[test]
fn cron_job_inspector_run_action_sends_a_worker_command() {
    let cron_job = fixture_api_resource("batch", "CronJob", "cronjobs");
    let detail = ResourceDetail {
        api_resource: cron_job.clone(),
        name: "nightly-report".into(),
        namespace: Some("kube-system".into()),
        uid: "cron-job-uid".into(),
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
    open_typed_detail(&mut harness, cron_job, detail);

    harness
        .get_by_label("More actions for nightly-report")
        .click();
    harness.run();
    harness.get_by_label("Run now").click();
    harness.run();
    assert!(
        harness.state().ui_state.clusters[&2]
            .pending_cron_job_run
            .is_some()
    );
    harness.run();
    harness.get_by_label("Run now").click();
    harness.run();

    assert!(
        harness
            .state()
            .worker
            .commands
            .last()
            .and_then(|command| command.as_ref().as_any().downcast_ref::<RunCronJob>())
            .is_some_and(|command| command.cluster_key == 2
                && command.namespace == "kube-system"
                && command.resource_name == "nightly-report")
    );
}

#[test]
fn config_map_inspector_saves_only_changed_existing_data_values() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    let detail = config_map_detail(BTreeMap::from([
        ("mode".into(), "development".into()),
        ("unused".into(), "preserved".into()),
    ]));
    open_typed_detail(&mut harness, detail.api_resource.clone(), detail);

    let editor = harness
        .state_mut()
        .ui_state
        .global_blades
        .navigator_mut()
        .and_then(|navigator| navigator.current_mut().resource_detail_mut())
        .and_then(|entry| entry.data_editor.as_mut())
        .expect("ConfigMap editor should initialize from the detail payload");
    editor
        .draft_values
        .insert("mode".into(), "production".into());
    harness.run();
    harness.get_by_label("Save data").click_accesskit();
    harness.run();

    assert!(
        harness
            .state()
            .worker
            .commands
            .last()
            .is_some_and(|command| command
                .as_ref()
                .as_any()
                .downcast_ref::<UpdateResourceData>()
                .is_some_and(|command| {
                    command.history_entry_id > 0
                        && command.request_id > 0
                        && command.update.expected_resource_version == "1"
                        && command.update.expected_values
                            == BTreeMap::from([("mode".into(), "development".into())])
                        && command.update.updated_values
                            == BTreeMap::from([("mode".into(), "production".into())])
                }))
    );
}

#[test]
fn config_map_resource_detail_inspector_snapshot() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    let detail = config_map_detail(BTreeMap::from([
        ("app.toml".into(), "[server]\nport = 8080".into()),
        ("log-level".into(), "info".into()),
    ]));
    open_typed_detail(&mut harness, detail.api_resource.clone(), detail);

    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "resource_inspectors/config_map_resource_detail_inspector_snapshot/config_map_resource_detail_inspector",
    ));
}

#[test]
fn resource_detail_more_actions_use_the_shared_resource_menu() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    let detail = config_map_detail(BTreeMap::from([("mode".into(), "development".into())]));
    open_typed_detail(&mut harness, detail.api_resource.clone(), detail);

    harness
        .get_by_label("More actions for settings")
        .click_accesskit();
    harness.run();
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "resource_actions/resource_detail_more_actions_use_the_shared_resource_menu/config_map_resource_detail_actions",
    ));
    harness.get_by_label("Edit").click_accesskit();
    harness.run();
    assert!(
        harness
            .state()
            .worker
            .commands
            .iter()
            .rev()
            .nth(1)
            .is_some_and(|command| command
                .as_ref()
                .as_any()
                .downcast_ref::<GetResourceYaml>()
                .is_some_and(|command| command.resource_name == "settings"))
    );
    assert!(
        harness
            .state()
            .worker
            .commands
            .last()
            .is_some_and(|command| command
                .as_ref()
                .as_any()
                .downcast_ref::<LoadResourceSchema>()
                .is_some())
    );

    harness
        .get_by_label("More actions for settings")
        .click_accesskit();
    harness.run();
    harness.get_by_label("Delete").click_accesskit();
    harness.run();
    assert!(
        harness.state().ui_state.clusters[&2]
            .pending_delete
            .as_ref()
            .is_some_and(|pending| pending.resource_name == "settings")
    );
}

#[test]
fn secret_inspector_masks_values_and_prompts_for_a_real_external_change() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    let secrets = fixture_api_resource("core", "Secret", "secrets");
    let detail = ResourceDetail {
        api_resource: secrets.clone(),
        name: "credentials".into(),
        namespace: Some("kube-system".into()),
        uid: "secret-uid".into(),
        resource_version: "1".into(),
        is_deleting: false,
        finalizers: Vec::new(),
        creation_timestamp: None,
        owners: Vec::new(),
        labels: BTreeMap::new(),
        annotations: BTreeMap::new(),
        payload: ResourceDetailPayload::Secret(SecretDetail {
            data: BTreeMap::from([
                (
                    "password".into(),
                    SecretDataDetail {
                        byte_len: 6,
                        text: Some("secret".into()),
                    },
                ),
                (
                    "binary".into(),
                    SecretDataDetail {
                        byte_len: 2,
                        text: None,
                    },
                ),
            ]),
            immutable: false,
            type_: "Opaque".into(),
        }),
    };
    open_typed_detail(&mut harness, secrets, detail);

    harness.get_by_label("Reveal").click_accesskit();
    harness.run();
    harness.get_by_label("Hide");
    harness.get_by_label("Binary data");
    harness.get_by_label("This value cannot be edited in the inspector.");

    harness
        .state_mut()
        .ui_state
        .global_blades
        .navigator_mut()
        .and_then(|navigator| navigator.current_mut().resource_detail_mut())
        .and_then(|entry| entry.data_editor.as_mut())
        .expect("Secret editor should initialize")
        .draft_values
        .insert("password".into(), "changed".into());
    harness
        .state_mut()
        .worker
        .results
        .push_back(Box::new(ResourceDetailUpdated {
            cluster_key: 2,
            history_entry_id: 1,
            detail: Box::new(ResourceDetail {
                payload: ResourceDetailPayload::Secret(SecretDetail {
                    data: BTreeMap::from([(
                        "password".into(),
                        SecretDataDetail {
                            byte_len: 7,
                            text: Some("cluster".into()),
                        },
                    )]),
                    immutable: false,
                    type_: "Opaque".into(),
                }),
                ..config_map_detail(BTreeMap::new())
            }),
        }) as WorkerResultBox);
    harness.run();
    harness.get_by_label("Data changed on cluster");
    harness.get_by_label("Keep my edits").click_accesskit();
    harness.run();
    assert_eq!(
        harness
            .state()
            .ui_state
            .global_blades
            .navigator()
            .and_then(|navigator| navigator.current().resource_detail())
            .and_then(|entry| entry.data_editor.as_ref())
            .and_then(|editor| editor.draft_values.get("password"))
            .map(String::as_str),
        Some("changed")
    );
}

#[test]
fn secret_resource_detail_inspector_snapshot() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    let secrets = fixture_api_resource("core", "Secret", "secrets");
    let detail = ResourceDetail {
        api_resource: secrets.clone(),
        name: "api-credentials".into(),
        namespace: Some("kube-system".into()),
        uid: "secret-snapshot-uid".into(),
        resource_version: "1".into(),
        is_deleting: false,
        finalizers: Vec::new(),
        creation_timestamp: None,
        owners: Vec::new(),
        labels: BTreeMap::from([("app.kubernetes.io/name".into(), "api".into())]),
        annotations: BTreeMap::new(),
        payload: ResourceDetailPayload::Secret(SecretDetail {
            data: BTreeMap::from([
                (
                    "password".into(),
                    SecretDataDetail {
                        byte_len: 24,
                        text: Some("super-secret-password-value".into()),
                    },
                ),
                (
                    "certificate".into(),
                    SecretDataDetail {
                        byte_len: 1872,
                        text: Some("-----BEGIN CERTIFICATE-----\n…".into()),
                    },
                ),
                (
                    "binary-token".into(),
                    SecretDataDetail {
                        byte_len: 32,
                        text: None,
                    },
                ),
            ]),
            immutable: false,
            type_: "Opaque".into(),
        }),
    };
    open_typed_detail(&mut harness, secrets, detail);

    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "resource_inspectors/secret_resource_detail_inspector_snapshot/secret_resource_detail_inspector",
    ));
}

#[test]
fn resource_table_reflows_after_viewport_resize() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    harness.run();
    harness.get_by_label("Apps & Containers").click_accesskit();
    harness.run();

    harness.set_size(egui::vec2(1024.0, 1024.0));
    harness.run();
    harness.ui_harness(
        "resource_tables/resource_table_reflows_after_viewport_resize/resource_table_narrow",
    );

    components::test_support::setup_egui(&mut harness);
    harness.run();
    harness.ui_harness(
        "resource_tables/resource_table_reflows_after_viewport_resize/resource_table_resized",
    );
}
