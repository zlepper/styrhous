use super::*;

#[derive(Debug)]
pub(crate) struct LoadClusters;
#[derive(Debug)]
pub(crate) struct LoadImportedClusters;
#[derive(Debug)]
pub(crate) struct LoadManagedClusterDiscovery;
#[derive(Debug)]
pub(crate) struct AddAksCluster {
    pub(crate) subscription_id: String,
    pub(crate) resource_group: String,
    pub(crate) cluster_name: String,
}
#[derive(Debug)]
pub(crate) struct AddTailscaleCluster {
    pub(crate) host_name: String,
}
#[derive(Debug)]
pub(crate) struct ConnectToCluster {
    pub(crate) cluster: String,
    pub(crate) cluster_key: i32,
}
/// Replace every source used for one resource type as a single worker-side
/// operation. This makes a namespace-scope change immediately supersede any
/// queued initializations from the previous scope.
#[derive(Debug)]
pub(crate) struct ReconcileResourceWatches {
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) sources: Vec<ResourceWatchSource>,
}

#[derive(Debug, Clone)]
pub(crate) enum ResourceWatchSource {
    Namespace(String),
    AllNamespaces(BTreeSet<String>),
    Cluster,
}
#[derive(Debug)]
pub(crate) struct StartResourceDetailWatch {
    pub(crate) cluster_key: i32,
    pub(crate) history_entry_id: u64,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) resource_name: String,
    pub(crate) resource_uid: String,
    pub(crate) pod_metrics_api_available: bool,
    pub(crate) node_metrics_api_available: bool,
}
#[derive(Debug)]
pub(crate) struct StopResourceDetailWatch {
    pub(crate) cluster_key: i32,
    pub(crate) history_entry_id: u64,
}
#[derive(Debug)]
pub(crate) struct StartPodMetricsWatch {
    pub(crate) cluster_key: i32,
    pub(crate) namespace: String,
}
#[derive(Debug)]
pub(crate) struct StopPodMetricsWatch {
    pub(crate) cluster_key: i32,
    pub(crate) namespace: String,
}
#[derive(Debug)]
pub(crate) struct StartNodeMetricsWatch {
    pub(crate) cluster_key: i32,
}
#[derive(Debug)]
pub(crate) struct StopNodeMetricsWatch {
    pub(crate) cluster_key: i32,
}
#[derive(Debug)]
pub(crate) struct GetResourceYaml {
    pub(crate) editor_id: u64,
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) resource_name: String,
}
#[derive(Debug)]
pub(crate) struct DeleteResource {
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) resource_name: String,
    pub(crate) resource_uid: Option<String>,
    pub(crate) bulk_delete_id: Option<u64>,
}
#[derive(Debug)]
pub(crate) struct ForceDeleteResource {
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) resource_name: String,
    pub(crate) resource_uid: String,
}
#[derive(Debug)]
pub(crate) struct RestartDeployment {
    pub(crate) cluster_key: i32,
    pub(crate) namespace: String,
    pub(crate) resource_name: String,
}
#[derive(Debug)]
pub(crate) struct RunCronJob {
    pub(crate) cluster_key: i32,
    pub(crate) namespace: String,
    pub(crate) resource_name: String,
}
#[derive(Debug)]
pub(crate) struct GetResourceScale {
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) resource_name: String,
}
#[derive(Debug)]
pub(crate) struct UpdateResourceScale {
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) resource_name: String,
    pub(crate) replicas: i32,
}
pub(crate) struct ApplyResourceYaml {
    pub(crate) editor_id: u64,
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) resource_name: String,
    pub(crate) yaml: String,
}
#[derive(Debug)]
pub(crate) struct LoadResourceSchema {
    pub(crate) editor_id: u64,
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
}
pub(crate) struct ValidateResourceYaml {
    pub(crate) editor_id: u64,
    pub(crate) revision: u64,
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) resource_name: String,
    pub(crate) yaml: String,
}
#[derive(Debug)]
pub(crate) struct UpdateResourceData {
    pub(crate) cluster_key: i32,
    pub(crate) history_entry_id: u64,
    pub(crate) request_id: u64,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: String,
    pub(crate) resource_name: String,
    pub(crate) update: ResourceDataUpdate,
}
#[derive(Debug)]
pub(crate) struct StartPodLogStream {
    pub(crate) cluster_key: i32,
    pub(crate) log_window_id: u64,
    pub(crate) namespace: String,
    pub(crate) pod_name: String,
    pub(crate) container: String,
}
#[derive(Debug)]
pub(crate) struct StopPodLogStream {
    pub(crate) cluster_key: i32,
    pub(crate) log_window_id: u64,
}

/// The values are intentionally omitted from Debug output because this command can
/// contain Secret plaintext. The worker logs failed commands at debug format.
pub struct ResourceDataUpdate {
    pub expected_resource_version: String,
    pub expected_values: BTreeMap<String, String>,
    pub updated_values: BTreeMap<String, String>,
}

impl std::fmt::Debug for ResourceDataUpdate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResourceDataUpdate")
            .field("keys", &self.updated_values.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl std::fmt::Debug for ApplyResourceYaml {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApplyResourceYaml")
            .field("editor_id", &self.editor_id)
            .field("cluster_key", &self.cluster_key)
            .field("api_resource", &self.api_resource)
            .field("namespace", &self.namespace)
            .field("resource_name", &self.resource_name)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for ValidateResourceYaml {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ValidateResourceYaml")
            .field("editor_id", &self.editor_id)
            .field("revision", &self.revision)
            .field("cluster_key", &self.cluster_key)
            .field("api_resource", &self.api_resource)
            .field("namespace", &self.namespace)
            .field("resource_name", &self.resource_name)
            .finish_non_exhaustive()
    }
}
