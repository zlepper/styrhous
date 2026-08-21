use super::*;

pub(crate) struct DynamicKubernetesResourceWatcher {
    pub(crate) client: kube::Client,
    pub(crate) event_sender: WorkerResultSender,
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) watched_namespaces: Option<BTreeSet<String>>,
    pub(crate) custom_columns: Vec<CustomResourceColumn>,
}

impl DynamicKubernetesResourceWatcher {
    async fn watch_resources(self, mut initialized: Option<oneshot::Sender<()>>) {
        let DynamicKubernetesResourceWatcher {
            client,
            event_sender,
            cluster_key,
            api_resource,
            namespace,
            watched_namespaces,
            custom_columns,
        } = self;
        // Convert our ApiResource to kube's ApiResource using discovery
        let group = if api_resource.group == "core" {
            ""
        } else {
            &api_resource.group
        };

        let gvk = GroupVersionKind::gvk(group, &api_resource.version, &api_resource.kind);

        let discovery_result = kube::discovery::pinned_kind(&client, &gvk).await;
        let (ar, caps) = match discovery_result {
            Ok(r) => r,
            Err(error) => {
                warn!(
                    "Failed to discover API resource {}/{}: {}",
                    api_resource.group, api_resource.name, error
                );
                event_sender
                    .send(KubernetesResourceWatchFailed {
                        cluster_key,
                        api_resource,
                        namespace,
                        error: format!("{error:#?}"),
                    })
                    .await
                    .log_if_error("Failed to send resource watcher discovery error");
                drop(initialized);
                return;
            }
        };

        let api: Api<DynamicObject> = match (caps.scope, namespace.as_deref()) {
            (kube::discovery::Scope::Namespaced, Some(namespace)) => {
                Api::namespaced_with(client, namespace, &ar)
            }
            (kube::discovery::Scope::Namespaced, None) if watched_namespaces.is_some() => {
                Api::all_with(client, &ar)
            }
            (kube::discovery::Scope::Cluster, None) => Api::all_with(client, &ar),
            (discovered_scope, requested_namespace) => {
                let error = format!(
                    "Resource scope mismatch: discovered {discovered_scope:?} scope with namespace {requested_namespace:?}"
                );
                event_sender
                    .send(KubernetesResourceWatchFailed {
                        cluster_key,
                        api_resource,
                        namespace,
                        error,
                    })
                    .await
                    .log_if_error("Failed to send resource watcher scope error");
                drop(initialized);
                return;
            }
        };

        run_resource_watch(
            watcher(api, watcher_config()),
            ResourceWatchContext {
                event_sender,
                cluster_key,
                api_resource,
                scope: ResourceWatchScope::from_parts(namespace, watched_namespaces),
            },
            initialized.take(),
            |item| extract_minimal_resource(item, &custom_columns),
            |item| item.metadata.namespace.as_deref(),
            get_resource_uid,
        )
        .await;
    }
}

impl ResourceWatcher for DynamicKubernetesResourceWatcher {
    fn watch_resources(
        self: Box<Self>,
        initialized: Option<oneshot::Sender<()>>,
    ) -> ResourceWatcherFuture {
        Box::pin(async move { (*self).watch_resources(initialized).await })
    }
}

pub(crate) struct TypedKubernetesResourceWatcher<T> {
    client: kube::Client,
    event_sender: WorkerResultSender,
    cluster_key: i32,
    api_resource: ApiResource,
    namespace: Option<String>,
    watched_namespaces: Option<BTreeSet<String>>,
    extract: fn(&T) -> MinimalResource,
}

impl<T> TypedKubernetesResourceWatcher<T>
where
    T: Resource<DynamicType = (), Scope = NamespaceResourceScope>
        + Clone
        + Debug
        + Send
        + Sync
        + for<'de> k8s_openapi::serde::Deserialize<'de>
        + 'static,
{
    async fn watch_resources(self, mut initialized: Option<oneshot::Sender<()>>) {
        let TypedKubernetesResourceWatcher {
            client,
            event_sender,
            cluster_key,
            api_resource,
            namespace,
            watched_namespaces,
            extract,
        } = self;
        let api = match (namespace.as_deref(), watched_namespaces.as_ref()) {
            (Some(namespace), _) => Api::<T>::namespaced(client, namespace),
            (None, Some(_)) => Api::<T>::all(client),
            (None, None) => {
                event_sender
                    .send(KubernetesResourceWatchFailed {
                        cluster_key,
                        api_resource,
                        namespace: None,
                        error: "A namespaced typed watcher was started without a namespace"
                            .to_owned(),
                    })
                    .await
                    .log_if_error("Failed to send resource watcher scope error");
                drop(initialized);
                return;
            }
        };
        run_resource_watch(
            watcher(api, watcher_config()),
            ResourceWatchContext {
                event_sender,
                cluster_key,
                api_resource,
                scope: ResourceWatchScope::from_parts(namespace, watched_namespaces),
            },
            initialized.take(),
            extract,
            |item| item.meta().namespace.as_deref(),
            get_resource_uid,
        )
        .await;
    }
}

