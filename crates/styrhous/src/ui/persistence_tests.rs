use super::*;
use crate::api_resource::ApiResource;
use crate::cluster_connection_manager::Cluster;
use crate::minimal_namespace::MinimalNamespace;
use crate::worker::*;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

#[derive(Default)]
struct MemoryStorage(HashMap<String, String>);

impl eframe::Storage for MemoryStorage {
    fn get_string(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }

    fn set_string(&mut self, key: &str, value: String) {
        self.0.insert(key.to_owned(), value);
    }

    fn remove_string(&mut self, key: &str) {
        self.0.remove(key);
    }

    fn flush(&mut self) {}
}

#[test]
fn log_display_options_round_trip_through_eframe_storage() {
    let expected = LogDisplayOptions {
        show_line_numbers: true,
        show_timestamps: true,
        render_ansi: false,
    };
    let mut storage = MemoryStorage::default();

    eframe::set_value(&mut storage, LOG_DISPLAY_OPTIONS_STORAGE_KEY, &expected);

    assert_eq!(
        eframe::get_value::<LogDisplayOptions>(&storage, LOG_DISPLAY_OPTIONS_STORAGE_KEY),
        Some(expected)
    );
}

#[test]
fn terminal_launch_settings_round_trip_through_eframe_storage() {
    let expected = TerminalLaunchSettings {
        custom_template: Some("alacritty -e {command}".into()),
        debug_image_presets: vec![crate::terminal_launcher::DebugImagePreset {
            name: "Operations".into(),
            image: "registry.example/debug-tools:v1".into(),
            profile: crate::terminal_launcher::DebugProfile::Sysadmin,
        }],
    };
    let mut storage = MemoryStorage::default();

    eframe::set_value(
        &mut storage,
        TERMINAL_LAUNCH_SETTINGS_STORAGE_KEY,
        &expected,
    );

    assert_eq!(
        eframe::get_value::<TerminalLaunchSettings>(&storage, TERMINAL_LAUNCH_SETTINGS_STORAGE_KEY),
        Some(expected)
    );
}

#[test]
fn resource_table_preferences_round_trip_through_app_storage() {
    let resource = ApiResource {
        group: "core".into(),
        version: "v1".into(),
        kind: "Pod".into(),
        name: "pods".into(),
        namespaced: true,
    };
    let key = table_preferences::ResourceTableKey::workspace(&resource);
    let columns = vec![table_preferences::TableColumnDefinition {
        id: "name".into(),
        label: "Name".into(),
        default_width: 160.0,
        sortable: true,
    }];
    let mut storage = MemoryStorage::default();
    let mut app = MyEguiApp::<MockWorker>::default();
    app.resource_table_preferences
        .set_width(&key, &columns, "name", 260.0);
    assert!(app.resource_table_preferences.add_custom_column(
        &key,
        table_preferences::CustomMetadataColumn {
            source: table_preferences::MetadataColumnSource::Label,
            key: "app.kubernetes.io/name".into(),
            label: "Application".into(),
        },
    ));

    eframe::App::save(&mut app, &mut storage);

    let mut restored = MyEguiApp::<MockWorker>::default();
    restored.load_persisted_state(Some(&storage));
    assert_eq!(
        restored
            .resource_table_preferences
            .resolved_columns(&key, &columns)[0]
            .width,
        260.0
    );
    assert_eq!(
        restored.resource_table_preferences.custom_columns(&key)[0].label,
        "Application"
    );
}

#[test]
fn app_does_not_persist_egui_area_memory() {
    let app = MyEguiApp::<MockWorker>::default();

    assert!(!eframe::App::persist_egui_memory(&app));
}

#[test]
fn egui_context_configuration_uses_faster_mouse_wheel_scrolling() {
    let ctx = egui::Context::default();
    configure_egui_context(&ctx);

    assert_eq!(
        ctx.options(|options| options.input_options.line_scroll_speed),
        scroll::LINE_SCROLL_SPEED
    );
}

fn api_resource(group: &str, version: &str, kind: &str, name: &str) -> ApiResource {
    ApiResource {
        group: group.into(),
        version: version.into(),
        kind: kind.into(),
        name: name.into(),
        namespaced: true,
    }
}

