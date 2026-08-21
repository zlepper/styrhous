use super::*;

#[derive(Debug)]
pub(crate) struct KubernetesClustersUpdated(pub(crate) Vec<Cluster>);
#[derive(Debug)]
pub(crate) struct ImportedKubernetesClusters(pub(crate) Vec<Cluster>);
#[derive(Debug)]
pub(crate) struct ManagedClusterImported;
#[derive(Debug)]
pub(crate) struct ManagedClusterDiscoveryUpdated {
    pub(crate) tools: ClusterDiscoveryTools,
    pub(crate) aks_clusters: Vec<AvailableAksCluster>,
    pub(crate) tailscale_clusters: Vec<AvailableTailscaleCluster>,
    pub(crate) azure_error: Option<String>,
    pub(crate) azure_warning: Option<String>,
    pub(crate) tailscale_error: Option<String>,
}
#[derive(Debug)]
pub(crate) struct ManagedClusterDiscoveryFailed {
    pub(crate) error: String,
}
#[derive(Debug)]
pub(crate) struct KubernetesNamespacesAdded {
    pub(crate) cluster_key: i32,
    pub(crate) namespace: MinimalNamespace,
}
#[derive(Debug)]
pub(crate) struct KubernetesNamespacesDeleted {
    pub(crate) cluster_key: i32,
    pub(crate) namespace_name: String,
}
#[derive(Debug)]
pub(crate) struct KubernetesNamespacesReplaced {
    pub(crate) cluster_key: i32,
    pub(crate) namespaces: Vec<MinimalNamespace>,
}
#[derive(Debug)]
pub(crate) struct KubernetesNamespacesLoadFailed {
    pub(crate) cluster_key: i32,
    pub(crate) error: String,
}
#[derive(Debug)]
pub(crate) struct KubernetesApisLoaded {
    pub(crate) cluster_key: i32,
    pub(crate) api_resources: Vec<ApiResource>,
    pub(crate) scalable_api_resources: std::collections::BTreeSet<ApiResource>,
    pub(crate) pod_metrics_api_available: bool,
    pub(crate) node_metrics_api_available: bool,
}
#[derive(Debug)]
pub(crate) struct KubernetesCustomResourceColumnsLoaded {
    pub(crate) cluster_key: i32,
    pub(crate) columns: std::collections::BTreeMap<ApiResource, Vec<CustomResourceColumn>>,
}
#[derive(Debug)]
pub(crate) struct KubernetesResourceSchemasLoaded {
    pub(crate) cluster_key: i32,
    pub(crate) schemas: std::collections::BTreeMap<ApiResource, ResourceSchema>,
}
#[derive(Debug)]
pub(crate) struct KubernetesApisLoadFailed {
    pub(crate) cluster_key: i32,
    pub(crate) error: String,
}
#[derive(Debug)]
pub(crate) struct KubernetesClusterConnectionCreated {
    pub(crate) cluster_key: i32,
}
#[derive(Debug)]
pub(crate) struct KubernetesResourceAdded {
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) resource: MinimalResource,
}
#[derive(Debug)]
pub(crate) struct KubernetesResourceDeleted {
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) resource_uid: String,
}
#[derive(Debug)]
pub(crate) struct KubernetesResourcesReplaced {
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) resources: Vec<MinimalResource>,
}
#[derive(Debug)]
pub(crate) struct KubernetesResourceWatchStarted {
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
}
#[derive(Debug, Clone)]
pub(crate) struct KubernetesResourceWatchFailed {
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) error: String,
}
#[derive(Debug)]
pub(crate) struct HelmReleasesReplaced {
    pub(crate) cluster_key: i32,
    pub(crate) namespace: String,
    pub(crate) releases: Vec<HelmRelease>,
}
#[derive(Debug)]
pub(crate) struct HelmReleaseBackendFailed {
    pub(crate) cluster_key: i32,
    pub(crate) namespace: String,
    pub(crate) backend: &'static str,
    pub(crate) error: String,
}
#[derive(Debug)]
pub(crate) struct ResourceDetailUpdated {
    pub(crate) cluster_key: i32,
    pub(crate) history_entry_id: u64,
    pub(crate) detail: Box<ResourceDetail>,
}
#[derive(Debug)]
pub(crate) struct ResourceDetailDeleted {
    pub(crate) cluster_key: i32,
    pub(crate) history_entry_id: u64,
}
#[derive(Debug)]
pub(crate) struct ManagedResourcesReplaced {
    pub(crate) cluster_key: i32,
    pub(crate) history_entry_id: u64,
    pub(crate) resources: Vec<ManagedResource>,
}
#[derive(Debug)]
pub(crate) struct ManagedResourcesWatchFailed {
    pub(crate) cluster_key: i32,
    pub(crate) history_entry_id: u64,
    pub(crate) error: String,
}
#[derive(Debug)]
pub(crate) struct ResourceEventsReplaced {
    pub(crate) cluster_key: i32,
    pub(crate) history_entry_id: u64,
    pub(crate) events: Vec<ResourceEvent>,
}
#[derive(Debug)]
pub(crate) struct ResourceDetailWatchFailed {
    pub(crate) cluster_key: i32,
    pub(crate) history_entry_id: u64,
    pub(crate) events: bool,
    pub(crate) error: String,
}
#[derive(Debug)]
pub(crate) struct PodMetricsUpdated {
    pub(crate) cluster_key: i32,
    pub(crate) namespace: String,
    pub(crate) usages: BTreeMap<String, PodUsage>,
}
#[derive(Debug)]
pub(crate) struct PodMetricsWatchFailed {
    pub(crate) cluster_key: i32,
    pub(crate) namespace: String,
    pub(crate) error: String,
}
#[derive(Debug)]
pub(crate) struct PodMetricsApiUnavailable {
    pub(crate) cluster_key: i32,
}
#[derive(Debug)]
pub(crate) struct NodeMetricsUpdated {
    pub(crate) cluster_key: i32,
    pub(crate) usages: BTreeMap<String, NodeUsage>,
}
#[derive(Debug)]
pub(crate) struct NodeMetricsWatchFailed {
    pub(crate) cluster_key: i32,
    pub(crate) error: String,
}
#[derive(Debug)]
pub(crate) struct NodeMetricsApiUnavailable {
    pub(crate) cluster_key: i32,
}
#[derive(Debug)]
pub(crate) struct ResourceDetailPodUsageUpdated {
    pub(crate) cluster_key: i32,
    pub(crate) history_entry_id: u64,
    pub(crate) usage: PodUsage,
}
#[derive(Debug)]
pub(crate) struct ResourceDetailPodUsageFailed {
    pub(crate) cluster_key: i32,
    pub(crate) history_entry_id: u64,
    pub(crate) error: String,
}
#[derive(Debug)]
pub(crate) struct ResourceDetailPodUsageMissing {
    pub(crate) cluster_key: i32,
    pub(crate) history_entry_id: u64,
}
#[derive(Debug)]
pub(crate) struct ResourceDetailNodeUsageUpdated {
    pub(crate) cluster_key: i32,
    pub(crate) history_entry_id: u64,
    pub(crate) usage: NodeUsage,
}
#[derive(Debug)]
pub(crate) struct ResourceDetailNodeUsageFailed {
    pub(crate) cluster_key: i32,
    pub(crate) history_entry_id: u64,
    pub(crate) error: String,
}
#[derive(Debug)]
pub(crate) struct ResourceDetailNodeUsageMissing {
    pub(crate) cluster_key: i32,
    pub(crate) history_entry_id: u64,
}
#[derive(Debug)]
pub(crate) struct ResourceYamlFetched {
    pub(crate) editor_id: u64,
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) resource_name: String,
    pub(crate) yaml: String,
}
#[derive(Debug)]
pub(crate) struct ResourceSchemaLoaded {
    pub(crate) editor_id: u64,
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) schema: ResourceSchema,
}
#[derive(Debug)]
pub(crate) struct ResourceYamlValidated {
    pub(crate) editor_id: u64,
    pub(crate) revision: u64,
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) resource_name: String,
}
#[derive(Debug)]
pub(crate) struct ResourceYamlValidationFailed {
    pub(crate) editor_id: u64,
    pub(crate) revision: u64,
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) resource_name: String,
    pub(crate) error: ResourceApiError,
}
#[derive(Debug)]
pub(crate) struct ResourceDeleteCompleted {
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) resource_name: String,
    pub(crate) bulk_delete_id: Option<u64>,
}
#[derive(Debug)]
pub(crate) struct ResourceForceDeleteCompleted {
    pub(crate) cluster_key: i32,
    pub(crate) resource_name: String,
}
#[derive(Debug)]
pub(crate) struct ResourceApplyCompleted {
    pub(crate) editor_id: u64,
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) resource_name: String,
}
#[derive(Debug)]
pub(crate) struct ResourceApplyFailed {
    pub(crate) editor_id: u64,
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) resource_name: String,
    pub(crate) error: ResourceApiError,
}
#[derive(Debug)]
pub(crate) struct DeploymentRestartCompleted {
    pub(crate) namespace: String,
    pub(crate) resource_name: String,
}
#[derive(Debug)]
pub(crate) struct ResourceScaleFetched {
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) resource_name: String,
    pub(crate) replicas: i32,
}
#[derive(Debug)]
pub(crate) struct ResourceScaleUpdated {
    pub(crate) cluster_key: i32,
    pub(crate) resource_name: String,
}
#[derive(Debug)]
pub(crate) struct ResourceDataUpdateCompleted {
    pub(crate) cluster_key: i32,
    pub(crate) history_entry_id: u64,
    pub(crate) request_id: u64,
}
#[derive(Debug)]
pub(crate) struct PodLogStreamStarted {
    pub(crate) log_window_id: u64,
}
#[derive(Debug)]
pub(crate) struct PodLogStreamEnded {
    pub(crate) log_window_id: u64,
}
#[derive(Debug)]
pub(crate) struct PodLogStreamFailed {
    pub(crate) log_window_id: u64,
    pub(crate) error: String,
}

