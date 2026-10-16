use super::*;
use crate::pod_metrics::{MissingResourceQuantity, resource_thresholds};
use crate::resource_detail::{NodePodResources, PodResourceThresholds};
use k8s_openapi::api::core::v1::Container;

pub(crate) struct ManagedResourceWatchRequest {
    pub(crate) cluster_key: i32,
    pub(crate) client: kube::Client,
    pub(crate) root_api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) root_name: String,
    pub(crate) root_uid: String,
    pub(crate) history_entry_id: u64,
    pub(crate) event_sender: WorkerResultSender,
}

/// Watch the small, well-known set of resource kinds which can make up a
/// built-in workload controller hierarchy. Kubernetes has no generic reverse
/// owner-reference query, so this deliberately does not attempt custom types.
pub(crate) async fn watch_managed_resources(request: ManagedResourceWatchRequest) {
    let ManagedResourceWatchRequest {
        cluster_key,
        client,
        root_api_resource,
        namespace,
        root_name,
        root_uid,
        history_entry_id,
        event_sender,
    } = request;
    let resource_types = managed_resource_types(&root_api_resource);
    if resource_types.is_empty() {
        event_sender
            .send(ManagedResourcesReplaced {
                cluster_key,
                history_entry_id,
                resources: Vec::new(),
                node_pod_resources: None,
            })
            .await
            .log_if_error("Failed to send empty managed resources");
        return;
    }

    let (updates_sender, mut updates_receiver) = tokio::sync::mpsc::unbounded_channel();
    let mut tasks = JoinSet::new();
    for resource_type in resource_types {
        let client = client.clone();
        let namespace = namespace.clone();
        let root_name = root_name.clone();
        let updates_sender = updates_sender.clone();
        tasks.spawn(async move {
            match resource_type {
                ManagedResourceType::ReplicaSet => {
                    if let Some(namespace) = namespace {
                        watch_managed_type::<ReplicaSet>(
                            client,
                            namespace,
                            updates_sender,
                            resource_handlers::replica_set::extract,
                        )
                        .await
                    }
                }
                ManagedResourceType::Job => {
                    if let Some(namespace) = namespace {
                        watch_managed_type::<Job>(
                            client,
                            namespace,
                            updates_sender,
                            resource_handlers::job::extract,
                        )
                        .await
                    }
                }
                ManagedResourceType::Pod => {
                    if let Some(namespace) = namespace {
                        watch_managed_type::<Pod>(
                            client,
                            namespace,
                            updates_sender,
                            resource_handlers::pod::extract,
                        )
                        .await
                    }
                }
                ManagedResourceType::PodOnNode => {
                    watch_pods_on_node(client, root_name, updates_sender).await
                }
            }
        });
    }
    drop(updates_sender);

    let mut by_type = BTreeMap::<ApiResource, Vec<ManagedResource>>::new();
    while let Some(update) = updates_receiver.recv().await {
        match update {
            ManagedResourceUpdate::Replaced {
                api_resource,
                resources,
                node_pod_resources,
            } => {
                by_type.insert(api_resource.clone(), resources);
                let resources = by_type
                    .values()
                    .flatten()
                    .filter(|resource| {
                        if root_api_resource.kind == "Node" {
                            belongs_to_node(resource, &root_name)
                        } else {
                            belongs_to_workload_tree(
                                resource,
                                &root_uid,
                                &root_api_resource,
                                &by_type,
                            )
                        }
                    })
                    .cloned()
                    .collect();
                event_sender
                    .send(ManagedResourcesReplaced {
                        cluster_key,
                        history_entry_id,
                        resources,
                        node_pod_resources,
                    })
                    .await
                    .log_if_error("Failed to send managed resource update");
            }
            ManagedResourceUpdate::Failed {
                api_resource,
                error,
            } => event_sender
                .send(ManagedResourcesWatchFailed {
                    cluster_key,
                    history_entry_id,
                    error: format!("Unable to watch {}: {error}", api_resource.display_name()),
                })
                .await
                .log_if_error("Failed to send managed resource watch failure"),
        }
    }
    while tasks.join_next().await.is_some() {}
}

