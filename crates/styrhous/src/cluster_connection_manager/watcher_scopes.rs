use super::*;

#[derive(Clone)]
pub(crate) enum ResourceWatchScope {
    Single(Option<String>),
    AllNamespaces(BTreeSet<String>),
}

pub(crate) struct ResourceWatchContext {
    pub(crate) event_sender: WorkerResultSender,
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) scope: ResourceWatchScope,
}

impl ResourceWatchScope {
    pub(crate) fn from_parts(
        namespace: Option<String>,
        watched_namespaces: Option<BTreeSet<String>>,
    ) -> Self {
        watched_namespaces.map_or(Self::Single(namespace), Self::AllNamespaces)
    }

    pub(crate) fn source_namespace(&self) -> Option<String> {
        match self {
            Self::Single(namespace) => namespace.clone(),
            Self::AllNamespaces(_) => None,
        }
    }

    pub(crate) fn event_namespace(
        &self,
        resource_namespace: Option<&str>,
    ) -> Option<Option<String>> {
        match self {
            Self::Single(namespace) => Some(namespace.clone()),
            Self::AllNamespaces(namespaces) => resource_namespace
                .filter(|namespace| namespaces.contains(*namespace))
                .map(|namespace| Some(namespace.to_owned())),
        }
    }

    async fn send_initial_replacements(
        &self,
        event_sender: &WorkerResultSender,
        cluster_key: i32,
        api_resource: &ApiResource,
        resources: Vec<MinimalResource>,
    ) {
        match self {
            Self::Single(namespace) => {
                event_sender
                    .send(KubernetesResourcesReplaced {
                        cluster_key,
                        api_resource: api_resource.clone(),
                        namespace: namespace.clone(),
                        resources,
                    })
                    .await
                    .log_if_error("Failed to send resources replaced");
            }
            Self::AllNamespaces(namespaces) => {
                for namespace in namespaces {
                    let resources = resources
                        .iter()
                        .filter(|resource| resource.namespace.as_deref() == Some(namespace))
                        .cloned()
                        .collect();
                    event_sender
                        .send(KubernetesResourcesReplaced {
                            cluster_key,
                            api_resource: api_resource.clone(),
                            namespace: Some(namespace.clone()),
                            resources,
                        })
                        .await
                        .log_if_error("Failed to send resources replaced");
                }
            }
        }
    }
}

/// Execute the common list/watch lifecycle for dynamic and typed resources.
/// Constructors remain responsible for selecting the Kubernetes API and
/// validating its scope; this runner owns filtering, buffering, and UI events.
pub(crate) async fn run_resource_watch<T, E, S>(
    stream: S,
    context: ResourceWatchContext,
    mut initialized: Option<oneshot::Sender<()>>,
    extract: impl Fn(&T) -> MinimalResource,
    resource_namespace: impl for<'a> Fn(&'a T) -> Option<&'a str>,
    resource_uid: impl Fn(&T) -> String,
) where
    S: futures_util::Stream<Item = std::result::Result<Event<T>, E>>,
    E: Debug,
{
    let mut buffer = Vec::<MinimalResource>::new();
    pin_mut!(stream);

    while let Some(event) = stream.next().await {
        let event = match event {
            Ok(event) => event,
            Err(error) => {
                warn!("Resource watcher error: {error:?}");
                context
                    .event_sender
                    .send(KubernetesResourceWatchFailed {
                        cluster_key: context.cluster_key,
                        api_resource: context.api_resource.clone(),
                        namespace: context.scope.source_namespace(),
                        error: format!("{error:#?}"),
                    })
                    .await
                    .log_if_error("Failed to send resource watcher error");
                return;
            }
        };

        match event {
            Event::Apply(item) => {
                let resource = extract(&item);
                let Some(namespace) = context.scope.event_namespace(resource.namespace.as_deref())
                else {
                    continue;
                };
                context
                    .event_sender
                    .send(KubernetesResourceAdded {
                        cluster_key: context.cluster_key,
                        api_resource: context.api_resource.clone(),
                        namespace,
                        resource,
                    })
                    .await
                    .log_if_error("Failed to send resource added");
            }
            Event::Delete(item) => {
                let Some(namespace) = context.scope.event_namespace(resource_namespace(&item))
                else {
                    continue;
                };
                context
                    .event_sender
                    .send(KubernetesResourceDeleted {
                        cluster_key: context.cluster_key,
                        api_resource: context.api_resource.clone(),
                        namespace,
                        resource_uid: resource_uid(&item),
                    })
                    .await
                    .log_if_error("Failed to send resource deleted");
            }
            Event::Init => buffer.clear(),
            Event::InitApply(item) => buffer.push(extract(&item)),
            Event::InitDone => {
                context
                    .scope
                    .send_initial_replacements(
                        &context.event_sender,
                        context.cluster_key,
                        &context.api_resource,
                        std::mem::take(&mut buffer),
                    )
                    .await;
                let _ = initialized.take();
            }
        }
    }
}
