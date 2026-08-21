use super::*;

#[test]
fn resource_search_filters_rows_and_restores_its_mode_per_resource_type() {
    let pods = fixture_api_resource("core", "Pod", "pods");
    let nodes = fixture_api_resource("core", "Node", "nodes");
    let mut state = oracle_resource_table_state();
    state.clusters.get_mut(&2).unwrap().resource_cache.insert(
        (nodes, Some("kube-system".into())),
        ResourceWatchState {
            is_synced: true,
            ..Default::default()
        },
    );
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = state;
    harness.run();

    harness.get_by_label("Search resources").click();
    harness.run();
    harness
        .input_mut()
        .events
        .push(egui::Event::Text("c66z".into()));
    harness.run();

    harness.get_by_label("Open details for coredns-66bc5c9577-z9gt9");
    harness.get_by_label("7 resources hidden by search");
    assert_eq!(
        harness.state().ui_state.clusters[&2].resource_searches[&pods].query,
        "c66z"
    );

    harness.get_by_label("Use regex search").click();
    harness.run();
    assert!(harness.state().ui_state.clusters[&2].resource_searches[&pods].regex_mode);
    harness
        .state_mut()
        .ui_state
        .clusters
        .get_mut(&2)
        .unwrap()
        .resource_searches
        .insert(
            pods.clone(),
            super::super::super::super::state::ResourceSearchState {
                query: "COREDNS.*Z9".into(),
                regex_mode: true,
            },
        );
    harness.run();
    harness.get_by_label("Open details for coredns-66bc5c9577-z9gt9");
    harness.get_by_label("7 resources hidden by search");
    harness.ui_harness("resource_tables/resource_search_filters_rows_and_restores_its_mode_per_resource_type/resource_search_filtered");

    harness.get_by_label("Nodes").click_accesskit();
    harness.run();
    harness.get_by_label("Apps & Containers").click_accesskit();
    harness.run();
    harness.get_by_label("Pods").click_accesskit();
    harness.run();
    assert_eq!(
        harness.state().ui_state.clusters[&2].resource_searches[&pods].query,
        "COREDNS.*Z9"
    );
    assert!(harness.state().ui_state.clusters[&2].resource_searches[&pods].regex_mode);
}

#[test]
fn invalid_resource_search_regex_is_shown_in_workspace() {
    let pods = fixture_api_resource("core", "Pod", "pods");
    let mut state = oracle_resource_table_state();
    state
        .clusters
        .get_mut(&2)
        .unwrap()
        .resource_searches
        .insert(
            pods,
            super::super::super::super::state::ResourceSearchState {
                query: "coredns[a-z".into(),
                regex_mode: true,
            },
        );

    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = state;
    harness.run();

    harness.get_by_label("Invalid regular expression");
    harness.ui_harness("resource_tables/invalid_resource_search_regex_is_shown_in_workspace/resource_search_invalid_regex");
}