#[derive(Clone, Copy)]
pub(crate) enum ManagedResourceType {
    ReplicaSet,
    Job,
    Pod,
    PodOnNode,
}

pub(crate) fn managed_resource_types(api_resource: &ApiResource) -> Vec<ManagedResourceType> {
    match (api_resource.group.as_str(), api_resource.kind.as_str()) {
        ("apps", "Deployment") => vec![ManagedResourceType::ReplicaSet, ManagedResourceType::Pod],
        ("batch", "CronJob") => vec![ManagedResourceType::Job, ManagedResourceType::Pod],
        ("apps", "ReplicaSet")
        | ("apps", "StatefulSet")
        | ("apps", "DaemonSet")
        | ("core", "ReplicationController")
        | ("batch", "Job") => vec![ManagedResourceType::Pod],
        ("core", "Node") => vec![ManagedResourceType::PodOnNode],
        _ => Vec::new(),
    }
}

pub(crate) enum ManagedResourceUpdate {
    Replaced {
        api_resource: ApiResource,
        resources: Vec<ManagedResource>,
        node_pod_resources: Option<NodePodResources>,
    },
    Failed {
        api_resource: ApiResource,
        error: String,
    },
}

pub(crate) async fn watch_managed_type<T>(
    client: kube::Client,
    namespace: String,
    sender: tokio::sync::mpsc::UnboundedSender<ManagedResourceUpdate>,
    extract: fn(&T) -> MinimalResource,
) where
    T: Resource<DynamicType = (), Scope = NamespaceResourceScope>
        + Clone
        + k8s_openapi::serde::de::DeserializeOwned
        + std::fmt::Debug
        + Send
        + 'static,
{
    let api_resource = api_resource_for::<T>();
    let api = Api::<T>::namespaced(client, &namespace);
    let stream = watcher(api, watcher_config());
    pin_mut!(stream);
    let mut resources = BTreeMap::new();
    while let Some(event) = stream.next().await {
        let event = match event {
            Ok(event) => event,
            Err(error) => {
                let _ = sender.send(ManagedResourceUpdate::Failed {
                    api_resource,
                    error: format!("{error:#?}"),
                });
                return;
            }
        };
        match event {
            Event::Init => resources.clear(),
            Event::InitApply(resource) | Event::Apply(resource) => {
                if let Some(resource) = managed_resource_from_typed(&resource, extract) {
                    resources.insert(resource.uid.clone(), resource);
                }
            }
            Event::Delete(resource) => {
                resources.remove(&get_resource_uid(&resource));
            }
            Event::InitDone => {}
        }
        let _ = sender.send(ManagedResourceUpdate::Replaced {
            api_resource: api_resource.clone(),
            resources: resources.values().cloned().collect(),
            node_pod_resources: None,
        });
    }
}

pub(crate) async fn watch_pods_on_node(
    client: kube::Client,
    node_name: String,
    sender: tokio::sync::mpsc::UnboundedSender<ManagedResourceUpdate>,
) {
    let api_resource = api_resource_for::<Pod>();
    let api = Api::<Pod>::all(client);
    let config = watcher_config().fields(&format!("spec.nodeName={node_name}"));
    let stream = watcher(api, config);
    pin_mut!(stream);
    let mut state = NodePodWatchState::new(node_name);
    while let Some(event) = stream.next().await {
        let event = match event {
            Ok(event) => event,
            Err(error) => {
                let _ = sender.send(ManagedResourceUpdate::Failed {
                    api_resource,
                    error: format!("{error:#?}"),
                });
                return;
            }
        };
        state.apply(event);
        let _ = sender.send(ManagedResourceUpdate::Replaced {
            api_resource: api_resource.clone(),
            resources: state.managed_resources(),
            node_pod_resources: state.totals(),
        });
    }
}

struct NodePodWatchState {
    node_name: String,
    pods: BTreeMap<String, Pod>,
    initialized: bool,
}

impl NodePodWatchState {
    fn new(node_name: String) -> Self {
        Self {
            node_name,
            pods: BTreeMap::new(),
            initialized: false,
        }
    }

