//! Deterministic UI flow and visual-regression scenarios.

use super::super::MyEguiApp;
use super::super::state::ClusterConnectionState;
use super::super::state::{
    BulkDeleteProgress, BulkDeleteTarget, HelmReleaseWatchState, PendingCronJobRun, PendingDelete,
    PendingForceDelete, PodMetricsNamespaceState, ResourceWatchState, UiState, ValidationState,
    YamlEditorWindowState,
};
use super::super::table_preferences::{
    PersistedResourceTablePreferences, ResourceTableKey, TableColumnDefinition,
};
use super::fixtures::{
    application_harness, application_harness_with_state, application_harness_with_terminal,
    fixture_api_resource, fixture_cluster, fixture_cluster_scoped_api_resource,
    oracle_resource_table_state,
};
use super::harness::{MockUiHarnessExt, command_is};
use crate::cluster_connection_manager::{
    AvailableAksCluster, AvailableTailscaleCluster, Cluster, ClusterDiscoveryTools,
};
use crate::helm_release::{HelmRelease, StorageDriver};
use crate::minimal_namespace::MinimalNamespace;
use crate::minimal_resource::{MinimalResource, PodLogContainer};
use crate::pod_metrics::{ContainerUsage, NodeUsage, POD_USAGE_HISTORY_WINDOW, PodUsage};
use crate::resource_catalog::build_resource_navigation;
use crate::resource_detail::{
    ConfigMapDetail, ManagedResource, ManagedResourceAssociation, NodeDetail, PodConditionDetail,
    PodContainerDetail, PodDetail, PodEnvironmentVariableDetail, PodEnvironmentVariableSource,
    PodResourceThresholds, PodVolumeDetail, ResourceDetail, ResourceDetailPayload, ResourceEvent,
    ResourceOwner, SecretDataDetail, SecretDetail,
};
use crate::resource_schema::ResourceSchema;
use crate::resource_table::{
    AVAILABLE_COLUMN, CONTAINERS_COLUMN, CellValue, ContainerIndicator, ContainerKind, NODE_COLUMN,
    READY_COLUMN, RESTARTS_COLUMN, STATUS_COLUMN, StatusTone, UP_TO_DATE_COLUMN,
};
use crate::terminal_launcher::{
    DebugImagePreset, DebugProfile, ShellRequest, TerminalLaunchSettings, TerminalLauncher,
    test_support::MockTerminalLauncher,
};
use crate::updater::UpdateStatus;
use crate::worker::*;
use components::test_support::{HarnessSnapshotOptions, UiHarnessSnapshot};
use egui::text::{CCursor, CCursorRange};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use k8s_openapi::serde_json::json;
use std::collections::{BTreeMap, HashMap, HashSet};
use time::OffsetDateTime;

mod flows;

const OPEN_APPLICATION_SETTINGS: &str = "Open application settings: Configure terminal launching, debug images, and application updates.";
const OPEN_CLUSTER_DISCOVERY: &str =
    "Open cluster discovery: Find and add clusters available through Azure CLI or Tailscale.";
const OPEN_NAMESPACE_SELECTOR_SETTINGS: &str =
    "Open namespace selector settings: Configure namespace identities from labels and annotations.";

struct YamlEditorSnapshotState {
    editor: YamlEditorWindowState,
    commands: Vec<WorkerCommandBox>,
}

fn fixture_helm_release() -> HelmRelease {
    HelmRelease {
        storage: StorageDriver::Secret,
        storage_name: "sh.helm.release.v1.demo.v2".into(),
        name: "demo".into(),
        namespace: "kube-system".into(),
        revision: 2,
        status: "deployed".into(),
        description: "Upgrade complete".into(),
        notes: "Thank you for installing Demo.\n\nYour release is ready.".into(),
        chart: "nginx".into(),
        chart_version: "1.2.3".into(),
        app_version: "1.25.0".into(),
        first_deployed: "2026-08-01T12:00:00Z".into(),
        last_deployed: "2026-08-15T12:00:00Z".into(),
        values: json!({"replicaCount": 2}),
        manifest: "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: demo".into(),
        storage_labels: BTreeMap::new(),
        storage_annotations: BTreeMap::new(),
    }
}

