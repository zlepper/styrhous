use super::*;

#[test]
fn pod_resource_detail_inspector_snapshot() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    harness.run();
    harness.get_by_label("Apps & Containers").click_accesskit();
    harness.run();

    let name = "coredns-66bc5c9577-ffw2s";
    let resource_position = harness
        .get_by_label(&format!("Open details for {name}"))
        .rect()
        .center();
    primary_click(&mut harness, resource_position);
    harness.run_steps(1);
    let pods = fixture_api_resource("core", "Pod", "pods");
    harness.state_mut().worker.results.extend([
        Box::new(ResourceDetailUpdated {
            cluster_key: 2,
            history_entry_id: 1,
            detail: Box::new(ResourceDetail {
                api_resource: pods,
                name: name.into(),
                namespace: Some("kube-system".into()),
                uid: "fixture-0".into(),
                resource_version: "1".into(),
                is_deleting: false,
                finalizers: Vec::new(),
                creation_timestamp: None,
                owners: Vec::new(),
                labels: BTreeMap::from([("k8s-app".into(), "kube-dns".into())]),
                annotations: BTreeMap::new(),
                payload: ResourceDetailPayload::Pod(Box::new(PodDetail {
                    phase: "Running".into(),
                    conditions: vec![PodConditionDetail {
                        type_: "Ready".into(),
                        status: "True".into(),
                        reason: None,
                        message: None,
                    }],
                    node_name: Some("kind-control-plane".into()),
                    pod_ip: Some("10.244.0.3".into()),
                    host_ip: Some("172.18.0.2".into()),
                    qos_class: Some("Burstable".into()),
                    restart_policy: Some("Always".into()),
                    service_account_name: Some("coredns".into()),
                    dns_policy: Some("ClusterFirst".into()),
                    containers: vec![PodContainerDetail {
                        name: "coredns".into(),
                        image: "registry.k8s.io/coredns/coredns:v1.11.1".into(),
                        ready: true,
                        restart_count: 0,
                        state: "Running".into(),
                        reason: None,
                        message: None,
                        command: vec!["/coredns".into()],
                        args: vec![
                            "-conf".into(),
                            "/etc/coredns/Corefile".into(),
                            "--cluster-domain=cluster.local --feature-gates=VeryLongFeature=true --request-timeout=60s".into(),
                        ],
                        ports: vec!["53/UDP".into()],
                        environment_variables: vec![
                            PodEnvironmentVariableDetail {
                                name: "LOG_LEVEL".into(),
                                value: Some("info".into()),
                                source: PodEnvironmentVariableSource::Literal,
                            },
                            PodEnvironmentVariableDetail {
                                name: "KUBERNETES_SERVICE_HOST".into(),
                                value: Some("10.244.0.3".into()),
                                source: PodEnvironmentVariableSource::Field {
                                    path: "status.podIP".into(),
                                },
                            },
                            PodEnvironmentVariableDetail {
                                name: "DNS_LOG_FORMAT".into(),
                                value: Some("json".into()),
                                source: PodEnvironmentVariableSource::ConfigMapKey {
                                    name: "coredns-settings".into(),
                                    key: "log-format".into(),
                                    optional: false,
                                },
                            },
                            PodEnvironmentVariableDetail {
                                name: "API_TOKEN".into(),
                                value: Some("test-token".into()),
                                source: PodEnvironmentVariableSource::SecretKey {
                                    name: "api-credentials".into(),
                                    key: "token".into(),
                                    optional: false,
                                },
                            },
                        ],
                        resource_requests: PodResourceThresholds {
                            cpu_nanocores: Some(10_000_000),
                            memory_bytes: Some(16 * 1024 * 1024),
                        },
                        resource_limits: PodResourceThresholds {
                            cpu_nanocores: Some(100_000_000),
                            memory_bytes: Some(128 * 1024 * 1024),
                        },
                    }],
                    log_containers: Vec::new(),
                    volumes: vec![
                        PodVolumeDetail {
                            name: "config-volume".into(),
                            kind: "ConfigMap".into(),
                            source: "coredns".into(),
                            mount_path: Some("/etc/coredns".into()),
                            read_only: false,
                        },
                        PodVolumeDetail {
                            name: "kube-api-access".into(),
                            kind: "Projected".into(),
                            source: "kube-api-access".into(),
                            mount_path: Some(
                                "/var/run/secrets/kubernetes.io/serviceaccount".into(),
                            ),
                            read_only: true,
                        },
                    ],
                })),
            }),
        }) as WorkerResultBox,
        Box::new(ResourceEventsReplaced {
            cluster_key: 2,
            history_entry_id: 1,
            events: vec![ResourceEvent {
                uid: "event-1".into(),
                type_: "Normal".into(),
                reason: "Started".into(),
                message: "Started container coredns".into(),
                source: Some("kubelet".into()),
                count: 1,
                last_timestamp: None,
            }],
        }) as WorkerResultBox,
    ]);
    let now = OffsetDateTime::now_utc();
    for (seconds_ago, cpu_nanocores, memory_bytes) in [
        (90, 12_000_000, 20 * 1024 * 1024),
        (45, 18_000_000, 24 * 1024 * 1024),
        (0, 15_000_000, 22 * 1024 * 1024),
    ] {
        harness
            .state_mut()
            .worker
            .results
            .push_back(Box::new(ResourceDetailPodUsageUpdated {
                cluster_key: 2,
                history_entry_id: 1,
                usage: PodUsage {
                    timestamp: now - time::Duration::seconds(seconds_ago),
                    cpu_nanocores,
                    memory_bytes,
                    containers: BTreeMap::from([(
                        "coredns".into(),
                        ContainerUsage {
                            cpu_nanocores,
                            memory_bytes,
                        },
                    )]),
                },
            }) as WorkerResultBox);
    }
    harness.run();

    let first_arg_top = harness.get_by_label("-conf").rect().top();
    let second_arg_top = harness.get_by_label("/etc/coredns/Corefile").rect().top();
    let long_arg_top = harness
        .get_by_label(
            "--cluster-domain=cluster.local --feature-gates=VeryLongFeature=true --request-timeout=60s",
        )
        .rect()
        .top();
    assert_eq!(first_arg_top, second_arg_top);
    assert_eq!(second_arg_top, long_arg_top);

    harness.event(egui::Event::PointerGone);
    harness.run();
    // Chart points are positioned against the live clock, so allow their anti-aliased edges to
    // move by a fraction of a pixel between snapshot runs.
    harness.ui_harness(
        HarnessSnapshotOptions::strict(
            "resource_inspectors/pod_resource_detail_inspector_snapshot/pod_resource_detail_inspector",
        )
        .max_failed_pixels(100),
    );
    harness
        .state_mut()
        .worker
        .results
        .push_back(Box::new(PodMetricsApiUnavailable { cluster_key: 2 }) as WorkerResultBox);
    harness.run();
    harness.event(egui::Event::PointerGone);
    harness.run();
    harness.ui_harness(
        HarnessSnapshotOptions::strict(
            "resource_inspectors/pod_resource_detail_inspector_snapshot/pod_resource_detail_inspector_usage_unavailable",
        )
        .max_failed_pixels(100),
    );
    harness.get_by_label("Reveal").scroll_to_me();
    harness.run();
    harness.get_by_label("Reveal").click();
    harness.run();
    harness.get_by_label("test-token");
}

