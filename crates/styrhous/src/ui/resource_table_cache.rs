use super::metadata_fields::MetadataKeySuggestions;
use super::state::ResourceWatchKey;
use crate::api_resource::ApiResource;
use components::SortDirection;

#[derive(Debug, Default)]
pub(super) struct ResourceTableCache {
    entry: Option<PreparedResourceTable>,
}

impl ResourceTableCache {
    pub(super) fn clear(&mut self) {
        self.entry = None;
    }

    pub(super) fn matches(&self, key: &ResourceTableCacheKey) -> bool {
        self.entry.as_ref().is_some_and(|entry| entry.key == *key)
    }

    pub(super) fn replace(&mut self, entry: PreparedResourceTable) {
        self.entry = Some(entry);
    }

    pub(super) fn prepared(&self) -> &PreparedResourceTable {
        self.entry
            .as_ref()
            .expect("resource-table cache is prepared before rendering")
    }

    pub(super) fn generation(&self) -> u64 {
        self.entry.as_ref().map_or(0, |entry| entry.generation)
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) struct ResourceTableCacheKey {
    pub(super) api_resource: ApiResource,
    pub(super) watch_revisions: Vec<(ResourceWatchKey, u64)>,
    pub(super) pod_metric_revisions: Vec<(String, u64)>,
    pub(super) node_metric_revision: u64,
    pub(super) pod_metrics_api_available: bool,
    pub(super) node_metrics_api_available: bool,
    pub(super) search_query: String,
    pub(super) regex_mode: bool,
    pub(super) sort: Option<(String, SortDirection)>,
}

#[derive(Debug)]
pub(super) struct PreparedResourceTable {
    pub(super) key: ResourceTableCacheKey,
    pub(super) watch_keys: Vec<ResourceWatchKey>,
    pub(super) rows: Vec<PreparedResourceTableRow>,
    pub(super) resource_count: usize,
    pub(super) visible_resource_count: usize,
    pub(super) regex_error: Option<String>,
    pub(super) metadata_key_suggestions: MetadataKeySuggestions,
    pub(super) generation: u64,
}

#[derive(Debug)]
pub(super) enum PreparedResourceTableRow {
    Resource(PreparedResourceIdentity),
    HiddenBySearch(usize),
}

#[derive(Debug)]
pub(super) struct PreparedResourceIdentity {
    pub(super) watch_index: usize,
    pub(super) uid: String,
}