fn fixture_helm_release_revisions() -> Vec<HelmRelease> {
    let mut previous = fixture_helm_release();
    previous.revision = 1;
    previous.status = "superseded".into();
    previous.description = "Initial install".into();
    previous.notes = "The initial release was deployed.".into();
    previous.last_deployed = "2026-08-01T12:00:00Z".into();
    vec![fixture_helm_release(), previous]
}

fn open_terminal_settings(state: &mut UiState, draft: TerminalLaunchSettings) {
    let mut commands = Vec::new();
    state.open_terminal_settings(&draft, &mut commands);
    assert!(
        commands.is_empty(),
        "opening local settings must not start worker work"
    );
}

fn open_inspector_with_column_settings(state: &mut UiState, commands: &mut Vec<WorkerCommandBox>) {
    let deployment = fixture_api_resource("apps", "Deployment", "deployments");
    state.open_resource_detail(
        2,
        deployment.clone(),
        "api".into(),
        Some("kube-system".into()),
        "deployment-uid".into(),
        commands,
    );
    let mut preferences = PersistedResourceTablePreferences::default();
    let mut settings = super::super::resource_table_settings::target(
        &mut preferences,
        ResourceTableKey::workspace(&deployment),
        &[TableColumnDefinition {
            id: "name".into(),
            label: "Name".into(),
            default_width: 160.0,
            sortable: true,
        }],
    );
    settings.set_resource_detail_owner(1);
    let discarded = state.global_blades.push(Box::new(settings));
    assert!(discarded.is_empty());
}

fn select_namespace(harness: &mut Harness<MyEguiApp<MockWorker>>, namespace: &str) {
    harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Namespace")
        .click();
    harness.run_steps(3);
    harness.get_by_label(namespace).click();
    harness.run_steps(2);
}

fn overflowing_pod_detail() -> PodDetail {
    PodDetail {
        phase: "Running".into(),
        conditions: vec![
            PodConditionDetail {
                type_: "Initialized".into(),
                status: "True".into(),
                reason: Some("PodCompleted".into()),
                message: Some("All init containers have completed.".into()),
            },
            PodConditionDetail {
                type_: "Ready".into(),
                status: "True".into(),
                reason: Some("ContainersReady".into()),
                message: Some("All containers are ready.".into()),
            },
        ],
        node_name: Some("kind-control-plane".into()),
        pod_ip: Some("10.244.0.23".into()),
        host_ip: Some("172.18.0.2".into()),
        qos_class: Some("Burstable".into()),
        restart_policy: Some("Always".into()),
        service_account_name: Some("api".into()),
        dns_policy: Some("ClusterFirst".into()),
        containers: (0..3)
            .map(|index| PodContainerDetail {
                name: format!("api-{index}"),
                image: "registry.example.com/api:v1.2.3".into(),
                ready: true,
                restart_count: 0,
                state: "Running".into(),
                reason: None,
                message: None,
                command: vec!["/app/api".into()],
                args: vec!["--serve".into(), "--metrics-address=:9090".into()],
                ports: vec!["8080/TCP".into(), "9090/TCP".into()],
                environment_variables: vec![
                    PodEnvironmentVariableDetail {
                        name: "LOG_LEVEL".into(),
                        value: Some("info".into()),
                        source: PodEnvironmentVariableSource::Literal,
                    },
                    PodEnvironmentVariableDetail {
                        name: "DATABASE_URL".into(),
                        value: Some("postgresql://database/api".into()),
                        source: PodEnvironmentVariableSource::SecretKey {
                            name: "api-database".into(),
                            key: "url".into(),
                            optional: false,
                        },
                    },
                ],
                resource_requests: Default::default(),
                resource_limits: Default::default(),
            })
            .collect(),
        log_containers: Vec::new(),
        volumes: (0..3)
            .map(|index| PodVolumeDetail {
                name: format!("config-{index}"),
                kind: "ConfigMap".into(),
                source: "api-configuration".into(),
                mount_path: Some(format!("/etc/api/config-{index}")),
                read_only: true,
            })
            .collect(),
    }
}

