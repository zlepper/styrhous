use super::*;
use crate::ui::table_preferences::CustomMetadataColumn;

#[test]
fn prepared_table_uses_production_filter_sort_and_metadata_paths() {
    let deployment = crate::api_resource::ApiResource {
        group: "apps".to_owned(),
        version: "v1".to_owned(),
        kind: "Deployment".to_owned(),
        name: "deployments".to_owned(),
        namespaced: true,
    };
    let mut resources = ["my-api", "a-p-i", "api-server", "api", "worker"].map(table_resource);
    for (resource, rank) in resources.iter_mut().zip(["c", "a", "b", "d", "e"]) {
        resource.labels.insert("rank".to_owned(), rank.to_owned());
    }
    resources[0]
        .annotations
        .insert("example.com/team".to_owned(), "platform".to_owned());
    let cluster = cluster_with_resources(&deployment, resources);
    let mut configuration = resource_table_configuration(
        1_280.0,
        &deployment,
        &[],
        false,
        &mut PersistedResourceTablePreferences::default(),
    );
    let mut cache = ResourceTableCache::default();

    assert_eq!(
        prepared_names(
            &mut cache,
            &cluster,
            &deployment,
            &ResourceSearchState::default(),
            &configuration,
        ),
        ["a-p-i", "api", "api-server", "my-api", "worker"]
    );

    let fuzzy_search = ResourceSearchState {
        query: "api".to_owned(),
        regex_mode: false,
    };
    assert_eq!(
        prepared_names(
            &mut cache,
            &cluster,
            &deployment,
            &fuzzy_search,
            &configuration,
        ),
        ["api", "api-server", "my-api", "a-p-i"]
    );
    let prepared = cache.prepared();
    assert_eq!(prepared.resource_count, 5);
    assert_eq!(prepared.visible_resource_count, 4);
    assert!(matches!(
        prepared.rows.last(),
        Some(PreparedResourceTableRow::HiddenBySearch(1))
    ));
    assert_eq!(prepared.metadata_key_suggestions.labels, ["rank"]);
    assert_eq!(
        prepared.metadata_key_suggestions.annotations,
        ["example.com/team"]
    );

    configuration.sort_state = Some(components::SortState::new(
        "owner",
        components::SortDirection::Ascending,
    ));
    assert_eq!(
        prepared_names(
            &mut cache,
            &cluster,
            &deployment,
            &fuzzy_search,
            &configuration,
        ),
        ["api", "api-server", "my-api", "a-p-i"]
    );

    let metadata_column = CustomMetadataColumn {
        source: MetadataColumnSource::Label,
        key: "rank".to_owned(),
        label: "Rank".to_owned(),
    };
    let metadata_column_id = metadata_column.id();
    configuration.metadata_columns = vec![metadata_column];
    configuration.sort_state = Some(components::SortState::new(
        &metadata_column_id,
        components::SortDirection::Ascending,
    ));
    assert_eq!(
        prepared_names(
            &mut cache,
            &cluster,
            &deployment,
            &ResourceSearchState::default(),
            &configuration,
        ),
        ["a-p-i", "api-server", "my-api", "api", "worker"]
    );

    let invalid_regex = ResourceSearchState {
        query: "[".to_owned(),
        regex_mode: true,
    };
    assert!(
        prepared_names(
            &mut cache,
            &cluster,
            &deployment,
            &invalid_regex,
            &configuration,
        )
        .is_empty()
    );
    assert!(cache.prepared().regex_error.is_some());
}
