//! Cordon, uncordon and drain from the Nodes table's row menu.

use super::*;
use crate::cluster_connection_manager::{DrainPod, NodeDrainReport};

fn node(name: &str, cordoned: bool) -> MinimalResource {
    let mut node = MinimalResource {
        uid: format!("{name}-uid"),
        name: name.to_owned(),
        namespace: None,
        creation_timestamp: None,
        controller_owner: None,
        labels: Default::default(),
        annotations: Default::default(),
        cells: BTreeMap::new(),
        log_containers: Vec::new(),
    };
    if cordoned {
        crate::resource_handlers::node::mark_cordoned(&mut node);
    }
    node
}

/// The Nodes table of the kind-kind fixture cluster with a schedulable and a
/// cordoned worker.
fn nodes_harness() -> Harness<'static, MyEguiApp<MockWorker>> {
    let nodes = fixture_cluster_scoped_api_resource("core", "Node", "nodes");
    let mut state = oracle_resource_table_state();
    let cluster = state.clusters.get_mut(&2).expect("kind fixture exists");
    cluster.selected_api_resource = Some(nodes.clone());
    cluster.resource_cache.insert(
        (nodes, None),
        ResourceWatchState {
            resources: [node("worker-a", false), node("worker-b", true)]
                .into_iter()
                .map(|node| (node.uid.clone(), node))
                .collect(),
            is_synced: true,
            error: None,
            revision: 0,
        },
    );
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = state;
    harness.run();
    harness
}

fn open_row_menu_item(harness: &mut Harness<MyEguiApp<MockWorker>>, node_name: &str, item: &str) {
    click_label(harness, &format!("More actions for {node_name}"));
    click_label(harness, item);
    harness.event(egui::Event::PointerGone);
    harness.run();
}

fn last_cordon(harness: &Harness<MyEguiApp<MockWorker>>) -> Option<(i32, u64, String, bool)> {
    harness
        .state()
        .worker
        .last_command::<CordonNode>()
        .map(|command| {
            (
                command.cluster_key,
                command.operation_id,
                command.node_name.clone(),
                command.cordon,
            )
        })
}

#[test]
fn the_row_menu_cordons_a_schedulable_node_and_uncordons_a_cordoned_one() {
    let mut harness = nodes_harness();

    click_label(&mut harness, "More actions for worker-b");
    assert!(
        harness.query_by_label("Cordon").is_none(),
        "a cordoned Node offers Uncordon instead"
    );
    click_label(&mut harness, "Uncordon");
    assert_eq!(
        last_cordon(&harness),
        Some((2, 1, "worker-b".to_owned(), false))
    );

    harness.event(egui::Event::PointerGone);
    harness.run();
    open_row_menu_item(&mut harness, "worker-a", "Cordon");
    assert_eq!(
        last_cordon(&harness),
        Some((2, 2, "worker-a".to_owned(), true))
    );

    harness.deliver_worker_result(NodeCordonCompleted {
        cluster_key: 2,
        operation_id: 2,
        node_name: "worker-a".to_owned(),
        cordon: true,
    });
    harness.deliver_worker_result(NodeCordonFailed {
        cluster_key: 2,
        operation_id: 1,
        node_name: "worker-b".to_owned(),
        cordon: false,
        error: "nodes \"worker-b\" is forbidden".to_owned(),
    });
    let history = &harness.state().ui_state.clusters[&2].operation_history;
    assert_eq!(
        history
            .iter()
            .map(|entry| (entry.title.as_str(), entry.target.as_str()))
            .collect::<Vec<_>>(),
        [
            ("Node cordoned", "worker-a"),
            ("Couldn’t uncordon Node", "worker-b")
        ]
    );
}

#[test]
fn drain_asks_for_confirmation_and_records_the_outcome_of_its_own_operation() {
    let mut harness = nodes_harness();

    open_row_menu_item(&mut harness, "worker-a", "Drain…");
    harness.ui_harness("resource_actions/drain_node/drain_confirmation");
    assert!(
        harness.state().worker.last_command::<DrainNode>().is_none(),
        "nothing is drained before the confirmation"
    );
    click_label(&mut harness, "Cancel");
    assert!(
        harness.state().ui_state.clusters[&2]
            .pending_node_drain
            .is_none()
    );
    assert!(harness.state().worker.last_command::<DrainNode>().is_none());

    open_row_menu_item(&mut harness, "worker-a", "Drain…");
    click_label(&mut harness, "Drain");
    let drain = harness
        .state()
        .worker
        .last_command::<DrainNode>()
        .map(|command| {
            (
                command.cluster_key,
                command.operation_id,
                command.node_name.clone(),
            )
        });
    assert_eq!(drain, Some((2, 1, "worker-a".to_owned())));
    assert_eq!(
        harness.state().ui_state.clusters[&2].node_drains.ids(),
        vec![1]
    );

    let report = NodeDrainReport {
        evicted: vec![DrainPod {
            namespace: "apps".to_owned(),
            name: "api".to_owned(),
            uid: None,
        }],
        failed: vec![(
            DrainPod {
                namespace: "apps".to_owned(),
                name: "db-0".to_owned(),
                uid: None,
            },
            "Still refused after 120s: budget".to_owned(),
        )],
        daemon_set_pods: 1,
        ..Default::default()
    };
    for operation_id in [7, 1, 1] {
        harness.deliver_worker_result(NodeDrainCompleted {
            cluster_key: 2,
            operation_id,
            node_name: "worker-a".to_owned(),
            report: report.clone(),
        });
    }
    let cluster = &harness.state().ui_state.clusters[&2];
    assert!(cluster.node_drains.ids().is_empty());
    let [entry] = cluster.operation_history.as_slice() else {
        panic!(
            "only the tracked drain's first result is recorded: {:?}",
            cluster.operation_history
        );
    };
    assert_eq!(
        (entry.title.as_str(), entry.target.as_str()),
        ("Node drain incomplete", "worker-a")
    );
    assert_eq!(
        entry.details.as_deref(),
        Some(
            "Evicted 1 of 2 Pods. The Node stays cordoned.\nLeft 1 DaemonSet Pod running.\n\
             apps/db-0: Still refused after 120s: budget"
        )
    );
}

#[test]
fn a_cordon_outcome_is_recorded_once_and_only_for_its_own_request() {
    let mut harness = nodes_harness();
    open_row_menu_item(&mut harness, "worker-a", "Cordon");
    assert_eq!(
        last_cordon(&harness),
        Some((2, 1, "worker-a".to_owned(), true))
    );

    for operation_id in [9, 1, 1] {
        harness.deliver_worker_result(NodeCordonCompleted {
            cluster_key: 2,
            operation_id,
            node_name: "worker-a".to_owned(),
            cordon: true,
        });
    }
    harness.deliver_worker_result(NodeCordonFailed {
        cluster_key: 2,
        operation_id: 1,
        node_name: "worker-a".to_owned(),
        cordon: true,
        error: "late duplicate".to_owned(),
    });

    let cluster = &harness.state().ui_state.clusters[&2];
    assert!(cluster.node_cordons.ids().is_empty());
    let [entry] = cluster.operation_history.as_slice() else {
        panic!(
            "only the tracked request's first outcome is recorded: {:?}",
            cluster.operation_history
        );
    };
    assert_eq!(entry.title, "Node cordoned");
}