    fn apply(&mut self, event: Event<Pod>) {
        match event {
            Event::Init => {
                self.pods.clear();
                self.initialized = false;
            }
            Event::InitApply(resource) | Event::Apply(resource) => {
                let uid = get_resource_uid(&resource);
                if resource
                    .spec
                    .as_ref()
                    .and_then(|spec| spec.node_name.as_deref())
                    == Some(self.node_name.as_str())
                {
                    self.pods.insert(uid, resource);
                } else {
                    self.pods.remove(&uid);
                }
            }
            Event::Delete(resource) => {
                self.pods.remove(&get_resource_uid(&resource));
            }
            Event::InitDone => self.initialized = true,
        }
    }

    fn managed_resources(&self) -> Vec<ManagedResource> {
        self.pods
            .values()
            .filter_map(scheduled_pod_from_typed)
            .collect()
    }

    fn totals(&self) -> Option<NodePodResources> {
        self.initialized
            .then(|| total_node_pod_resources(self.pods.values()))
    }
}

fn total_node_pod_resources<'a>(pods: impl Iterator<Item = &'a Pod>) -> NodePodResources {
    let mut total = NodePodResources {
        requests: zero_thresholds(),
        configured_limits: zero_thresholds(),
        pod_count: 0,
        pods_without_cpu_limit: 0,
        pods_without_memory_limit: 0,
    };
    for pod in pods {
        if matches!(
            pod.status
                .as_ref()
                .and_then(|status| status.phase.as_deref()),
            Some("Succeeded" | "Failed")
        ) {
            continue;
        }
        let Some(spec) = pod.spec.as_ref() else {
            continue;
        };
        let mut app = PodResourcePair::zero();
        let mut sidecars = PodResourcePair::zero();
        let mut init_peak = PodResourcePair::zero();
        let mut missing_cpu_limit = false;
        let mut missing_memory_limit = false;

        for container in &spec.containers {
            let (resources, missing_cpu, missing_memory) = container_resources(container);
            app = app.checked_add(resources);
            missing_cpu_limit |= missing_cpu;
            missing_memory_limit |= missing_memory;
        }
        for container in spec.init_containers.as_deref().unwrap_or_default() {
            let (resources, missing_cpu, missing_memory) = container_resources(container);
            missing_cpu_limit |= missing_cpu;
            missing_memory_limit |= missing_memory;
            if container.restart_policy.as_deref() == Some("Always") {
                sidecars = sidecars.checked_add(resources);
                init_peak = init_peak.max(sidecars);
            } else {
                init_peak = init_peak.max(sidecars.checked_add(resources));
            }
        }
        let overhead = resource_thresholds(spec.overhead.as_ref(), MissingResourceQuantity::Zero);
        let resources = app
            .checked_add(sidecars)
            .max(init_peak)
            .checked_add(PodResourcePair {
                requests: overhead,
                limits: overhead,
            });
        total.requests = checked_add_thresholds(total.requests, resources.requests);
        total.configured_limits = checked_add_thresholds(total.configured_limits, resources.limits);
        total.pod_count += 1;
        total.pods_without_cpu_limit += usize::from(missing_cpu_limit);
        total.pods_without_memory_limit += usize::from(missing_memory_limit);
    }
    total
}

fn container_resources(container: &Container) -> (PodResourcePair, bool, bool) {
    let requests = container
        .resources
        .as_ref()
        .and_then(|resources| resources.requests.as_ref());
    let limits = container
        .resources
        .as_ref()
        .and_then(|resources| resources.limits.as_ref());
    let missing_cpu_limit = limits.is_none_or(|limits| !limits.contains_key("cpu"));
    let missing_memory_limit = limits.is_none_or(|limits| !limits.contains_key("memory"));
    (
        PodResourcePair {
            requests: resource_thresholds(requests, MissingResourceQuantity::Zero),
            limits: resource_thresholds(limits, MissingResourceQuantity::Zero),
        },
        missing_cpu_limit,
        missing_memory_limit,
    )
}

#[derive(Clone, Copy)]
struct PodResourcePair {
    requests: PodResourceThresholds,
    limits: PodResourceThresholds,
}

impl PodResourcePair {
    fn zero() -> Self {
        Self {
            requests: zero_thresholds(),
            limits: zero_thresholds(),
        }
    }