fn secondary_click(harness: &mut Harness<MyEguiApp<MockWorker>>, position: egui::Pos2) {
    harness.event(egui::Event::PointerMoved(position));
    harness.event(egui::Event::PointerButton {
        pos: position,
        button: egui::PointerButton::Secondary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    harness.event(egui::Event::PointerButton {
        pos: position,
        button: egui::PointerButton::Secondary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
}

fn primary_click<L: TerminalLauncher>(
    harness: &mut Harness<MyEguiApp<MockWorker, L>>,
    position: egui::Pos2,
) {
    harness.event(egui::Event::PointerMoved(position));
    harness.event(egui::Event::PointerButton {
        pos: position,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    harness.event(egui::Event::PointerButton {
        pos: position,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
}

fn open_settings(harness: &mut Harness<MyEguiApp<MockWorker>>) {
    let settings_position = harness.get_by_label("Settings").rect().center();
    primary_click(harness, settings_position);
    harness.run();
}

fn open_cluster_discovery(harness: &mut Harness<MyEguiApp<MockWorker>>) {
    let discovery_position = harness.get_by_label(OPEN_CLUSTER_DISCOVERY).rect().center();
    primary_click(harness, discovery_position);
    harness.run_steps(2);
}

fn drag<L: TerminalLauncher>(
    harness: &mut Harness<MyEguiApp<MockWorker, L>>,
    from: egui::Pos2,
    to: egui::Pos2,
) {
    harness.event(egui::Event::PointerMoved(from));
    harness.event(egui::Event::PointerButton {
        pos: from,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    harness.run();
    harness.event(egui::Event::PointerMoved(to));
    harness.run();
    harness.event(egui::Event::PointerButton {
        pos: to,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    harness.run();
}

fn type_text<L: TerminalLauncher>(
    harness: &mut Harness<MyEguiApp<MockWorker, L>>,
    accessibility_label: &str,
    value: &str,
) {
    let position = harness.get_by_label(accessibility_label).rect().center();
    primary_click(harness, position);
    harness.run();
    harness
        .input_mut()
        .events
        .push(egui::Event::Text(value.into()));
    harness.run();
}

fn multi_container_pod_table_state() -> (UiState, String) {
    let pods = fixture_api_resource("core", "Pod", "pods");
    let mut state = oracle_resource_table_state();
    let resource = state
        .clusters
        .get_mut(&2)
        .expect("kind fixture exists")
        .resource_cache
        .get_mut(&(pods, Some("kube-system".into())))
        .expect("pod fixture exists")
        .resources
        .values_mut()
        .next()
        .expect("pod resource exists");
    resource.log_containers = vec![
        PodLogContainer {
            name: "coredns".into(),
            kind: ContainerKind::App,
            image: None,
        },
        PodLogContainer {
            name: "dns-autoscaler".into(),
            kind: ContainerKind::App,
            image: None,
        },
    ];
    let pod_name = resource.name.clone();
    (state, pod_name)
}

fn open_typed_detail(
    harness: &mut Harness<MyEguiApp<MockWorker>>,
    api_resource: crate::api_resource::ApiResource,
    detail: ResourceDetail,
) {
    let mut commands = Vec::new();
    harness.state_mut().ui_state.open_resource_detail(
        2,
        api_resource,
        detail.name.clone(),
        detail.namespace.clone(),
        detail.uid.clone(),
        &mut commands,
    );
    harness.state_mut().worker.commands.extend(commands);
    harness
        .state_mut()
        .worker
        .results
        .push_back(Box::new(ResourceDetailUpdated {
            cluster_key: 2,
            history_entry_id: 1,
            detail: Box::new(detail),
        }) as WorkerResultBox);
    harness.run();
}

fn config_map_detail(data: BTreeMap<String, String>) -> ResourceDetail {
    ResourceDetail {
        api_resource: fixture_api_resource("core", "ConfigMap", "configmaps"),
        name: "settings".into(),
        namespace: Some("kube-system".into()),
        uid: "configmap-uid".into(),
        resource_version: "1".into(),
        is_deleting: false,
        finalizers: Vec::new(),
        creation_timestamp: None,
        owners: Vec::new(),
        labels: BTreeMap::new(),
        annotations: BTreeMap::new(),
        payload: ResourceDetailPayload::ConfigMap(ConfigMapDetail {
            data,
            immutable: false,
        }),
    }
}

fn deployment_selector_schema() -> ResourceSchema {
    ResourceSchema::new(json!({
        "type": "object",
        "properties": {
            "apiVersion": {"type": "string"},
            "kind": {"type": "string"},
            "metadata": {"type": "object"},
            "spec": {
                "type": "object",
                "properties": {
                    "selector": {
                        "description": "Label selector for the Pods managed by this Deployment.",
                        "allOf": [{"$ref": "#/components/schemas/LabelSelector"}]
                    }
                }
            }
        },
        "components": {
            "schemas": {
                "LabelSelector": {
                    "type": "object",
                    "properties": {
                        "matchLabels": {
                            "type": "object",
                            "description": "Map of label keys and values that must match the selected Pods.",
                            "additionalProperties": {"type": "string"}
                        },
                        "matchExpressions": {
                            "type": "array",
                            "description": "Requirements for selecting Pods by label.",
                            "items": {"type": "object"}
                        }
                    }
                }
            }
        }
    }))
}

fn show_apps_resource_table(harness: &mut Harness<MyEguiApp<MockWorker>>) {
    harness.state_mut().ui_state = oracle_resource_table_state();
    harness.run();
    let apps_and_containers = harness.get_by_label("Apps & Containers").rect().center();
    primary_click(harness, apps_and_containers);
    harness.run_steps(2);
}

fn resource_table_name_header_left(harness: &Harness<MyEguiApp<MockWorker>>) -> egui::Pos2 {
    let selection = harness
        .get_by_role_and_label(egui::accesskit::Role::CheckBox, "Select all rows")
        .rect();
    egui::pos2(selection.right() + 16.0, selection.center().y)
}

fn resource_table_name_header_context_position(
    harness: &Harness<MyEguiApp<MockWorker>>,
) -> egui::Pos2 {
    let header = resource_table_name_header_left(harness);
    egui::pos2(header.x + 132.0, header.y)
}

fn resource_table_name_resize_handle(harness: &Harness<MyEguiApp<MockWorker>>) -> egui::Rect {
    harness
        .get_by_role_and_label(egui::accesskit::Role::Button, "Resize Name column")
        .rect()
}

fn resource_table_name_width(harness: &mut Harness<MyEguiApp<MockWorker>>) -> f32 {
    let resource = fixture_api_resource("core", "Pod", "pods");
    let name_column = TableColumnDefinition {
        id: "name".into(),
        label: "Name".into(),
        default_width: 160.0,
        sortable: true,
    };
    harness
        .state_mut()
        .resource_table_preferences
        .resolved_columns(
            &ResourceTableKey::workspace(&resource),
            std::slice::from_ref(&name_column),
        )
        .into_iter()
        .next()
        .expect("the Name column remains visible")
        .width
}

fn open_workspace_column_settings(harness: &mut Harness<MyEguiApp<MockWorker>>) {
    let header = resource_table_name_header_context_position(harness);
    secondary_click(harness, header);
    harness.run_steps(2);
    let configure = harness.get_by_label("Configure columns").rect().center();
    primary_click(harness, configure);
    harness.run_steps(2);
}

mod application_flow;
mod cluster_and_helm;
mod debug_profiles;
mod discovery_and_navigation;
mod global_blades;
mod resource_details;
mod resource_inspectors;
mod resource_tables;
mod selection_and_context;
mod table_configuration;
mod terminal;
