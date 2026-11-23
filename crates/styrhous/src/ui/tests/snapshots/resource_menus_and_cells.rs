//! The resource actions menu (C07) and resource table behaviour (C08):
//! grouped actions, header sorting and column visibility.

use super::*;
use components::SortDirection;

/// The names of the resource table's rows, top to bottom.
fn row_names(harness: &Harness<MyEguiApp<MockWorker>>) -> Vec<String> {
    let mut rows = harness
        .query_all_by_role(egui::accesskit::Role::Button)
        .filter_map(|node| {
            let label = node.accesskit_node().label()?;
            let name = label.strip_prefix("Open details for ")?.to_owned();
            Some((node.rect().top(), name))
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| left.0.total_cmp(&right.0));
    rows.into_iter().map(|(_, name)| name).collect()
}

#[test]
fn copy_name_puts_the_resource_name_on_the_clipboard_and_closes_the_menu() {
    let mut harness = application_harness::<MockWorker>();
    show_apps_resource_table(&mut harness);
    let more_actions = harness
        .get_by_label("More actions for coredns-66bc5c9577-ffw2s")
        .rect()
        .center();
    primary_click(&mut harness, more_actions);
    harness.run();

    let copy = harness.get_by_label("Copy name").rect().center();
    primary_click(&mut harness, copy);
    harness.step();
    assert!(
        harness
            .output()
            .platform_output
            .commands
            .contains(&egui::OutputCommand::CopyText(
                "coredns-66bc5c9577-ffw2s".into()
            )),
        "Copy name puts the resource's name on the clipboard"
    );
    harness.run();
    assert!(
        harness.query_by_label("Copy name").is_none(),
        "the menu closes after an action"
    );
}

#[test]
fn clicking_a_resource_table_header_sorts_ascending_descending_then_by_default() {
    let mut harness = application_harness::<MockWorker>();
    show_apps_resource_table(&mut harness);
    let default_order = row_names(&harness);
    let mut ascending = default_order.clone();
    ascending.sort();
    let mut descending = ascending.clone();
    descending.reverse();

    // The default order is by name too, so the persisted sort tells the
    // first and third clicks apart.
    let pods = fixture_api_resource("core", "Pod", "pods");
    let name_header = resource_table_name_header_context_position(&harness);
    for (expected, sort) in [
        (&ascending, Some(SortDirection::Ascending)),
        (&descending, Some(SortDirection::Descending)),
        (&default_order, None),
    ] {
        primary_click(&mut harness, name_header);
        harness.run();
        assert_eq!(&row_names(&harness), expected);
        let persisted = harness
            .state_mut()
            .resource_table_preferences
            .sort(
                &ResourceTableKey::workspace(&pods),
                &[TableColumnDefinition::sortable("name", "Name", 160.0)],
            )
            .map(|sort| (sort.column_id, sort.direction));
        assert_eq!(
            persisted,
            sort.map(|direction| ("name".to_owned(), direction))
        );
    }
}

#[test]
fn the_header_menu_hides_columns_and_resets_them() {
    let mut harness = application_harness::<MockWorker>();
    show_apps_resource_table(&mut harness);
    let header = resource_table_name_header_context_position(&harness);

    secondary_click(&mut harness, header);
    harness.run_steps(2);
    let owner = harness
        .get_by_role_and_label(egui::accesskit::Role::CheckBox, "Owner")
        .rect()
        .center();
    primary_click(&mut harness, owner);
    harness.run();
    assert!(
        harness.query_by_label("Owner column").is_none(),
        "unticking a column hides it"
    );
    assert!(
        harness.query_by_label("Owner").is_some(),
        "a checkbox item keeps the menu open"
    );

    let reset = harness.get_by_label("Reset to default").rect().center();
    primary_click(&mut harness, reset);
    harness.run();
    harness.get_by_label("Owner column");
}

#[test]
fn configure_presets_in_a_debug_image_menu_opens_the_terminal_settings() {
    let mut harness = application_harness::<MockWorker>();
    let settings = TerminalLaunchSettings {
        debug_image_presets: vec![DebugImagePreset {
            name: "Operations".into(),
            image: "registry.example/ops:v1".into(),
            profile: DebugProfile::General,
        }],
        ..Default::default()
    };
    harness.state_mut().terminal_launch_settings = settings.clone();
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
            revision: 0,
        },
    );
    harness.state_mut().ui_state = state;
    harness.run();

    let more_actions = harness
        .get_by_label("More actions for kind-control-plane")
        .rect()
        .center();
    primary_click(&mut harness, more_actions);
    harness.run();
    let shell = harness.get_by_label("Shell").rect().center();
    primary_click(&mut harness, shell);
    harness.run();
    harness.get_by_label("Operations (General profile)");
    click_label(&mut harness, "Configure presets…");

    assert_eq!(
        harness
            .state()
            .ui_state
            .terminal_settings_blade()
            .expect("Configure presets… opens the terminal settings")
            .draft,
        settings,
        "the settings open with the saved presets"
    );
}

#[test]
fn missing_cells_say_why_they_are_empty() {
    let mut harness = application_harness::<MockWorker>();
    let mut state = oracle_resource_table_state();
    let pods = fixture_api_resource("core", "Pod", "pods");
    let pod = state
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
    pod.creation_timestamp = None;
    pod.cells
        .insert(NODE_COLUMN.into(), CellValue::Text("-".into()));
    harness.state_mut().ui_state = state;
    harness.run();
    click_label(&mut harness, "Apps & Containers");

    harness.get_by_label("No creation time reported");
    assert!(
        harness
            .query_all_by_label("Kubernetes has not assigned this Pod to a Node.")
            .next()
            .is_some(),
        "an unscheduled Pod's node reads as missing"
    );
    assert!(
        harness.query_by_label("Open details for Node -").is_none(),
        "with no node to open"
    );
    assert!(
        harness
            .query_all_by_label("No controlling owner")
            .next()
            .is_some()
    );
}
