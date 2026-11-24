use super::metadata_fields::MetadataKeySuggestions;
use super::state::ResourceWatchKey;
use crate::api_resource::ApiResource;
use components::SortDirection;
use components::design::theme::Tone;
use std::ops::Range;

/// How a resource table's rows are grouped under group headers (S17).
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
pub(super) enum Grouping {
    /// One list of rows.
    #[default]
    None,
    /// A group per namespace.
    Namespace,
    /// One group for a cluster-scoped kind (the Nodes).
    Cluster,
}

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
    /// The Node Pod count revision, while the Nodes table is sorted by its Pods.
    pub(super) node_pod_count_revision: u64,
    pub(super) search_query: String,
    pub(super) regex_mode: bool,
    pub(super) sort: Option<(String, SortDirection)>,
    pub(super) grouping: Grouping,
    /// Only the problem rows show.
    pub(super) problems_only: bool,
}

#[derive(Debug)]
pub(super) struct PreparedResourceTable {
    pub(super) key: ResourceTableCacheKey,
    pub(super) watch_keys: Vec<ResourceWatchKey>,
    /// The rows to show, group after group.
    pub(super) rows: Vec<PreparedResourceTableRow>,
    /// The groups of `rows`, in order.
    pub(super) groups: Vec<PreparedGroup>,
    pub(super) resource_count: usize,
    /// The resources the search matches.
    pub(super) visible_resource_count: usize,
    /// The resources the search matches that are doing badly.
    pub(super) problem_count: usize,
    /// The resources shown: those the search matches, and only the problem
    /// ones while `problems_only`.
    pub(super) shown_count: usize,
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
    /// How badly the resource is doing, when it is not well.
    pub(super) problem: Option<Tone>,
}

/// A run of rows under one group header.
#[derive(Debug)]
pub(super) struct PreparedGroup {
    /// The group's name; `None` for rows without a header.
    pub(super) name: Option<String>,
    /// The group's resources the search matches, shown or not.
    pub(super) total: usize,
    /// Those of them that are doing badly.
    pub(super) failing: usize,
    /// The group's rows in `PreparedResourceTable::rows`.
    pub(super) rows: Range<usize>,
}
