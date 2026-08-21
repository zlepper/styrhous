//! Terminal actions and terminal-settings scenarios.

use super::*;

#[test]
fn namespace_popup_option_does_not_activate_the_overlapped_resource_button() {
    let mut state = oracle_resource_table_state();
    let cluster = state.clusters.get_mut(&2).expect("kind fixture exists");
    for namespace in ["default", "monitoring"] {
        cluster.namespaces.insert(
            namespace.into(),
            MinimalNamespace {
                name: namespace.into(),
                display_name: None,
            },
        );
    }

    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = state;
    harness.run();
    harness.get_by_label("Apps & Containers").click_accesskit();
    harness.run();
    harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Namespace")
        .click();
    harness.run();

    let namespace_option = harness.get_by_label("monitoring");
    let overlapped_resource = harness
        .get_by_label("Open details for coredns-66bc5c9577-z9gt9")
        .rect();
    assert!(
        namespace_option.rect().intersects(overlapped_resource),
        "the namespace option must overlap a resource button to exercise popup input ownership"
    );

    namespace_option.click();
    harness.run_steps(2);

    assert_eq!(
        harness.state().ui_state.clusters[&2].selected_namespaces,
        HashSet::from(["monitoring".to_owned()])
    );
    assert!(
        harness.state().ui_state.clusters[&2]
            .resource_detail_panel
            .is_none(),
        "the namespace click must not open the overlapped resource"
    );
    assert!(
        !harness
            .state()
            .worker
            .commands
            .iter()
            .any(|command| command_is::<StartResourceDetailWatch>(command).is_some()),
        "the namespace click must not start an underlying detail watch"
    );
}

#[test]
fn namespace_popup_filters_to_an_offscreen_option_before_pointer_click() {
    let generated_namespaces = (0..12)
        .map(|index| MinimalNamespace {
            name: format!("k8s-styrhous-fixture-{index:02}"),
            display_name: None,
        })
        .collect::<Vec<_>>();
    let target_namespace = "kube-system";
    let mut namespaces = vec![MinimalNamespace {
        name: "default".into(),
        display_name: None,
    }];
    namespaces.extend(generated_namespaces);
    namespaces.extend([
        MinimalNamespace {
            name: "kube-node-lease".into(),
            display_name: None,
        },
        MinimalNamespace {
            name: "kube-public".into(),
            display_name: None,
        },
    ]);
    namespaces.push(MinimalNamespace {
        name: target_namespace.into(),
        display_name: None,
    });
    let setup_state = || {
        let mut state = oracle_resource_table_state();
        let cluster = state.clusters.get_mut(&2).expect("kind fixture exists");
        cluster.selected_namespaces.clear();
        cluster.selected_api_resource = None;
        cluster.namespaces = namespaces
            .iter()
            .cloned()
            .map(|namespace| (namespace.name.clone().into(), namespace))
            .collect();
        state
    };

    let mut offscreen_harness = application_harness::<MockWorker>();
    offscreen_harness.state_mut().ui_state = setup_state();
    offscreen_harness.run();
    offscreen_harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Namespace")
        .click();
    offscreen_harness.run();
    offscreen_harness.get_by_label(target_namespace).click();
    offscreen_harness.run_steps(1);
    assert!(
        offscreen_harness.state().ui_state.clusters[&2]
            .selected_namespaces
            .is_empty(),
        "the unfiltered target is off-screen, so its accessibility node must not be used for a pointer click"
    );

    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = setup_state();
    harness.run();
    harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Namespace")
        .click();
    harness.run();
    let search_input = harness
        .query_by_role_and_label(egui::accesskit::Role::TextInput, "Search Namespace")
        .expect("the namespace popup search input should be present");
    assert!(
        search_input.is_focused(),
        "the popup search field must receive focus before typing"
    );
    search_input.type_text(target_namespace);
    harness.run_steps(1);
    assert!(
        harness
            .query_by_role_and_label(egui::accesskit::Role::TextInput, "Search Namespace")
            .is_some_and(|input| input.value().as_deref() == Some(target_namespace)),
        "the text input must contain the namespace filter before the test clicks its option"
    );

    harness.state_mut().worker.results.extend((0..32).map(|_| {
        Box::new(KubernetesNamespacesReplaced {
            cluster_key: 2,
            namespaces: namespaces.clone(),
        }) as WorkerResultBox
    }));
    harness.get_by_label(target_namespace).click();
    harness.run_steps(1);

    assert_eq!(
        harness.state().ui_state.clusters[&2].selected_namespaces,
        HashSet::from([target_namespace.to_owned()]),
        "the filtered namespace option must receive a pointer click even while the worker delivers a burst of discovery results"
    );
}

