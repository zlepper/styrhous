use super::*;

pub(crate) struct DynamicKubernetesResourceWatcher {
    pub(crate) client: kube::Client,
    pub(crate) event_sender: WorkerResultSender,
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) watched_namespaces: Option<BTreeSet<String>>,
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
        } = self;
        let kube_resource = dynamic_api::kube_api_resource(&api_resource);
        let api: Api<DynamicObject> = match (api_resource.namespaced, namespace.as_deref()) {
            (true, Some(namespace)) => {
                Api::namespaced_with(client.clone(), namespace, &kube_resource)
            }
            (true, None) if watched_namespaces.is_some() => {
                Api::all_with(client.clone(), &kube_resource)
            }
            (false, None) => Api::all_with(client.clone(), &kube_resource),
            (namespaced, requested_namespace) => {
                let error = format!(
                    "Resource scope mismatch: {} is {} but the watch has namespace {requested_namespace:?}",
                    api_resource.kind,
                    if namespaced {
                        "namespaced"
                    } else {
                        "cluster-scoped"
                    },
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

        // Rows are converted with the CRD's printer columns, so look them up while the
        // first page is being listed rather than before starting the list.
        let (custom_columns, events) = buffer_until(
            resilient_watcher(api, watcher_config()).boxed(),
            crd_printer_columns(client, &api_resource),
        )
        .await;
        event_sender
            .send(KubernetesCustomResourceColumnsLoaded {
                cluster_key,
                columns: BTreeMap::from([(api_resource.clone(), custom_columns.clone())]),
            })
            .await
            .log_if_error("Failed to send custom resource columns");

        run_resource_watch(
            events,
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
            RetryTracker::new(),
        )
        .await;
    }
}

/// `stream` with the items `buffer_until` buffered in front of it.
pub(crate) type BufferedStream<S> = futures_util::stream::Chain<
    futures_util::stream::Iter<std::vec::IntoIter<<S as futures_util::Stream>::Item>>,
    S,
>;

/// Resolves `ready` while still polling `stream`, so neither waits for the other. Items
/// that arrive first are buffered and replayed, in order, ahead of the rest of the stream.
pub(crate) async fn buffer_until<S, F>(mut stream: S, ready: F) -> (F::Output, BufferedStream<S>)
where
    S: futures_util::Stream + Unpin,
    F: Future,
{
    let mut buffered = Vec::new();
    let mut ready = std::pin::pin!(ready);
    let mut stream_open = true;
    let output = loop {
        tokio::select! {
            output = &mut ready => break output,
            item = stream.next(), if stream_open => match item {
                Some(item) => buffered.push(item),
                None => stream_open = false,
            },
        }
    };
    (output, futures_util::stream::iter(buffered).chain(stream))
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
            resilient_watcher(api, watcher_config()),
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
            RetryTracker::new(),
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
            resilient_watcher(Api::<T>::all(client), watcher_config()),
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
            RetryTracker::new(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn items_that_arrive_before_the_future_resolves_are_replayed_in_order() {
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        let items = futures_util::stream::unfold(receiver, |mut receiver| async move {
            receiver.recv().await.map(|item| (item, receiver))
        })
        .boxed();
        let (resolve, ready) = oneshot::channel::<&str>();
        sender.send(1).unwrap();
        sender.send(2).unwrap();
        let resolver = tokio::spawn(async move {
            // Both items are pulled from the stream while the future is still pending.
            tokio::time::sleep(Duration::from_millis(20)).await;
            resolve.send("columns").unwrap();
        });

        let (output, rest) = buffer_until(items, async { ready.await.unwrap() }).await;
        resolver.await.unwrap();
        sender.send(3).unwrap();
        drop(sender);

        assert_eq!(output, "columns");
        assert_eq!(rest.collect::<Vec<_>>().await, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn a_stream_that_ends_first_still_waits_for_the_future() {
        let (output, rest) = buffer_until(futures_util::stream::iter([1, 2]), async {
            tokio::time::sleep(Duration::from_millis(10)).await;
            "done"
        })
        .await;

        assert_eq!(output, "done");
        assert_eq!(rest.collect::<Vec<_>>().await, vec![1, 2]);
    }
}