    fn checked_add(self, other: Self) -> Self {
        Self {
            requests: checked_add_thresholds(self.requests, other.requests),
            limits: checked_add_thresholds(self.limits, other.limits),
        }
    }

    fn max(self, other: Self) -> Self {
        Self {
            requests: max_thresholds(self.requests, other.requests),
            limits: max_thresholds(self.limits, other.limits),
        }
    }
}

fn zero_thresholds() -> PodResourceThresholds {
    PodResourceThresholds {
        cpu_nanocores: Some(0),
        memory_bytes: Some(0),
    }
}

fn checked_add_thresholds(
    left: PodResourceThresholds,
    right: PodResourceThresholds,
) -> PodResourceThresholds {
    PodResourceThresholds {
        cpu_nanocores: left
            .cpu_nanocores
            .zip(right.cpu_nanocores)
            .and_then(|(left, right)| left.checked_add(right)),
        memory_bytes: left
            .memory_bytes
            .zip(right.memory_bytes)
            .and_then(|(left, right)| left.checked_add(right)),
    }
}

fn max_thresholds(
    left: PodResourceThresholds,
    right: PodResourceThresholds,
) -> PodResourceThresholds {
    PodResourceThresholds {
        cpu_nanocores: left
            .cpu_nanocores
            .zip(right.cpu_nanocores)
            .map(|(left, right)| left.max(right)),
        memory_bytes: left
            .memory_bytes
            .zip(right.memory_bytes)
            .map(|(left, right)| left.max(right)),
    }
}

pub(crate) trait ApiResourceScope {
    const NAMESPACED: bool;
}

impl ApiResourceScope for NamespaceResourceScope {
    const NAMESPACED: bool = true;
}

impl ApiResourceScope for ClusterResourceScope {
    const NAMESPACED: bool = false;
}

pub(crate) fn api_resource_for<T>() -> ApiResource
where
    T: Resource<DynamicType = ()>,
    T::Scope: ApiResourceScope,
{
    let group = T::group(&());
    ApiResource {
        group: if group.is_empty() {
            "core".into()
        } else {
            group.into_owned()
        },
        version: T::version(&()).into_owned(),
        kind: T::kind(&()).into_owned(),
        name: T::plural(&()).into_owned(),
        namespaced: T::Scope::NAMESPACED,
    }
}

pub(crate) fn managed_resource_from_typed<T>(
    resource: &T,
    extract: impl FnOnce(&T) -> MinimalResource,
) -> Option<ManagedResource>
where
    T: Resource<DynamicType = (), Scope = NamespaceResourceScope>,
{
    let metadata = resource.meta();
    let controller_owner_uid = metadata
        .owner_references
        .as_ref()?
        .iter()
        .find(|owner| owner.controller == Some(true))?
        .uid
        .clone();
    let minimal_resource = extract(resource);
    Some(ManagedResource {
        api_resource: api_resource_for::<T>(),
        name: minimal_resource.name,
        namespace: minimal_resource.namespace,
        uid: minimal_resource.uid,
        association: ManagedResourceAssociation::ControllerOwnerUid(controller_owner_uid),
        creation_timestamp: minimal_resource.creation_timestamp,
        cells: minimal_resource.cells,
    })
}

pub(crate) fn scheduled_pod_from_typed(resource: &Pod) -> Option<ManagedResource> {
    let node_name = resource.spec.as_ref()?.node_name.clone()?;
    let minimal_resource = resource_handlers::pod::extract(resource);
    Some(ManagedResource {
        api_resource: api_resource_for::<Pod>(),
        name: minimal_resource.name,
        namespace: minimal_resource.namespace,
        uid: minimal_resource.uid,
        association: ManagedResourceAssociation::NodeName(node_name),
        creation_timestamp: minimal_resource.creation_timestamp,
        cells: minimal_resource.cells,
    })
}