#[test]
fn shell_action_launches_the_selected_context_pod_and_application_container() {
    let mut harness = application_harness_with_terminal::<MockWorker, MockTerminalLauncher>();
    let mut state = oracle_resource_table_state();
    let pods = fixture_api_resource("core", "Pod", "pods");
    let resource = state
        .clusters
        .get_mut(&2)
        .unwrap()
        .resource_cache
        .get_mut(&(pods, Some("kube-system".into())))
        .unwrap()
        .resources
        .values_mut()
        .next()
        .unwrap();
    resource.log_containers = vec![PodLogContainer {
        name: "coredns".into(),
        kind: ContainerKind::App,
        image: None,
    }];
    let pod_name = resource.name.clone();
    harness.state_mut().ui_state = state;
    harness.run();

    let action_label = format!("More actions for {pod_name}");
    harness.get_by_label(&action_label).click_accesskit();
    harness.run();
    harness.get_by_label("Shell").click_accesskit();
    harness.run();

    assert_eq!(
        harness.state().terminal_launcher.requests.as_slice(),
        &[ShellRequest::Pod {
            kube_context: "kind-kind".into(),
            namespace: "kube-system".into(),
            pod_name,
            container: "coredns".into(),
        }]
    );
}

#[test]
fn pod_debug_shell_action_launches_configured_and_pod_images_for_the_selected_target() {
    let mut harness = application_harness_with_terminal::<MockWorker, MockTerminalLauncher>();
    let mut state = oracle_resource_table_state();
    let pods = fixture_api_resource("core", "Pod", "pods");
    let resource = state
        .clusters
        .get_mut(&2)
        .unwrap()
        .resource_cache
        .get_mut(&(pods, Some("kube-system".into())))
        .unwrap()
        .resources
        .values_mut()
        .next()
        .unwrap();
    resource.log_containers = vec![
        PodLogContainer {
            name: "setup".into(),
            kind: ContainerKind::Init,
            image: Some("registry.example/setup:v1".into()),
        },
        PodLogContainer {
            name: "coredns".into(),
            kind: ContainerKind::App,
            image: Some("registry.example/coredns:v1".into()),
        },
        PodLogContainer {
            name: "sidecar".into(),
            kind: ContainerKind::App,
            image: Some("registry.example/sidecar:v1".into()),
        },
        PodLogContainer {
            name: "debugger".into(),
            kind: ContainerKind::Ephemeral,
            image: Some("registry.example/debugger:v1".into()),
        },
    ];
    let pod_name = resource.name.clone();
    harness.state_mut().ui_state = state;
    harness.run();

    let more_actions_label = format!("More actions for {pod_name}");
    let more_actions_position = harness.get_by_label(&more_actions_label).rect().center();
    primary_click(&mut harness, more_actions_position);
    harness.run();
    let debug_shell_position = harness.get_by_label("Debug shell ⏵").rect().center();
    primary_click(&mut harness, debug_shell_position);
    harness.run();
    let target_position = harness.get_by_label("coredns ⏵").rect().center();
    primary_click(&mut harness, target_position);
    harness.run();
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "terminal/pod_debug_shell_action_launches_configured_and_pod_images_for_the_selected_target/debug_images",
    ));
    let busybox_position = harness.get_by_label("Busybox — General").rect().center();
    primary_click(&mut harness, busybox_position);
    harness.run();

    primary_click(&mut harness, more_actions_position);
    harness.run();
    let debug_shell_position = harness.get_by_label("Debug shell ⏵").rect().center();
    primary_click(&mut harness, debug_shell_position);
    harness.run();
    let target_position = harness.get_by_label("coredns ⏵").rect().center();
    primary_click(&mut harness, target_position);
    harness.run();
    let pod_image_position = harness
        .get_by_label("registry.example/coredns:v1 — General")
        .rect()
        .center();
    primary_click(&mut harness, pod_image_position);
    harness.run();

    primary_click(&mut harness, more_actions_position);
    harness.run();
    let debug_shell_position = harness.get_by_label("Debug shell ⏵").rect().center();
    primary_click(&mut harness, debug_shell_position);
    harness.run();
    let target_position = harness.get_by_label("sidecar ⏵").rect().center();
    primary_click(&mut harness, target_position);
    harness.run();
    let busybox_position = harness.get_by_label("Busybox — General").rect().center();
    primary_click(&mut harness, busybox_position);
    harness.run();

    assert_eq!(
        harness.state().terminal_launcher.requests.as_slice(),
        [
            ShellRequest::PodDebug {
                kube_context: "kind-kind".into(),
                namespace: "kube-system".into(),
                pod_name: pod_name.clone(),
                target_container: "coredns".into(),
                preset: DebugImagePreset {
                    name: "Busybox".into(),
                    image: "busybox".into(),
                    profile: DebugProfile::General,
                },
            },
            ShellRequest::PodDebug {
                kube_context: "kind-kind".into(),
                namespace: "kube-system".into(),
                pod_name: pod_name.clone(),
                target_container: "coredns".into(),
                preset: DebugImagePreset {
                    name: "registry.example/coredns:v1".into(),
                    image: "registry.example/coredns:v1".into(),
                    profile: DebugProfile::General,
                },
            },
            ShellRequest::PodDebug {
                kube_context: "kind-kind".into(),
                namespace: "kube-system".into(),
                pod_name,
                target_container: "sidecar".into(),
                preset: DebugImagePreset {
                    name: "Busybox".into(),
                    image: "busybox".into(),
                    profile: DebugProfile::General,
                },
            },
        ]
    );
}