#[derive(Debug)]
pub(crate) struct WorkerError {
    pub(crate) error: Error,
}

#[derive(Debug)]
pub(crate) struct ClusterConnectionFailed {
    pub(crate) cluster_key: i32,
    pub(crate) error: String,
}
#[derive(Debug)]
pub(crate) struct ResourceYamlFetchFailed {
    pub(crate) editor_id: u64,
    pub(crate) error: String,
}
#[derive(Debug)]
pub(crate) struct ResourceSchemaLoadFailed {
    pub(crate) editor_id: u64,
    pub(crate) error: String,
}
#[derive(Debug)]
pub(crate) struct ResourceDeleteFailed {
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) resource_name: String,
    pub(crate) bulk_delete_id: Option<u64>,
    pub(crate) error: String,
}
#[derive(Debug)]
pub(crate) struct ResourceForceDeleteFailed {
    pub(crate) cluster_key: i32,
    pub(crate) error: String,
}
#[derive(Debug)]
pub(crate) struct DeploymentRestartFailed {
    pub(crate) cluster_key: i32,
    pub(crate) error: String,
}
#[derive(Debug)]
pub(crate) struct CronJobRunCompleted {
    pub(crate) namespace: String,
    pub(crate) cron_job_name: String,
    pub(crate) job_name: String,
}
#[derive(Debug)]
pub(crate) struct CronJobRunFailed {
    pub(crate) cluster_key: i32,
    pub(crate) error: String,
}
#[derive(Debug)]
pub(crate) struct ResourceScaleFailed {
    pub(crate) cluster_key: i32,
    pub(crate) error: String,
}
#[derive(Debug)]
pub(crate) struct ResourceYamlApplyCommandFailed {
    pub(crate) editor_id: u64,
    pub(crate) error: String,
}
#[derive(Debug)]
pub(crate) struct ResourceYamlValidationCommandFailed {
    pub(crate) editor_id: u64,
    pub(crate) revision: u64,
    pub(crate) error: String,
}