fn saved_selections() -> PersistedClusterSelections {
    PersistedClusterSelections {
        selections: BTreeMap::from([(
            "dev".into(),
            state::PersistedClusterSelection {
                selected_namespaces: BTreeSet::from(["default".into(), "obsolete".into()]),
                selected_api_resource: Some(state::PersistedApiResource {
                    group: "apps".into(),
                    name: "deployments".into(),
                }),
            },
        )]),
        last_selected_context: None,
    }
}

fn apply_results(
    ui_state: &mut UiState,
    results: impl IntoIterator<Item = WorkerResultBox>,
) -> Vec<WorkerCommandBox> {
    let mut worker = MockWorker {
        results: VecDeque::from_iter(results),
        ..Default::default()
    };
    ui_state.update(&mut worker)
}

fn current_dev_cluster() -> WorkerResultBox {
    Box::new(KubernetesClustersUpdated(vec![Cluster {
        name: "dev".into(),
        is_current: true,
    }]))
}

fn dev_namespaces() -> WorkerResultBox {
    Box::new(KubernetesNamespacesReplaced {
        cluster_key: 1,
        namespaces: vec![MinimalNamespace {
            name: "default".into(),
            display_name: None,
        }],
    })
}

fn dev_api_resources() -> WorkerResultBox {
    Box::new(KubernetesApisLoaded {
        cluster_key: 1,
        api_resources: vec![api_resource("apps", "v1", "Deployment", "deployments")],
        scalable_api_resources: Default::default(),
        pod_metrics_api_available: false,
        node_metrics_api_available: false,
    })
}

#[test]
fn cluster_selections_round_trip_through_app_storage_without_shared_state() {
    let expected = PersistedClusterSelections {
        last_selected_context: Some("prod".into()),
        ..saved_selections()
    };
    let mut storage = MemoryStorage::default();
    let mut app = MyEguiApp::<MockWorker>::default();
    app.ui_state.cluster_selections = expected.clone();

    eframe::App::save(&mut app, &mut storage);

    let mut restored = MyEguiApp::<MockWorker>::default();
    restored.load_persisted_state(Some(&storage));

    assert_eq!(restored.ui_state.cluster_selections, expected);
}

#[test]
fn legacy_cluster_selections_without_a_selected_context_remain_loadable() {
    #[derive(serde::Serialize)]
    struct LegacyPersistedClusterSelections {
        selections: BTreeMap<String, state::PersistedClusterSelection>,
    }

    let expected = saved_selections();
    let mut storage = MemoryStorage::default();
    eframe::set_value(
        &mut storage,
        CLUSTER_SELECTIONS_STORAGE_KEY,
        &LegacyPersistedClusterSelections {
            selections: expected.selections.clone(),
        },
    );

    let mut app = MyEguiApp::<MockWorker>::default();
    app.load_persisted_state(Some(&storage));

    assert_eq!(app.ui_state.cluster_selections, expected);
}

#[test]
fn resource_navigation_expansion_round_trips_through_app_storage() {
    let expected = ResourceNavigationExpansion {
        expanded_nodes: BTreeSet::from([
            "section:Apps & Containers".into(),
            "other-resources".into(),
            "other-resource-group:apps".into(),
        ]),
    };
    let mut storage = MemoryStorage::default();
    let mut app = MyEguiApp::<MockWorker>::default();
    app.ui_state.resource_navigation_expansion = expected.clone();

    eframe::App::save(&mut app, &mut storage);

    let mut restored = MyEguiApp::<MockWorker>::default();
    restored.load_persisted_state(Some(&storage));

    assert_eq!(restored.ui_state.resource_navigation_expansion, expected);
}

