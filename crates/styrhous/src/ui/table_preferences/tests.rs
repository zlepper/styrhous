use super::*;
fn resource(group: &str, name: &str) -> ApiResource {
    ApiResource {
        group: group.into(),
        version: "v1".into(),
        kind: name.into(),
        name: name.into(),
        namespaced: true,
    }
}
fn columns() -> Vec<TableColumnDefinition> {
    vec![
        TableColumnDefinition {
            id: "name".into(),
            label: "Name".into(),
            default_width: 180.0,
            sortable: true,
            numeric: false,
        },
        TableColumnDefinition {
            id: "age".into(),
            label: "Age".into(),
            default_width: 77.0,
            sortable: true,
            numeric: false,
        },
    ]
}
#[test]
fn workspace_key_ignores_cluster_and_version() {
    let mut first = resource("", "pods");
    first.version = "v1".into();
    let mut second = resource("core", "pods");
    second.version = "v2".into();
    assert_eq!(
        ResourceTableKey::workspace(&first),
        ResourceTableKey::workspace(&second)
    );
}
#[test]
fn layout_merges_new_columns_and_keeps_one_visible() {
    let key = ResourceTableKey::workspace(&resource("core", "pods"));
    let mut preferences = PersistedResourceTablePreferences::default();
    preferences.set_visible(&key, &columns(), "name", false);
    preferences.set_visible(&key, &columns(), "age", false);
    assert_eq!(preferences.resolved_columns(&key, &columns()).len(), 1);
    let mut extended = columns();
    extended.push(TableColumnDefinition {
        id: "status".into(),
        label: "Status".into(),
        default_width: 100.0,
        sortable: true,
        numeric: false,
    });
    assert_eq!(preferences.all_columns(&key, &extended).len(), 3);
}

#[test]
fn stale_columns_do_not_allow_hiding_every_available_column() {
    let key = ResourceTableKey::workspace(&resource("core", "pods"));
    let mut preferences = PersistedResourceTablePreferences::default();
    let mut wide_columns = columns();
    wide_columns.push(TableColumnDefinition {
        id: "namespace".into(),
        label: "Namespace".into(),
        default_width: 120.0,
        sortable: true,
        numeric: false,
    });
    preferences.all_columns(&key, &wide_columns);

    preferences.set_visible(&key, &columns(), "name", false);
    preferences.set_visible(&key, &columns(), "age", false);

    assert_eq!(preferences.resolved_columns(&key, &columns()).len(), 1);
}

#[test]
fn actions_column_cannot_be_resized_below_the_readable_minimum() {
    let key = ResourceTableKey::workspace(&resource("core", "pods"));
    let mut preferences = PersistedResourceTablePreferences::default();
    let actions = TableColumnDefinition {
        id: "actions".into(),
        label: "Actions".into(),
        default_width: 104.0,
        sortable: false,
        numeric: false,
    };

    preferences.set_width(&key, std::slice::from_ref(&actions), "actions", 12.0);

    assert_eq!(
        preferences.resolved_columns(&key, &[actions])[0].width,
        TABLE_MIN_COLUMN
    );
}

#[test]
fn detail_key_includes_the_outer_resource_type() {
    let pods = resource("", "pods");
    let deployment = resource("apps", "deployments");
    let replica_set = resource("apps", "replicasets");

    assert_ne!(
        ResourceTableKey::detail(&deployment, &pods),
        ResourceTableKey::detail(&replica_set, &pods)
    );
}