#[test]
fn node_shell_action_launches_the_selected_context_node_and_preset() {
    let mut harness = application_harness_with_terminal::<MockWorker, MockTerminalLauncher>();
    let nodes = fixture_cluster_scoped_api_resource("core", "Node", "nodes");
    let mut state = oracle_resource_table_state();
    let cluster = state.clusters.get_mut(&2).unwrap();
    cluster.selected_api_resource = Some(nodes.clone());
    cluster.resource_cache.insert(
        (nodes, None),
        ResourceWatchState {
            resources: BTreeMap::from([(
                "node-uid".into(),
                MinimalResource {
                    uid: "node-uid".into(),
                    name: "kind-control-plane".into(),
                    namespace: None,
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
        },
    );
    harness.state_mut().ui_state = state;
    harness.run();

    let more_actions_position = harness
        .get_by_label("More actions for kind-control-plane")
        .rect()
        .center();
    primary_click(&mut harness, more_actions_position);
    harness.run();
    let shell_position = harness.get_by_label("Shell ⏵").rect().center();
    primary_click(&mut harness, shell_position);
    harness.run();
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "terminal/node_shell_action_launches_the_selected_context_node_and_preset/node_shell_presets",
    ));
    let busybox_position = harness.get_by_label("Busybox — General").rect().center();
    primary_click(&mut harness, busybox_position);
    harness.run();

    assert_eq!(
        harness.state().terminal_launcher.requests.as_slice(),
        &[ShellRequest::Node {
            kube_context: "kind-kind".into(),
            node_name: "kind-control-plane".into(),
            preset: DebugImagePreset {
                name: "Busybox".into(),
                image: "busybox".into(),
                profile: DebugProfile::General,
            },
        }]
    );
}

#[test]
fn shell_launch_failure_uses_the_styled_error_modal_and_opens_terminal_settings() {
    let mut harness = application_harness_with_terminal::<MockWorker, MockTerminalLauncher>();
    let mut state = oracle_resource_table_state();
    let pods = fixture_api_resource("core", "Pod", "pods");
    let resource = state
        .clusters
        .get_mut(&2)
        .unwrap()
        .resource_cache
        .get_mut(&(pods, Some("kube-system".into())))
        .unwrap()
        .resources
        .values_mut()
        .next()
        .unwrap();
    resource.log_containers = vec![PodLogContainer {
        name: "coredns".into(),
        kind: ContainerKind::App,
        image: None,
    }];
    let pod_name = resource.name.clone();
    harness.state_mut().ui_state = state;
    harness.state_mut().terminal_launcher.failure = Some(
        "No supported terminal launcher was found. Tried: xdg-terminal-exec (No such file or directory (os error 2))."
            .into(),
    );
    harness.run();

    let action_label = format!("More actions for {pod_name}");
    harness.get_by_label(&action_label).click_accesskit();
    harness.run();
    harness.get_by_label("Shell").click_accesskit();
    harness.run_steps(2);

    harness.get_by_label("SHELL");
    harness.get_by_label("Couldn’t open a terminal");
    harness.get_by_label(
        "No supported terminal launcher was found. Tried: xdg-terminal-exec (No such file or directory (os error 2)).",
    );
    harness.get_by_label("Open settings");
    harness.get_by_label("Dismiss");
    assert!(harness.state().ui_state.terminal_launch_error.is_some());
    harness.run_steps(2);
    harness.ui_harness(HarnessSnapshotOptions::one_pixel("terminal/shell_launch_failure_uses_the_styled_error_modal_and_opens_terminal_settings/terminal_launch_error"));

    harness.get_by_label("Open settings").click_accesskit();
    harness.run_steps(2);

    assert!(harness.state().ui_state.terminal_launch_error.is_none());
    assert!(harness.state().ui_state.global_blades.navigator().is_some());
    harness.get_by_label("Terminal launcher");
}

#[test]
fn terminal_launch_error_dismisses_without_opening_settings() {
    let mut harness = application_harness::<MockWorker>();
    let mut state = oracle_resource_table_state();
    state.terminal_launch_error = Some("Unable to start xterm: permission denied".into());
    harness.state_mut().ui_state = state;
    harness.run();

    harness.get_by_label("Dismiss").click_accesskit();
    harness.run();

    assert!(harness.state().ui_state.terminal_launch_error.is_none());
    assert!(harness.state().ui_state.global_blades.navigator().is_none());
}

#[test]
fn settings_button_opens_the_terminal_launcher_blade() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    harness.run();

    let settings_position = harness.get_by_label("Settings").rect().center();
    primary_click(&mut harness, settings_position);
    harness.run();
    let application_settings_position = harness
        .get_by_label(OPEN_APPLICATION_SETTINGS)
        .rect()
        .center();
    primary_click(&mut harness, application_settings_position);
    harness.run();

    harness.get_by_label("Terminal launcher");
    harness.get_by_label("Save changes");
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "terminal/settings_button_opens_the_terminal_launcher_blade/settings_terminal_launcher",
    ));
}
