mod cluster_rail;
mod dialogs;
#[doc(hidden)]
pub mod log_viewer_profile;
mod log_windows;
mod resource_actions;
mod resource_detail;
mod resource_navigation;
mod settings;
mod state;
mod widgets;
mod workspace;
mod yaml_editor;

use crate::log_store::LogStoreService;
use crate::terminal_launcher::{
    PodShellRequest, SystemTerminalLauncher, TerminalLaunchSettings, TerminalLauncher,
};
use crate::worker::{Worker, WorkerTrait};
use components::apply_light_theme;
use dialogs::{
    show_delete_confirmation, show_deployment_restart_confirmation, show_deployment_restart_error,
    show_terminal_launch_error,
};
use state::{LogDisplayOptions, PersistedClusterSelections, ResourceNavigationExpansion, UiState};

const CLUSTER_SELECTIONS_STORAGE_KEY: &str = "cluster_selections";
const LOG_DISPLAY_OPTIONS_STORAGE_KEY: &str = "log_display_options";
const RESOURCE_NAVIGATION_EXPANSION_STORAGE_KEY: &str = "resource_navigation_expansion";
const TERMINAL_LAUNCH_SETTINGS_STORAGE_KEY: &str = "terminal_launch_settings";

pub struct MyEguiApp<W: WorkerTrait = Worker, L: TerminalLauncher = SystemTerminalLauncher> {
    worker: W,
    terminal_launcher: L,
    terminal_launch_settings: TerminalLaunchSettings,
    ui_state: UiState,
    log_store: LogStoreService,
}

impl<W: WorkerTrait, L: TerminalLauncher> Default for MyEguiApp<W, L> {
    fn default() -> Self {
        let log_store = LogStoreService::default();
        let mut worker = W::default();
        worker.set_log_store_appender(log_store.appender());
        Self {
            worker,
            terminal_launcher: L::default(),
            terminal_launch_settings: TerminalLaunchSettings::default(),
            ui_state: UiState::default(),
            log_store,
        }
    }
}

impl<W: WorkerTrait, L: TerminalLauncher> MyEguiApp<W, L> {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        apply_light_theme(&cc.egui_ctx);
        let log_store = LogStoreService::with_repaint_context(cc.egui_ctx.clone());
        let mut worker = W::with_repaint_context(cc.egui_ctx.clone());
        worker.set_log_store_appender(log_store.appender());
        let mut app = Self {
            worker,
            terminal_launcher: L::default(),
            terminal_launch_settings: TerminalLaunchSettings::default(),
            ui_state: UiState::default(),
            log_store,
        };
        app.load_persisted_state(cc.storage);
        app
    }

    fn load_persisted_state(&mut self, storage: Option<&dyn eframe::Storage>) {
        self.ui_state.log_display_options = storage
            .and_then(|storage| {
                eframe::get_value::<LogDisplayOptions>(storage, LOG_DISPLAY_OPTIONS_STORAGE_KEY)
            })
            .unwrap_or_default();
        self.terminal_launch_settings = storage
            .and_then(|storage| {
                eframe::get_value::<TerminalLaunchSettings>(
                    storage,
                    TERMINAL_LAUNCH_SETTINGS_STORAGE_KEY,
                )
            })
            .unwrap_or_default();
        self.ui_state.cluster_selections = storage
            .and_then(|storage| {
                eframe::get_value::<PersistedClusterSelections>(
                    storage,
                    CLUSTER_SELECTIONS_STORAGE_KEY,
                )
            })
            .unwrap_or_default();
        self.ui_state.resource_navigation_expansion = storage
            .and_then(|storage| {
                eframe::get_value::<ResourceNavigationExpansion>(
                    storage,
                    RESOURCE_NAVIGATION_EXPANSION_STORAGE_KEY,
                )
            })
            .unwrap_or_default();
    }
}

impl<W: WorkerTrait, L: TerminalLauncher> eframe::App for MyEguiApp<W, L> {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.worker.start();
        let mut commands_to_send = self.ui_state.update(&mut self.worker);
        let mut shell_requests = Vec::<PodShellRequest>::new();
        while let Some(result) = self.log_store.try_next_result() {
            if let crate::log_store::LogStoreResult::Failed { window_id, .. } = &result
                && let Some(window) = self.ui_state.log_windows.get(window_id)
                && !matches!(window.status, state::PodLogStatus::Failed(_))
            {
                commands_to_send.push(crate::worker::WorkerCommand::StopPodLogStream {
                    cluster_key: window.cluster_key,
                    log_window_id: *window_id,
                });
            }
            self.ui_state.apply_log_store_result(result);
        }

        cluster_rail::show(
            ctx,
            &mut self.ui_state,
            &mut commands_to_send,
            &self.terminal_launch_settings,
        );
        let clicked_api_resource = resource_navigation::show(ctx, &mut self.ui_state);
        yaml_editor::show(ctx, &mut self.ui_state, &mut commands_to_send);
        workspace::show(
            ctx,
            &mut self.ui_state,
            &mut commands_to_send,
            &mut shell_requests,
        );
        resource_detail::show(
            ctx,
            &mut self.ui_state,
            &mut commands_to_send,
            &mut shell_requests,
        );
        log_windows::show(
            ctx,
            &mut self.ui_state,
            &self.log_store,
            &mut commands_to_send,
        );
        show_delete_confirmation(ctx, &mut self.ui_state, &mut commands_to_send);
        show_deployment_restart_confirmation(ctx, &mut self.ui_state, &mut commands_to_send);
        settings::show(ctx, &mut self.ui_state, &mut self.terminal_launch_settings);
        show_terminal_launch_error(ctx, &mut self.ui_state, &self.terminal_launch_settings);
        show_deployment_restart_error(ctx, &mut self.ui_state);