#[test]
fn saved_selection_restores_when_discovery_results_arrive_in_either_order() {
    for results in [
        vec![current_dev_cluster(), dev_namespaces(), dev_api_resources()],
        vec![current_dev_cluster(), dev_api_resources(), dev_namespaces()],
    ] {
        let mut ui_state = UiState {
            cluster_selections: saved_selections(),
            ..Default::default()
        };
        let commands = apply_results(&mut ui_state, results);

        let cluster = &ui_state.clusters[&1];
        assert_eq!(
            cluster.selected_namespaces,
            HashSet::from(["default".into()])
        );
        assert_eq!(
            cluster
                .selected_api_resource
                .as_ref()
                .map(|resource| resource.name.as_str()),
            Some("deployments")
        );
        assert!(commands.iter().any(|command| {
            command
                .as_ref()
                .as_any()
                .downcast_ref::<ReconcileResourceWatches>()
                .is_some_and(|command| {
                    command.cluster_key == 1
                        && command.api_resource.name == "deployments"
                        && matches!(
                            command.sources.as_slice(),
                            [ResourceWatchSource::AllNamespaces(namespaces)]
                                if namespaces == &BTreeSet::from(["default".to_owned()])
                        )
                })
        }));
    }
}

#[test]
fn unavailable_saved_selection_is_discarded_without_affecting_other_contexts() {
    let mut selections = saved_selections();
    selections
        .selections
        .get_mut("dev")
        .unwrap()
        .selected_namespaces = BTreeSet::from(["obsolete".into()]);
    selections.selections.insert(
        "prod".into(),
        state::PersistedClusterSelection {
            selected_namespaces: BTreeSet::from(["production".into()]),
            selected_api_resource: Some(state::PersistedApiResource {
                group: "core".into(),
                name: "pods".into(),
            }),
        },
    );
    let mut ui_state = UiState {
        cluster_selections: selections,
        ..Default::default()
    };

    apply_results(
        &mut ui_state,
        [
            current_dev_cluster(),
            Box::new(KubernetesNamespacesReplaced {
                cluster_key: 1,
                namespaces: vec![MinimalNamespace {
                    name: "default".into(),
                    display_name: None,
                }],
            }),
            Box::new(KubernetesApisLoaded {
                cluster_key: 1,
                api_resources: vec![api_resource("apps", "v1", "Service", "services")],
                scalable_api_resources: Default::default(),
                pod_metrics_api_available: false,
                node_metrics_api_available: false,
            }),
        ],
    );

    assert!(ui_state.clusters[&1].selected_namespaces.is_empty());
    assert!(ui_state.clusters[&1].selected_api_resource.is_none());
    assert!(!ui_state.cluster_selections.selections.contains_key("dev"));
    assert!(ui_state.cluster_selections.selections.contains_key("prod"));
}

#[test]
fn selection_changes_update_only_their_contexts_persisted_entry() {
    let mut ui_state = UiState {
        cluster_selections: PersistedClusterSelections {
            selections: BTreeMap::from([(
                "prod".into(),
                state::PersistedClusterSelection {
                    selected_namespaces: BTreeSet::from(["production".into()]),
                    selected_api_resource: Some(state::PersistedApiResource {
                        group: "core".into(),
                        name: "pods".into(),
                    }),
                },
            )]),
            last_selected_context: None,
        },
        ..Default::default()
    };
    apply_results(
        &mut ui_state,
        [
            current_dev_cluster(),
            dev_namespaces(),
            Box::new(KubernetesApisLoaded {
                cluster_key: 1,
                api_resources: vec![api_resource("apps", "v1", "Deployment", "deployments")],
                scalable_api_resources: Default::default(),
                pod_metrics_api_available: false,
                node_metrics_api_available: false,
            }),
        ],
    );

    let deployment = api_resource("apps", "v1", "Deployment", "deployments");
    let mut commands = Vec::new();
    ui_state.select_api_resource(1, deployment, &mut commands);
    ui_state.replace_selected_namespaces(1, ["default".into()], &mut commands);

    assert_eq!(
        ui_state.cluster_selections.selections["dev"].selected_namespaces,
        BTreeSet::from(["default".into()])
    );
    assert_eq!(
        ui_state.cluster_selections.selections["dev"]
            .selected_api_resource
            .as_ref()
            .map(|resource| (resource.group.as_str(), resource.name.as_str())),
        Some(("apps", "deployments"))
    );
    assert_eq!(
        ui_state.cluster_selections.selections["prod"].selected_namespaces,
        BTreeSet::from(["production".into()])
    );
}