#[derive(Debug)]
pub(crate) enum ResourceYamlApplyFailure {
    Api(ResourceApplyFailed),
    Command(ResourceYamlApplyCommandFailed),
}

impl WorkerResult for ResourceYamlApplyFailure {
    fn apply(self, ui: &mut crate::ui::state::UiState, commands: &mut Vec<WorkerCommandBox>) {
        match self {
            Self::Api(failure) => failure.apply(ui, commands),
            Self::Command(failure) => failure.apply(ui, commands),
        }
    }
}

#[derive(Debug)]
pub(crate) enum ResourceYamlValidationFailure {
    Api(ResourceYamlValidationFailed),
    Command(ResourceYamlValidationCommandFailed),
}

impl WorkerResult for ResourceYamlValidationFailure {
    fn apply(self, ui: &mut crate::ui::state::UiState, commands: &mut Vec<WorkerCommandBox>) {
        match self {
            Self::Api(failure) => failure.apply(ui, commands),
            Self::Command(failure) => failure.apply(ui, commands),
        }
    }
}
#[derive(Debug)]
pub(crate) struct ResourceDataUpdateFailed {
    pub(crate) cluster_key: i32,
    pub(crate) history_entry_id: u64,
    pub(crate) request_id: u64,
    pub(crate) error: String,
}

impl WorkerResult for WorkerError {
    fn apply(self, _ui: &mut crate::ui::state::UiState, _commands: &mut Vec<WorkerCommandBox>) {
        tracing::error!(error = ?self.error, "Worker command failed");
    }
}