pub(crate) fn belongs_to_workload_tree(
    resource: &ManagedResource,
    root_uid: &str,
    root_api_resource: &ApiResource,
    all_resources: &BTreeMap<ApiResource, Vec<ManagedResource>>,
) -> bool {
    if matches!(
        &resource.association,
        ManagedResourceAssociation::ControllerOwnerUid(owner_uid) if owner_uid == root_uid
    ) {
        return is_managed_workload_child(root_api_resource, &resource.api_resource);
    }
    all_resources.values().flatten().any(|parent| {
        matches!(
            &resource.association,
            ManagedResourceAssociation::ControllerOwnerUid(owner_uid) if parent.uid == *owner_uid
        ) && is_managed_workload_child(&parent.api_resource, &resource.api_resource)
            && belongs_to_workload_tree(parent, root_uid, root_api_resource, all_resources)
    })
}

pub(crate) fn belongs_to_node(resource: &ManagedResource, node_name: &str) -> bool {
    matches!(
        &resource.association,
        ManagedResourceAssociation::NodeName(assigned_node) if assigned_node == node_name
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::api::core::v1::{PodStatus, ResourceRequirements};
    use k8s_openapi::apimachinery::pkg::api::resource::Quantity;

    fn container(
        name: &str,
        requests: &[(&str, &str)],
        limits: &[(&str, &str)],
        restartable: bool,
    ) -> Container {
        let quantities = |entries: &[(&str, &str)]| {
            entries
                .iter()
                .map(|(name, value)| ((*name).to_owned(), Quantity((*value).to_owned())))
                .collect()
        };
        Container {
            name: name.to_owned(),
            resources: Some(ResourceRequirements {
                requests: Some(quantities(requests)),
                limits: Some(quantities(limits)),
                ..Default::default()
            }),
            restart_policy: restartable.then(|| "Always".to_owned()),
            ..Default::default()
        }
    }

    fn pod(containers: Vec<Container>, init_containers: Vec<Container>, phase: &str) -> Pod {
        Pod {
            spec: Some(k8s_openapi::api::core::v1::PodSpec {
                node_name: Some("node-a".to_owned()),
                containers,
                init_containers: Some(init_containers),
                ..Default::default()
            }),
            status: Some(PodStatus {
                phase: Some(phase.to_owned()),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn node_pod_totals_use_init_peaks_restartable_sidecars_and_overhead() {
        let mut scheduled = pod(
            vec![
                container(
                    "app",
                    &[("cpu", "200m"), ("memory", "100Mi")],
                    &[("cpu", "400m"), ("memory", "200Mi")],
                    false,
                ),
                container(
                    "helper",
                    &[("cpu", "300m"), ("memory", "100Mi")],
                    &[("cpu", "600m"), ("memory", "200Mi")],
                    false,
                ),
            ],
            vec![
                container(
                    "sidecar-a",
                    &[("cpu", "100m"), ("memory", "64Mi")],
                    &[("cpu", "200m"), ("memory", "128Mi")],
                    true,
                ),
                container(
                    "setup-a",
                    &[("cpu", "800m"), ("memory", "50Mi")],
                    &[("cpu", "900m"), ("memory", "100Mi")],
                    false,
                ),
                container(
                    "sidecar-b",
                    &[("cpu", "200m"), ("memory", "32Mi")],
                    &[("cpu", "300m"), ("memory", "64Mi")],
                    true,
                ),
                container(
                    "setup-b",
                    &[("cpu", "700m"), ("memory", "150Mi")],
                    &[("cpu", "1"), ("memory", "300Mi")],
                    false,
                ),
            ],
            "Running",
        );
        scheduled.spec.as_mut().unwrap().overhead = Some(BTreeMap::from([
            ("cpu".to_owned(), Quantity("50m".to_owned())),
            ("memory".to_owned(), Quantity("4Mi".to_owned())),
        ]));

        let totals = total_node_pod_resources([&scheduled].into_iter());

        assert_eq!(totals.pod_count, 1);
        assert_eq!(totals.requests.cpu_nanocores, Some(1_050_000_000));
        assert_eq!(totals.requests.memory_bytes, Some(300 * 1024 * 1024));
        assert_eq!(totals.configured_limits.cpu_nanocores, Some(1_550_000_000));
        assert_eq!(
            totals.configured_limits.memory_bytes,
            Some(596 * 1024 * 1024)
        );
        assert_eq!(totals.pods_without_cpu_limit, 0);
        assert_eq!(totals.pods_without_memory_limit, 0);
    }

    #[test]
    fn node_pod_totals_sum_defined_limits_and_ignore_terminal_pods() {
        let bounded = pod(
            vec![container(
                "bounded",
                &[("cpu", "250m")],
                &[("cpu", "1"), ("memory", "512Mi")],
                false,
            )],
            Vec::new(),
            "Running",
        );
        let partially_unbounded = pod(
            vec![container("unbounded", &[], &[("memory", "256Mi")], false)],
            Vec::new(),
            "Pending",
        );
        let completed = pod(
            vec![container(
                "completed",
                &[("cpu", "2")],
                &[("cpu", "3")],
                false,
            )],
            Vec::new(),
            "Succeeded",
        );

        let totals =
            total_node_pod_resources([&bounded, &partially_unbounded, &completed].into_iter());
        assert_eq!(totals.pod_count, 2);
        assert_eq!(totals.requests.cpu_nanocores, Some(250_000_000));
        assert_eq!(totals.requests.memory_bytes, Some(0));
        assert_eq!(totals.configured_limits.cpu_nanocores, Some(1_000_000_000));
        assert_eq!(
            totals.configured_limits.memory_bytes,
            Some(768 * 1024 * 1024)
        );
        assert_eq!(totals.pods_without_cpu_limit, 1);
        assert_eq!(totals.pods_without_memory_limit, 0);

        let empty = total_node_pod_resources([].into_iter());
        assert_eq!(empty.pod_count, 0);
        assert_eq!(empty.requests.cpu_nanocores, Some(0));
        assert_eq!(empty.configured_limits.memory_bytes, Some(0));
    }

    #[test]
    fn node_pod_totals_mark_invalid_or_overflowed_quantities_unavailable() {
        let invalid = pod(
            vec![container("invalid", &[("cpu", "nonsense")], &[], false)],
            Vec::new(),
            "Running",
        );
        let large = pod(
            vec![container("large", &[("cpu", "5000000000")], &[], false)],
            Vec::new(),
            "Running",
        );
        assert_eq!(
            total_node_pod_resources([&invalid].into_iter())
                .requests
                .cpu_nanocores,
            None
        );
        assert_eq!(
            total_node_pod_resources([&large, &large].into_iter())
                .requests
                .cpu_nanocores,
            None
        );
    }

    #[test]
    fn node_pod_watch_updates_totals_atomically_after_initial_sync() {
        let mut state = NodePodWatchState::new("node-a".to_owned());
        let mut initial = pod(
            vec![container("app", &[("cpu", "100m")], &[], false)],
            Vec::new(),
            "Running",
        );
        initial.metadata.name = Some("app-pod".to_owned());
        initial.metadata.namespace = Some("default".to_owned());
        initial.metadata.uid = Some("app-pod-uid".to_owned());

        state.apply(Event::Init);
        state.apply(Event::InitApply(initial.clone()));
        assert!(state.totals().is_none());
        assert_eq!(state.managed_resources().len(), 1);

        state.apply(Event::InitDone);
        assert_eq!(
            state.totals().unwrap().requests.cpu_nanocores,
            Some(100_000_000)
        );
        let mut updated = initial.clone();
        updated.spec.as_mut().unwrap().containers[0]
            .resources
            .as_mut()
            .unwrap()
            .requests
            .as_mut()
            .unwrap()
            .insert("cpu".to_owned(), Quantity("500m".to_owned()));
        state.apply(Event::Apply(updated));
        assert_eq!(
            state.totals().unwrap().requests.cpu_nanocores,
            Some(500_000_000)
        );

        let mut moved = initial.clone();
        moved.spec.as_mut().unwrap().node_name = Some("node-b".to_owned());
        state.apply(Event::Apply(moved));
        assert_eq!(state.totals().unwrap().pod_count, 0);
        assert!(state.managed_resources().is_empty());

        state.apply(Event::Apply(initial.clone()));
        state.apply(Event::Delete(initial));
        assert_eq!(state.totals().unwrap().requests.cpu_nanocores, Some(0));
        state.apply(Event::Init);
        assert!(state.totals().is_none());
    }
}