        if let (Some(cluster_key), Some(api_resource)) =
            (self.ui_state.selected_cluster, clicked_api_resource)
        {
            self.ui_state
                .select_api_resource(cluster_key, api_resource, &mut commands_to_send);
        }

        for command in commands_to_send {
            self.worker.send_command(command);
        }
        for request in shell_requests {
            if let Err(error) = self
                .terminal_launcher
                .launch(&request, &self.terminal_launch_settings)
            {
                self.ui_state.terminal_launch_error = Some(error);
            }
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(
            storage,
            CLUSTER_SELECTIONS_STORAGE_KEY,
            &self.ui_state.cluster_selections,
        );
        eframe::set_value(
            storage,
            RESOURCE_NAVIGATION_EXPANSION_STORAGE_KEY,
            &self.ui_state.resource_navigation_expansion,
        );
        eframe::set_value(
            storage,
            LOG_DISPLAY_OPTIONS_STORAGE_KEY,
            &self.ui_state.log_display_options,
        );
        eframe::set_value(
            storage,
            TERMINAL_LAUNCH_SETTINGS_STORAGE_KEY,
            &self.terminal_launch_settings,
        );
    }

    fn persist_egui_memory(&self) -> bool {
        // Persist only the app settings explicitly written in `save`. Egui's complete memory
        // includes `Area` z-ordering, which can leave a stale overlay layer above a later blade.
        false
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod persistence_tests {
    use super::*;
    use crate::api_resource::ApiResource;
    use crate::cluster_connection_manager::Cluster;
    use crate::minimal_namespace::MinimalNamespace;
    use crate::worker::{MockWorker, WorkerResult};
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
        };
        let mut storage = MemoryStorage::default();

        eframe::set_value(
            &mut storage,
            TERMINAL_LAUNCH_SETTINGS_STORAGE_KEY,
            &expected,
        );

        assert_eq!(
            eframe::get_value::<TerminalLaunchSettings>(
                &storage,
                TERMINAL_LAUNCH_SETTINGS_STORAGE_KEY
            ),
            Some(expected)
        );
    }

    #[test]
    fn app_does_not_persist_egui_area_memory() {
        let app = MyEguiApp::<MockWorker>::default();

        assert!(!eframe::App::persist_egui_memory(&app));
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
        }
    }

    fn apply_results(
        ui_state: &mut UiState,
        results: impl IntoIterator<Item = WorkerResult>,
    ) -> Vec<crate::worker::WorkerCommand> {
        let mut worker = MockWorker {
            results: VecDeque::from_iter(results),
            ..Default::default()
        };
        ui_state.update(&mut worker)
    }

    fn current_dev_cluster() -> WorkerResult {
        WorkerResult::KubernetesClustersUpdated(vec![Cluster {
            name: "dev".into(),
            is_current: true,
        }])
    }

    fn dev_namespaces() -> WorkerResult {
        WorkerResult::KubernetesNamespacesReplaced {
            cluster_key: 1,
            namespaces: vec![MinimalNamespace {
                name: "default".into(),
                display_name: None,
            }],
        }
    }

    fn dev_api_resources() -> WorkerResult {
        WorkerResult::KubernetesApisLoaded {
            cluster_key: 1,
            api_resources: vec![api_resource("apps", "v1", "Deployment", "deployments")],
        }
    }

    #[test]
    fn cluster_selections_round_trip_through_app_storage_without_shared_state() {
        let expected = saved_selections();
        let mut storage = MemoryStorage::default();
        let mut app = MyEguiApp::<MockWorker>::default();
        app.ui_state.cluster_selections = expected.clone();

        eframe::App::save(&mut app, &mut storage);

        let mut restored = MyEguiApp::<MockWorker>::default();
        restored.load_persisted_state(Some(&storage));

        assert_eq!(restored.ui_state.cluster_selections, expected);
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
            assert!(commands.iter().any(|command| matches!(
                command,
                crate::worker::WorkerCommand::StartResourceWatch {
                    cluster_key: 1,
                    api_resource,
                    namespace,
                } if api_resource.name == "deployments" && namespace.as_deref() == Some("default")
            )));
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
                WorkerResult::KubernetesNamespacesReplaced {
                    cluster_key: 1,
                    namespaces: vec![MinimalNamespace {
                        name: "default".into(),
                        display_name: None,
                    }],
                },
                WorkerResult::KubernetesApisLoaded {
                    cluster_key: 1,
                    api_resources: vec![api_resource("apps", "v1", "Service", "services")],
                },
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
            },
            ..Default::default()
        };
        apply_results(
            &mut ui_state,
            [
                current_dev_cluster(),
                dev_namespaces(),
                WorkerResult::KubernetesApisLoaded {
                    cluster_key: 1,
                    api_resources: vec![api_resource("apps", "v1", "Deployment", "deployments")],
                },
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
}