#[test]
fn deployment_editor_completes_match_labels_in_a_selector() {
    let deployment = fixture_api_resource("apps", "Deployment", "deployments");
    let detail = ResourceDetail {
        api_resource: deployment.clone(),
        name: "coredns".into(),
        namespace: Some("kube-system".into()),
        uid: "deployment-match-labels-uid".into(),
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
    open_typed_detail(&mut harness, deployment.clone(), detail);

    harness
        .get_by_label("More actions for coredns")
        .click_accesskit();
    harness.run();
    harness.get_by_label("Edit").click_accesskit();
    harness.run();

    let editor_id = harness
        .state()
        .worker
        .commands
        .iter()
        .find_map(|command| {
            command
                .as_ref()
                .as_any()
                .downcast_ref::<GetResourceYaml>()
                .map(|command| command.editor_id)
        })
        .expect("editing the deployment fetches its YAML");
    let original_yaml = "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: coredns\nspec:\n  selector:\n    matchLabels:\n      app: coredns";
    let partial_yaml = "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: coredns\nspec:\n  selector:\n    match";
    assert!(
        deployment_selector_schema()
            .completion_at(partial_yaml, partial_yaml.chars().count())
            .suggestions
            .iter()
            .any(|suggestion| suggestion.label == "matchLabels"),
        "the deployment schema should complete the partial selector key"
    );
    harness.state_mut().worker.results.extend([
        Box::new(ResourceYamlFetched {
            editor_id,
            cluster_key: 2,
            api_resource: deployment.clone(),
            namespace: Some("kube-system".into()),
            resource_name: "coredns".into(),
            yaml: original_yaml.into(),
        }) as WorkerResultBox,
        Box::new(ResourceSchemaLoaded {
            editor_id,
            cluster_key: 2,
            api_resource: deployment.clone(),
            schema: deployment_selector_schema(),
        }) as WorkerResultBox,
    ]);
    harness.run();

    let mut editor = harness
        .state()
        .ui_state
        .yaml_editors
        .get(&editor_id)
        .expect("deployment editor remains open")
        .clone();
    editor.edited_yaml = partial_yaml.into();
    editor.validation_revision = 0;
    editor.validation_due = None;
    editor.diagnostics.clear();
    editor.retained_diagnostics.clear();
    editor.server_validation = ValidationState::Idle;
    let mut snapshot_harness = Harness::builder().build_ui_state(
        |ctx, state: &mut YamlEditorSnapshotState| {
            super::super::super::super::yaml_editor::show_editor_window(
                ctx,
                &mut state.editor,
                &mut state.commands,
            );
        },
        YamlEditorSnapshotState {
            editor,
            commands: Vec::new(),
        },
    );
    components::test_support::setup_egui(&mut snapshot_harness);
    snapshot_harness.run();

    let text_edit_id = egui::Id::new(("yaml-editor-text", editor_id));
    let mut text_edit_state =
        egui::widgets::text_edit::TextEditState::load(&snapshot_harness.ctx, text_edit_id)
            .expect("the deployment YAML editor has rendered");
    let cursor = partial_yaml.chars().count();
    text_edit_state
        .cursor
        .set_char_range(Some(CCursorRange::one(CCursor::new(cursor))));
    text_edit_state.store(&snapshot_harness.ctx, text_edit_id);
    snapshot_harness
        .ctx
        .memory_mut(|memory| memory.request_focus(text_edit_id));

    snapshot_harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Space);
    snapshot_harness.run();

    let editor = &snapshot_harness.state().editor;
    assert!(editor.suggestions_visible, "editor state: {editor:#?}");
    assert_eq!(editor.suggestions[0].label, "matchLabels");
    snapshot_harness.ui_harness("resource_editor/deployment_editor_completes_match_labels_in_a_selector/deployment_editor_match_labels_completion");
}
