//! Serializable UI choices that outlive a transient cluster connection.

use crate::api_resource::ApiResource;
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

#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(super) struct ResourceNavigationExpansion {
    #[serde(default)]
    pub(super) expanded_nodes: BTreeSet<String>,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(super) struct PersistedClusterSelection {
    #[serde(default)]
    pub(super) selected_namespaces: BTreeSet<String>,
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
            group: canonical_api_group(&api_resource.group).to_owned(),
            name: api_resource.name.clone(),
        }
    }

    pub(super) fn matches(&self, api_resource: &ApiResource) -> bool {
        self.group == canonical_api_group(&api_resource.group) && self.name == api_resource.name
    }
}

fn canonical_api_group(group: &str) -> &str {
    if group.is_empty() { "core" } else { group }
}
