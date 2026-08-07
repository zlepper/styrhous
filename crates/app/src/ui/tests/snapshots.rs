use super::super::dialogs::show_delete_confirmation;
use super::super::state::ClusterConnectionState;
use super::super::state::{PendingDelete, UiState};
use super::fixtures::{
    application_harness, fixture_api_resource, fixture_cluster, oracle_resource_table_state,
};
use crate::cluster_connection_manager::Cluster;
use crate::minimal_namespace::MinimalNamespace;
use crate::resource_catalog::build_resource_navigation;
use crate::worker::{MockWorker, WorkerCommand, WorkerResult};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[test]
fn oracle_resource_table_snapshot_uses_injected_cluster_state() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    harness.run();
    harness.get_by_label("Apps & Containers").click_accesskit();
    harness.run();
    harness.snapshot("oracle_resource_table_injected");
}

#[test]
fn resource_table_more_actions_snapshot() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    harness.run();
    harness.get_by_label("Apps & Containers").click_accesskit();
    harness.run();
    harness
        .get_by_label("More actions for coredns-66bc5c9577-ffw2s")
        .click_accesskit();
    harness.run();
    harness.snapshot("oracle_resource_table_actions");
}

#[test]
fn resource_table_row_context_menu_snapshot() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    harness.run();
    harness.get_by_label("Apps & Containers").click_accesskit();
    harness.run();

    let resource_name_rect = harness.get_by_label("coredns-66bc5c9577-ffw2s").rect();
    let click_position = egui::pos2(
        resource_name_rect.right() + 32.0,
        resource_name_rect.center().y,
    );
    harness.event(egui::Event::PointerMoved(click_position));
    harness.event(egui::Event::PointerButton {
        pos: click_position,
        button: egui::PointerButton::Secondary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    harness.event(egui::Event::PointerButton {
        pos: click_position,
        button: egui::PointerButton::Secondary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    harness.run();

    harness.get_by_label("Edit YAML");
    harness.snapshot("oracle_resource_table_row_context_actions");
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
    harness.snapshot("resource_table_narrow");

    harness.set_size(super::fixtures::APP_SNAPSHOT_SIZE);
    harness.run();
    harness.snapshot("resource_table_resized");
}

#[test]
fn cluster_rail_shows_connection_status_marker_and_tooltip() {
    let mut harness = application_harness::<MockWorker>();
    let mut state = oracle_resource_table_state();
    state
        .clusters
        .get_mut(&1)
        .expect("dev fixture exists")
        .connection = ClusterConnectionState::Connecting;
    harness.state_mut().ui_state = state;
    harness.run();

    harness.get_by_label("dev").hover();
    harness.run();
    harness.snapshot("cluster_rail_connection_status");
}

#[test]
fn delete_confirmation_can_be_cancelled_without_sending_a_command() {
    let mut cluster = fixture_cluster(1, "dev");
    cluster.selected_api_resource = Some(fixture_api_resource("", "ConfigMap", "configmaps"));
    cluster.pending_delete = Some(PendingDelete {
        resource_name: "important-config".into(),
        namespace: "default".into(),
    });
    let state = Rc::new(RefCell::new(UiState {
        clusters: HashMap::from([(1, cluster)]),
        next_cluster_key: 1,
        selected_cluster: Some(1),
    }));
    let commands = Rc::new(RefCell::new(Vec::new()));
    let state_for_ui = state.clone();
    let commands_for_ui = commands.clone();
    let mut harness = Harness::new_ui(move |ui| {
        show_delete_confirmation(
            ui.ctx(),
            &mut state_for_ui.borrow_mut(),
            &mut commands_for_ui.borrow_mut(),
        );
    });

    harness.run();
    harness.get_by_label("Cancel").click_accesskit();
    harness.run();

    assert!(commands.borrow().is_empty());
    assert!(state.borrow().clusters[&1].pending_delete.is_none());
}

#[test]
fn resource_navigation_selects_curated_and_other_resources() {
    let mut cluster = fixture_cluster(1, "dev");
    cluster.selected_namespaces.insert("default".into());
    cluster.resource_navigation = build_resource_navigation(vec![
        fixture_api_resource("core", "Pod", "pods"),
        fixture_api_resource("apps", "Deployment", "deployments"),
        fixture_api_resource("apps", "ControllerRevision", "controllerrevisions"),
    ]);
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = UiState {
        clusters: HashMap::from([(1, cluster)]),
        next_cluster_key: 1,
        selected_cluster: Some(1),
    };
    harness.run();

    harness.get_by_label("Apps & Containers").click_accesskit();
    harness.run();
    harness.get_by_label("pods").click_accesskit();
    harness.run();
    assert_eq!(
        harness.state().ui_state.clusters[&1]
            .selected_api_resource
            .as_ref()
            .map(|resource| resource.name.as_str()),
        Some("pods")
    );

    harness.get_by_label("apps").click_accesskit();
    harness.run();
    harness.get_by_label("Other").click_accesskit();
    harness.run();
    harness
        .get_by_label("controllerrevisions")
        .click_accesskit();
    harness.run();

    let app = harness.state();
    assert_eq!(
        app.ui_state.clusters[&1]
            .selected_api_resource
            .as_ref()
            .map(|resource| resource.name.as_str()),
        Some("controllerrevisions")
    );
    assert_eq!(
        app.worker
            .commands
            .iter()
            .filter_map(|command| match command {
                WorkerCommand::StartResourceWatch { api_resource, .. } =>
                    Some(api_resource.name.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>(),
        vec!["pods", "controllerrevisions"]
    );
}

#[test]
fn test_ui_flow() {
    let mut harness = application_harness::<MockWorker>();
    harness.run();
    harness.snapshot("01_empty_state");

    harness
        .state_mut()
        .worker
        .results
        .push_back(WorkerResult::KubernetesClustersUpdated(vec![
            Cluster {
                name: "dev".into(),
                cluster: None,
            },
            Cluster {
                name: "prod".into(),
                cluster: Some("production".into()),
            },
        ]));
    harness.run();
    harness.snapshot("02_clusters_loaded");

    harness.state_mut().select_cluster(1);
    harness.run();
    harness.snapshot("03_cluster_selected_empty");

    harness
        .state_mut()
        .worker
        .results
        .push_back(WorkerResult::KubernetesNamespacesReplaced {
            cluster_key: 1,
            namespaces: vec![
                MinimalNamespace {
                    name: "default".into(),
                    display_name: None,
                },
                MinimalNamespace {
                    name: "kube-system".into(),
                    display_name: None,
                },
                MinimalNamespace {
                    name: "monitoring".into(),
                    display_name: Some("Monitoring Stack".into()),
                },
            ],
        });
    harness.run();
    harness.snapshot("04_namespaces_loaded");

    harness
        .state_mut()
        .worker
        .results
        .push_back(WorkerResult::KubernetesApisLoaded {
            cluster_key: 1,
            api_resources: vec![
                fixture_api_resource("", "Pod", "pods"),
                fixture_api_resource("", "Service", "services"),
                fixture_api_resource("", "ConfigMap", "configmaps"),
                fixture_api_resource("apps", "Deployment", "deployments"),
                fixture_api_resource("apps", "StatefulSet", "statefulsets"),
                fixture_api_resource("networking.k8s.io", "Ingress", "ingresses"),
            ],
        });
    harness.run();
    harness.snapshot("05_api_resources_loaded");
}
