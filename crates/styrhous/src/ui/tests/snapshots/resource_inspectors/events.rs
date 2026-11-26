use super::*;

#[test]
fn pod_events_stay_aligned_and_inside_inspector_as_updates_arrive() {
    let mut harness = application_harness::<MockWorker>();
    harness
        .ctx
        .global_style_mut(|style| style.animation_time = 0.0);
    harness.state_mut().ui_state = oracle_resource_table_state();
    harness.run();
    harness.get_by_label("Workloads").click();
    harness.run();
    let name = "coredns-66bc5c9577-ffw2s";
    harness
        .get_by_label(&format!("Open details for {name}"))
        .click();
    harness.run_steps(1);
    harness
        .state_mut()
        .worker
        .results
        .push_back(Box::new(ResourceDetailUpdated {
            cluster_key: 2,
            history_entry_id: 1,
            detail: Box::new(ResourceDetail {
                yaml: None,
                api_resource: fixture_api_resource("core", "Pod", "pods"),
                name: name.into(),
                namespace: Some("kube-system".into()),
                uid: "fixture-0".into(),
                resource_version: "1".into(),
                is_deleting: false,
                finalizers: Vec::new(),
                creation_timestamp: None,
                owners: Vec::new(),
                labels: BTreeMap::new(),
                annotations: BTreeMap::new(),
                payload: ResourceDetailPayload::Pod(Box::new(PodDetail {
                    phase: "Running".into(),
                    ..Default::default()
                })),
            }),
        }));
    harness.run();
    let inspector_right = harness.get_by_label("Close blade").rect().right();
    let mut initial_columns = None;
    for count in [1, 7, 25] {
        harness.state_mut().worker.results.push_back(Box::new(ResourceEventsReplaced {
            cluster_key: 2,
            history_entry_id: 1,
            events: (0..count).map(|index| ResourceEvent {
                uid: format!("event-{index}"),
                type_: if index % 2 == 0 { "Normal" } else { "Warning" }.into(),
                reason: format!("EventReason{index}"),
                message: if index % 2 == 0 {
                    "Successfully pulled image registry.example.com/docker.io/library/rabbitmq:4.3.2-management".into()
                } else {
                    "0/29 nodes are available: 2 node(s) didn't match pod topology spread constraints.\nPreemption: no suitable nodes found.".into()
                },
                source: Some("kubelet".into()),
                count: 1,
                last_timestamp: None,
            }).collect(),
        }));
        harness.run();
        if count == 1 {
            // Normal events are hidden until asked for.
            harness.get_by_label_contains("Show ").click();
            harness.run();
        }
        harness.get_by_label("Reason").scroll_to_me();
        harness.run();
        let columns = ["Reason", "Message", "Source", "Time"]
            .map(|label| harness.get_by_label(label).rect().left());
        if let Some(initial) = initial_columns {
            assert_eq!(columns, initial, "event updates moved the columns");
        } else {
            initial_columns = Some(columns);
        }
        harness.get_by_label("EventReason0");
        assert_eq!(harness.get_all_by_label("kubelet").count(), count);
        for source in harness.get_all_by_label("kubelet") {
            assert!(
                (source.rect().left() - columns[2]).abs() <= 1.0,
                "event source is not aligned with its header"
            );
            assert!(
                source.rect().right() <= inspector_right,
                "event content widened the inspector"
            );
        }
    }
}

/// The Events entry of the section rail counts every event and, in the danger
/// colour, how many of them are warnings; with none it shows the total only.
#[test]
fn the_events_rail_entry_counts_the_warning_events_beside_the_total() {
    let mut harness = application_harness::<MockWorker>();
    harness
        .ctx
        .global_style_mut(|style| style.animation_time = 0.0);
    harness.state_mut().ui_state = oracle_resource_table_state();
    // The table rows are aged against the real clock, the detail against the
    // design day: put both on the design day so the ages in the snapshot hold.
    let design_now = crate::ui::tests::design_fixtures::design_now();
    let ui_state = &mut harness.state_mut().ui_state;
    ui_state.clock = crate::ui::clock::WallClock::Fixed(design_now);
    for cluster in ui_state.clusters.values_mut() {
        for watch in cluster.resource_cache.values_mut() {
            for resource in watch.resources.values_mut() {
                resource.creation_timestamp = Some(design_now - time::Duration::days(220));
            }
        }
    }
    harness.run();
    harness.get_by_label("Workloads").click();
    harness.run();
    harness
        .get_by_label("Open details for coredns-66bc5c9577-ffw2s")
        .click();
    harness.run_steps(1);
    harness
        .state_mut()
        .worker
        .results
        .push_back(Box::new(ResourceDetailUpdated {
            cluster_key: 2,
            history_entry_id: 1,
            detail: Box::new(crate::ui::tests::design_fixtures::coredns_pod_detail()),
        }));
    harness.run();
    let events = |types: &[&str]| {
        Box::new(ResourceEventsReplaced {
            cluster_key: 2,
            history_entry_id: 1,
            events: types
                .iter()
                .enumerate()
                .map(|(index, type_)| ResourceEvent {
                    uid: format!("event-{index}"),
                    type_: (*type_).into(),
                    reason: format!("EventReason{index}"),
                    message: "Something happened".into(),
                    source: Some("kubelet".into()),
                    count: 1,
                    last_timestamp: None,
                })
                .collect(),
        })
    };

    harness
        .state_mut()
        .worker
        .results
        .push_back(events(&["Normal", "Normal"]));
    harness.run();
    harness.get_by_label("Events, 2");

    harness.state_mut().worker.results.push_back(events(&[
        "Warning", "Normal", "Warning", "Normal", "Normal",
    ]));
    harness.run();
    harness.get_by_label("Events, 5, 2 problems");
    harness.ui_harness("resource_inspectors/events/the_events_rail_entry_counts_warnings");
}
