use super::*;

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
                if let Some(resource) = scheduled_pod_from_typed(&resource) {
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
        });
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
