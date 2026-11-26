use super::{
    ResourceTableData, prepare_cluster_table, prepare_resource_table, resolve_prepared_resource,
    resolved_resource_cell,
};
use crate::minimal_resource::MinimalResource;
use crate::pod_metrics::format_cpu;
use crate::resource_table::{CPU_COLUMN, CellValue};
use crate::ui::resource_table_cache::{Grouping, PreparedResourceTableRow, ResourceTableCache};
use crate::ui::state::{
    ClusterState, PodMetricsNamespaceState, ResourceSearchState, ResourceWatchState,
    SelectedNamespaces, UiState,
};
use crate::ui::table_preferences::{MetadataColumnSource, PersistedResourceTablePreferences};
use crate::ui::workspace::resource_table::{
    ResourceTableConfiguration, resource_table_configuration,
};
use std::collections::{BTreeMap, HashMap};

fn api_resource(kind: &str, name: &str, namespaced: bool) -> crate::api_resource::ApiResource {
    crate::api_resource::ApiResource {
        group: "core".to_owned(),
        version: "v1".to_owned(),
        kind: kind.to_owned(),
        name: name.to_owned(),
        namespaced,
    }
}

fn table_resource(name: &str) -> MinimalResource {
    MinimalResource {
        uid: format!("uid-{name}"),
        name: name.to_owned(),
        namespace: Some("default".to_owned()),
        creation_timestamp: None,
        controller_owner: None,
        labels: BTreeMap::new(),
        annotations: BTreeMap::new(),
        cells: BTreeMap::new(),
        log_containers: Vec::new(),
    }
}

fn cluster_with_resources(
    api_resource: &crate::api_resource::ApiResource,
    resources: impl IntoIterator<Item = MinimalResource>,
) -> ClusterState {
    let mut cluster = ClusterState::for_test(1, "test");
    cluster.selected_namespaces = SelectedNamespaces::from(["default"]);
    cluster.resource_cache.insert(
        (api_resource.clone(), Some("default".to_owned())),
        ResourceWatchState {
            resources: resources
                .into_iter()
                .map(|resource| (resource.uid.clone(), resource))
                .collect(),
            is_synced: true,
            revision: 1,
            ..Default::default()
        },
    );
    cluster
}

fn prepared_names(
    cache: &mut ResourceTableCache,
    cluster: &ClusterState,
    api_resource: &crate::api_resource::ApiResource,
    search: &ResourceSearchState,
    configuration: &ResourceTableConfiguration,
) -> Vec<String> {
    let prepared = prepare_resource_table(
        cache,
        ResourceTableData::of(cluster, api_resource),
        api_resource,
        search,
        configuration,
    );
    prepared
        .rows
        .iter()
        .filter_map(|row| match row {
            PreparedResourceTableRow::Resource(identity) => {
                resolve_prepared_resource(&cluster.resource_cache, prepared, identity)
                    .map(|resource| resource.name.clone())
            }
            PreparedResourceTableRow::HiddenBySearch(_) => None,
        })
        .collect()
}

fn pod_usage(cpu_nanocores: i64) -> crate::pod_metrics::PodUsage {
    crate::pod_metrics::PodUsage {
        timestamp: time::OffsetDateTime::UNIX_EPOCH,
        cpu_nanocores,
        memory_bytes: cpu_nanocores,
        containers: BTreeMap::new(),
    }
}

mod every_namespace;
mod grouping;
mod invalidation;
mod listed_counts;
mod metrics;
mod preparation;
