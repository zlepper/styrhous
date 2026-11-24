//! Serializable UI choices that outlive a transient cluster connection.

use super::state::SelectedNamespaces;
use crate::api_resource::{ApiResource, ApiResourceSet};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Cluster contexts are rebuilt whenever the kubeconfig is reloaded, while namespace and API
/// discovery complete asynchronously. Keeping these choices separately prevents that rebuild
/// from overwriting the values which still need to be restored.
#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(super) struct PersistedClusterSelections {
    #[serde(default)]
    pub(super) selections: BTreeMap<String, PersistedClusterSelection>,
    #[serde(default)]
    pub(super) last_selected_context: Option<String>,
}

/// The last completed API discovery of each kubeconfig context, shown while a new
/// connection discovers the cluster again.
#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(super) struct PersistedApiDiscovery {
    #[serde(default)]
    pub(super) contexts: BTreeMap<String, ApiResourceSet>,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(super) struct ResourceNavigationExpansion {
    #[serde(default)]
    pub(super) expanded_nodes: BTreeSet<String>,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(super) struct PersistedClusterSelection {
    #[serde(default)]
    pub(super) selected_namespaces: SelectedNamespaces,
    #[serde(default)]
    pub(super) selected_api_resource: Option<PersistedApiResource>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub(super) struct PersistedApiResource {
    pub(super) group: String,
    pub(super) name: String,
}

impl PersistedApiResource {
    pub(super) fn from_api_resource(api_resource: &ApiResource) -> Self {
        Self {
            group: api_resource.identity().0.to_owned(),
            name: api_resource.name.clone(),
        }
    }

    pub(super) fn matches(&self, api_resource: &ApiResource) -> bool {
        api_resource.identity() == (self.group.as_str(), self.name.as_str())
    }
}

/// The kinds pinned to the top of the sidebar (S09), in the order they show.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub(super) struct PinnedKinds {
    #[serde(default)]
    pub(super) kinds: Vec<PersistedApiResource>,
}

impl Default for PinnedKinds {
    /// Pods, Deployments, Nodes and Helm releases, as the design pins them.
    fn default() -> Self {
        let kind = |group: &str, name: &str| PersistedApiResource {
            group: group.to_owned(),
            name: name.to_owned(),
        };
        Self {
            kinds: vec![
                kind(crate::api_resource::CORE_GROUP, "pods"),
                kind("apps", "deployments"),
                kind(crate::api_resource::CORE_GROUP, "nodes"),
                kind(crate::helm_release::GROUP, crate::helm_release::NAME),
            ],
        }
    }
}

impl PinnedKinds {
    pub(super) fn contains(&self, api_resource: &ApiResource) -> bool {
        self.kinds.iter().any(|kind| kind.matches(api_resource))
    }

    /// Pin `api_resource` after the pinned kinds, or unpin it.
    pub(super) fn set_pinned(&mut self, api_resource: &ApiResource, pinned: bool) {
        self.kinds.retain(|kind| !kind.matches(api_resource));
        if pinned {
            self.kinds
                .push(PersistedApiResource::from_api_resource(api_resource));
        }
    }
}
