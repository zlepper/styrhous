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

    harness.get_by_label("Regular expression").click();
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

/// A search that hides every row offers Clear filter, which clears the
/// search and brings the rows back.
#[test]
fn a_filter_that_hides_every_row_can_be_cleared() {
    let pods = fixture_api_resource("core", "Pod", "pods");
    let mut state = oracle_resource_table_state();
    state
        .clusters
        .get_mut(&2)
        .unwrap()
        .resource_searches
        .insert(
            pods.clone(),
            super::super::super::super::state::ResourceSearchState {
                query: "no-such-pod".into(),
                regex_mode: false,
            },
        );

    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = state;
    harness.run();

    harness.get_by_label("No matching resources");
    harness.get_by_label("Try a different search term.");
    harness
        .ui_harness("resource_tables/a_filter_that_hides_every_row_can_be_cleared/filtered_empty");

    harness.get_by_label("Clear filter").click();
    harness.run();
    assert_eq!(
        harness.state().ui_state.clusters[&2].resource_searches[&pods].query,
        ""
    );
    assert!(harness.query_by_label("No matching resources").is_none());
    harness.get_by_label("Open details for coredns-66bc5c9577-ffw2s");
}

/// Press `/` as a keyboard does: the key, then the text it types.
fn press_slash(harness: &mut egui_kittest::Harness<'_, impl Sized>) {
    let events = &mut harness.input_mut().events;
    events.push(egui::Event::Key {
        key: egui::Key::Slash,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    });
    events.push(egui::Event::Text("/".into()));
    harness.run();
}

#[test]
fn slash_focuses_the_resource_search_but_types_into_a_focused_field() {
    let pods = fixture_api_resource("core", "Pod", "pods");
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    harness.run();

    press_slash(&mut harness);
    harness.run();
    assert!(harness.get_by_label("Search resources").is_focused());
    assert_eq!(
        harness.state().ui_state.clusters[&2].resource_searches[&pods].query,
        "",
        "the slash that focuses the search is not typed into it"
    );

    // While another field has focus, `/` is just text.
    harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Namespace scope")
        .click();
    harness.run();
    let namespace_search =
        harness.get_by_role_and_label(egui::accesskit::Role::TextInput, "Search Namespace");
    assert!(namespace_search.is_focused());
    press_slash(&mut harness);
    assert_eq!(
        harness
            .get_by_role_and_label(egui::accesskit::Role::TextInput, "Search Namespace")
            .value()
            .as_deref(),
        Some("/")
    );
    assert!(!harness.get_by_label("Search resources").is_focused());
}

#[test]
fn the_resource_search_is_disabled_until_a_resource_is_selected() {
    let mut state = oracle_resource_table_state();
    state.clusters.get_mut(&2).unwrap().selected_api_resource = None;
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = state;
    harness.run();

    press_slash(&mut harness);
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::F);
    harness.run();
    let search = harness.get_by_label("Search resources");
    assert!(search.accesskit_node().is_disabled());
    assert!(!search.is_focused());
}