impl<T> ResourceWatcher for TypedKubernetesResourceWatcher<T>
where
    T: Resource<DynamicType = (), Scope = NamespaceResourceScope>
        + Clone
        + Debug
        + Send
        + Sync
        + for<'de> k8s_openapi::serde::Deserialize<'de>
        + 'static,
{
    fn watch_resources(
        self: Box<Self>,
        initialized: Option<oneshot::Sender<()>>,
    ) -> ResourceWatcherFuture {
        Box::pin(async move { (*self).watch_resources(initialized).await })
    }
}

pub(crate) fn namespaced_typed_watcher<T>(
    context: TypedWatcherContext,
    extract: fn(&T) -> MinimalResource,
) -> Box<dyn ResourceWatcher>
where
    T: Resource<DynamicType = (), Scope = NamespaceResourceScope>
        + Clone
        + Debug
        + Send
        + Sync
        + for<'de> k8s_openapi::serde::Deserialize<'de>
        + 'static,
{
    Box::new(TypedKubernetesResourceWatcher {
        client: context.client,
        event_sender: context.event_sender,
        cluster_key: context.cluster_key,
        api_resource: context.api_resource,
        namespace: context.namespace,
        watched_namespaces: context.watched_namespaces,
        extract,
    })
}

pub(crate) struct ClusterTypedKubernetesResourceWatcher<T> {
    client: kube::Client,
    event_sender: WorkerResultSender,
    cluster_key: i32,
    api_resource: ApiResource,
    namespace: Option<String>,
    extract: fn(&T) -> MinimalResource,
}

impl<T> ClusterTypedKubernetesResourceWatcher<T>
where
    T: Resource<DynamicType = (), Scope = ClusterResourceScope>
        + Clone
        + Debug
        + Send
        + Sync
        + for<'de> k8s_openapi::serde::Deserialize<'de>
        + 'static,
{
    async fn watch_resources(self, mut initialized: Option<oneshot::Sender<()>>) {
        let ClusterTypedKubernetesResourceWatcher {
            client,
            event_sender,
            cluster_key,
            api_resource,
            namespace,
            extract,
        } = self;
        if namespace.is_some() {
            event_sender
                .send(KubernetesResourceWatchFailed {
                    cluster_key,
                    api_resource,
                    namespace,
                    error: "A cluster-scoped typed watcher was started with a namespace".to_owned(),
                })
                .await
                .log_if_error("Failed to send resource watcher scope error");
            drop(initialized);
            return;
        }

        run_resource_watch(
            watcher(Api::<T>::all(client), watcher_config()),
            ResourceWatchContext {
                event_sender,
                cluster_key,
                api_resource,
                scope: ResourceWatchScope::Single(None),
            },
            initialized.take(),
            extract,
            |item| item.meta().namespace.as_deref(),
            get_resource_uid,
        )
        .await;
    }
}

impl<T> ResourceWatcher for ClusterTypedKubernetesResourceWatcher<T>
where
    T: Resource<DynamicType = (), Scope = ClusterResourceScope>
        + Clone
        + Debug
        + Send
        + Sync
        + for<'de> k8s_openapi::serde::Deserialize<'de>
        + 'static,
{
    fn watch_resources(
        self: Box<Self>,
        initialized: Option<oneshot::Sender<()>>,
    ) -> ResourceWatcherFuture {
        Box::pin(async move { (*self).watch_resources(initialized).await })
    }
}

pub(crate) fn cluster_typed_watcher<T>(
    context: TypedWatcherContext,
    extract: fn(&T) -> MinimalResource,
) -> Box<dyn ResourceWatcher>
where
    T: Resource<DynamicType = (), Scope = ClusterResourceScope>
        + Clone
        + Debug
        + Send
        + Sync
        + for<'de> k8s_openapi::serde::Deserialize<'de>
        + 'static,
{
    Box::new(ClusterTypedKubernetesResourceWatcher {
        client: context.client,
        event_sender: context.event_sender,
        cluster_key: context.cluster_key,
        api_resource: context.api_resource,
        namespace: context.namespace,
        extract,
    })
}