#[test]
fn custom_columns_preserve_layout_state_and_can_be_renamed_or_removed() {
    let key = ResourceTableKey::workspace(&resource("core", "pods"));
    let custom = CustomMetadataColumn {
        source: MetadataColumnSource::Annotation,
        key: "example.com/team".into(),
        label: "Team".into(),
    };
    let id = custom.id();
    let mut preferences = PersistedResourceTablePreferences::default();

    assert!(preferences.add_custom_column(&key, custom.clone()));
    assert!(!preferences.add_custom_column(&key, custom));
    assert!(preferences.rename_custom_column(&key, &id, "Owning team".into()));
    assert_eq!(preferences.custom_columns(&key)[0].label, "Owning team");

    let mut definitions = columns();
    definitions.push(TableColumnDefinition {
        id: id.clone(),
        label: "Owning team".into(),
        default_width: 160.0,
        sortable: true,
        numeric: false,
    });
    preferences.set_width(&key, &definitions, &id, 240.0);
    preferences.set_sort(&key, &definitions, &id, SortDirection::Ascending);
    let resolved = preferences.resolved_columns(&key, &definitions);
    assert_eq!(
        resolved
            .iter()
            .find(|column| column.definition.id == id)
            .expect("custom column is present")
            .width,
        240.0
    );
    let custom_index = resolved
        .iter()
        .position(|column| column.definition.id == id)
        .expect("custom column is present");
    let age_index = resolved
        .iter()
        .position(|column| column.definition.id == "age")
        .expect("age column is present");
    assert!(custom_index < age_index);

    assert!(preferences.remove_custom_column(&key, &id));
    assert!(preferences.custom_columns(&key).is_empty());
    assert!(preferences.sort(&key, &columns()).is_none());
    assert!(!preferences.remove_custom_column(&key, &id));
}

/// The persisted sort as (column, ascending).
fn sort_of(
    preferences: &mut PersistedResourceTablePreferences,
    key: &ResourceTableKey,
) -> Option<(String, bool)> {
    preferences
        .sort(key, &columns())
        .map(|sort| (sort.column_id, sort.direction == SortDirection::Ascending))
}

#[test]
fn header_clicks_cycle_the_persisted_sort_ascending_descending_and_default() {
    let key = ResourceTableKey::workspace(&resource("core", "pods"));
    let mut preferences = PersistedResourceTablePreferences::default();

    let mut after_click = |column: &str| {
        preferences.sort_after_header_click(&key, &columns(), column);
        sort_of(&mut preferences, &key)
    };
    assert_eq!(after_click("name"), Some(("name".into(), true)));
    assert_eq!(after_click("name"), Some(("name".into(), false)));
    assert_eq!(after_click("name"), None);
    assert_eq!(after_click("name"), Some(("name".into(), true)));
    assert_eq!(
        after_click("age"),
        Some(("age".into(), true)),
        "another column starts ascending"
    );
}

#[test]
fn resetting_column_visibility_keeps_order_and_widths() {
    let key = ResourceTableKey::workspace(&resource("core", "pods"));
    let mut preferences = PersistedResourceTablePreferences::default();
    preferences.set_order(&key, &columns(), &["age".into(), "name".into()]);
    preferences.set_width(&key, &columns(), "name", 240.0);
    preferences.set_visible(&key, &columns(), "age", false);

    preferences.reset_column_visibility(&key, &columns());

    let resolved = preferences.resolved_columns(&key, &columns());
    assert_eq!(
        resolved
            .iter()
            .map(|column| (column.definition.id.as_str(), column.width))
            .collect::<Vec<_>>(),
        [("age", 77.0), ("name", 240.0)]
    );
}

#[test]
fn hiding_the_sorted_column_returns_to_the_default_order() {
    let key = ResourceTableKey::workspace(&resource("core", "pods"));
    let mut preferences = PersistedResourceTablePreferences::default();
    preferences.set_sort(&key, &columns(), "age", SortDirection::Descending);

    preferences.set_visible(&key, &columns(), "name", false);
    assert_eq!(
        sort_of(&mut preferences, &key),
        Some(("age".into(), false)),
        "hiding another column keeps the sort"
    );
    preferences.set_visible(&key, &columns(), "name", true);
    preferences.set_visible(&key, &columns(), "age", false);
    assert_eq!(sort_of(&mut preferences, &key), None);
}
