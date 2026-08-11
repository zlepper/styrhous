use super::super::MyEguiApp;
use super::super::state::{
    ClusterConnectionState, ClusterLoadState, ClusterState, ResourceWatchState, UiState,
};
use crate::api_resource::ApiResource;
use crate::minimal_namespace::MinimalNamespace;
use crate::minimal_resource::MinimalResource;
use crate::resource_catalog::{ResourceNavigation, build_resource_navigation};
use crate::resource_table::{CellValue, READY_COLUMN, RESTARTS_COLUMN, STATUS_COLUMN, StatusTone};
use crate::terminal_launcher::TerminalLauncher;
use crate::worker::WorkerTrait;
use egui_kittest::Harness;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

pub(super) fn application_harness<W: WorkerTrait + 'static>() -> Harness<'static, MyEguiApp<W>> {
    let mut harness = Harness::builder().build_eframe(|cc| MyEguiApp::<W>::new(cc));
    components::test_support::setup_egui(&mut harness);
    harness
}

pub(super) fn application_harness_with_terminal<
    W: WorkerTrait + 'static,
    L: TerminalLauncher + 'static,
>() -> Harness<'static, MyEguiApp<W, L>> {
    let mut harness = Harness::builder().build_eframe(|cc| MyEguiApp::<W, L>::new(cc));
    components::test_support::setup_egui(&mut harness);
    harness
}

pub(super) fn fixture_cluster(cluster_key: i32, name: &str) -> ClusterState {
    ClusterState {
        name: name.into(),
        cluster_key,
        namespaces: BTreeMap::new(),
        connection: ClusterConnectionState::Disconnected,
        namespaces_load: ClusterLoadState::Ready,
        api_resources_load: ClusterLoadState::Ready,
        selected_namespaces: HashSet::new(),
        resource_navigation: ResourceNavigation::default(),
        custom_resource_columns: BTreeMap::new(),
        scalable_api_resources: BTreeSet::new(),
        selected_api_resource: None,
        resource_cache: HashMap::new(),
        active_watchers: HashSet::new(),
        resource_searches: HashMap::new(),
        resource_detail_panel: None,
        next_detail_generation: 0,
        pending_delete: None,
        pending_force_delete: None,
        force_delete_error: None,
        pending_deployment_restart: None,
        deployment_restart_error: None,
        pending_scale: None,
        scale_error: None,
    }
}

pub(super) fn fixture_api_resource(group: &str, kind: &str, name: &str) -> ApiResource {
    ApiResource {
        group: group.into(),
        version: "v1".into(),
        kind: kind.into(),
        name: name.into(),
        namespaced: true,
    }
}

pub(super) fn fixture_cluster_scoped_api_resource(
    group: &str,
    kind: &str,
    name: &str,
) -> ApiResource {
    ApiResource {
        namespaced: false,
        ..fixture_api_resource(group, kind, name)
    }
}

fn fixture_resource(index: usize, name: &str) -> MinimalResource {
    MinimalResource {
        uid: format!("fixture-{index}"),
        name: name.into(),
        namespace: Some("kube-system".into()),
        creation_timestamp: Some(time::OffsetDateTime::now_utc() - time::Duration::days(220)),
        cells: BTreeMap::from([
            (READY_COLUMN.to_owned(), CellValue::Text("1/1".into())),
            (
                STATUS_COLUMN.to_owned(),
                CellValue::Status {
                    label: "Running".into(),
                    tone: StatusTone::Success,
                },
            ),
            (RESTARTS_COLUMN.to_owned(), CellValue::Number(0)),
        ]),
        log_containers: Vec::new(),
    }
}

pub(super) fn oracle_resource_table_state() -> UiState {
    let pods = fixture_api_resource("core", "Pod", "pods");
    let core_resources = [
        ("Binding", "bindings"),
        ("ComponentStatus", "componentstatuses"),
        ("ConfigMap", "configmaps"),
        ("Endpoints", "endpoints"),
        ("Event", "events"),
        ("LimitRange", "limitranges"),
        ("Namespace", "namespaces"),
        ("Node", "nodes"),
        ("PersistentVolumeClaim", "persistentvolumeclaims"),
        ("PersistentVolume", "persistentvolumes"),
        ("Pod", "pods"),
        ("PodTemplate", "podtemplates"),
        ("ReplicationController", "replicationcontrollers"),
        ("ResourceQuota", "resourcequotas"),
        ("Secret", "secrets"),
    ]
    .into_iter()
    .map(|(kind, name)| fixture_api_resource("core", kind, name))
    .collect::<Vec<_>>();

    let mut kind = fixture_cluster(2, "kind-kind");
    kind.connection = ClusterConnectionState::Connected;
    kind.namespaces.insert(
        "kube-system".into(),
        MinimalNamespace {
            name: "kube-system".into(),
            display_name: None,
        },
    );
    kind.selected_namespaces.insert("kube-system".into());
    kind.selected_api_resource = Some(pods.clone());
    let mut discovered_resources = core_resources;
    discovered_resources.extend([
        fixture_api_resource("apps", "Deployment", "deployments"),
        fixture_api_resource(
            "autoscaling",
            "HorizontalPodAutoscaler",
            "horizontalpodautoscalers",
        ),
        fixture_api_resource("batch", "Job", "jobs"),
    ]);
    kind.resource_navigation = build_resource_navigation(discovered_resources);
    kind.resource_cache.insert(
        (pods, Some("kube-system".into())),
        ResourceWatchState {
            resources: [
                "coredns-66bc5c9577-ffw2s",
                "coredns-66bc5c9577-z9gt9",
                "etcd-kind-control-plane",
                "kindnet-9qrlh",
                "kube-apiserver-kind-control-plane",
                "kube-controller-manager-kind-control-plane",
                "kube-proxy-v86gd",
                "kube-scheduler-kind-control-plane",
            ]
            .into_iter()
            .enumerate()
            .map(|(index, name)| (format!("fixture-{index}"), fixture_resource(index, name)))
            .collect(),
            is_synced: true,
            error: None,
        },
    );
    UiState {
        clusters: HashMap::from([
            (1, fixture_cluster(1, "dev")),
            (2, kind),
            (3, fixture_cluster(3, "kube-local")),
        ]),
        next_cluster_key: 3,
        selected_cluster: Some(2),
        ..Default::default()
    }
}
