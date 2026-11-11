use super::*;

pub(crate) async fn start_cluster_connection(
    cluster_key: i32,
    cluster_name: &str,
    event_sender: WorkerResultSender,
) -> Result<ClusterConnection> {
    info!("Starting cluster connection: {}", cluster_name);
    ClusterConnection::new(cluster_key, cluster_name, event_sender).await
}

/// Start watching a resource type in its selected namespace scope.
pub(crate) async fn start_resource_watcher(
    cluster_key: i32,
    client: kube::Client,
    api_resource: ApiResource,
    namespace: Option<String>,
    event_sender: WorkerResultSender,
    initialized: Option<oneshot::Sender<()>>,
) -> Result<(KubernetesResourceWatchStarted, tokio::task::JoinHandle<()>)> {
    start_resource_watcher_with_scope(
        cluster_key,
        client,
        api_resource,
        namespace,
        None,
        event_sender,
        initialized,
    )
    .await
}

pub(crate) async fn start_all_namespaces_resource_watcher(
    cluster_key: i32,
    client: kube::Client,
    api_resource: ApiResource,
    namespaces: BTreeSet<String>,
    event_sender: WorkerResultSender,
    initialized: Option<oneshot::Sender<()>>,
) -> Result<(KubernetesResourceWatchStarted, tokio::task::JoinHandle<()>)> {
    start_resource_watcher_with_scope(
        cluster_key,
        client,
        api_resource,
        None,
        Some(namespaces),
        event_sender,
        initialized,
    )
    .await
}

pub(crate) async fn start_resource_watcher_with_scope(
    cluster_key: i32,
    client: kube::Client,
    api_resource: ApiResource,
    namespace: Option<String>,
    watched_namespaces: Option<BTreeSet<String>>,
    event_sender: WorkerResultSender,
    initialized: Option<oneshot::Sender<()>>,
) -> Result<(KubernetesResourceWatchStarted, tokio::task::JoinHandle<()>)> {
    info!(
        "Starting resource watcher for {}/{} in {}",
        api_resource.group,
        api_resource.name,
        namespace.as_deref().unwrap_or("cluster-wide scope")
    );

    if api_resource.is_helm_releases() {
        let namespace = namespace.context("Helm releases require a namespace")?;
        let task = tokio::spawn(watch_helm_releases(
            cluster_key,
            client,
            namespace.clone(),
            event_sender,
            initialized,
        ));
        return Ok((
            KubernetesResourceWatchStarted {
                cluster_key,
                api_resource,
                namespace: Some(namespace),
            },
            task,
        ));
    }

    let context = TypedWatcherContext {
        client: client.clone(),
        event_sender: event_sender.clone(),
        cluster_key,
        api_resource: api_resource.clone(),
        namespace: namespace.clone(),
        watched_namespaces: watched_namespaces.clone(),
    };
    let watcher = if let Some(watcher) = resource_handlers::watcher_for(context) {
        watcher
    } else {
        let custom_columns = KubernetesApiInspector {
            client: client.clone(),
        }
        .custom_resource_columns()
        .await
        .remove(&api_resource)
        .unwrap_or_default();
        event_sender
            .send(KubernetesCustomResourceColumnsLoaded {
                cluster_key,
                columns: BTreeMap::from([(api_resource.clone(), custom_columns.clone())]),
            })
            .await
            .log_if_error("Failed to send custom resource columns");
        Box::new(DynamicKubernetesResourceWatcher {
            client,
            event_sender,
            cluster_key,
            api_resource: api_resource.clone(),
            namespace: namespace.clone(),
            watched_namespaces,
            custom_columns,
        })
    };

    let task = tokio::spawn(watcher.watch_resources(initialized));

    Ok((
        KubernetesResourceWatchStarted {
            cluster_key,
            api_resource,
            namespace,
        },
        task,
    ))
}

pub(crate) async fn watch_helm_releases(
    cluster_key: i32,
    client: kube::Client,
    namespace: String,
    event_sender: WorkerResultSender,
    initialized: Option<oneshot::Sender<()>>,
) {
    let secrets = Api::<Secret>::namespaced(client.clone(), &namespace);
    let config_maps = Api::<ConfigMap>::namespaced(client, &namespace);
    consume_helm_streams(
        resilient_watcher(secrets, watcher_config().labels("owner=helm")),
        resilient_watcher(config_maps, watcher_config().labels("owner=helm")),
        &HelmWatchContext {
            cluster_key,
            namespace,
            event_sender,
        },
        initialized,
        [RetryTracker::new(), RetryTracker::new()],
    )
    .await;
}

pub(crate) struct HelmWatchContext {
    pub(crate) cluster_key: i32,
    pub(crate) namespace: String,
    pub(crate) event_sender: WorkerResultSender,
}

/// Per storage backend (Secrets, ConfigMaps) bookkeeping for the merged list.
struct HelmBackend {
    name: &'static str,
    active: bool,
    /// The backend has completed (or given up on) its first list, so it no
    /// longer holds back the merged release list.
    synced: bool,
    retry: RetryTracker,
    /// Storage keys seen during a relist in progress. Existing records stay
    /// visible until `InitDone` prunes the ones that were not seen again.
    listing: Option<BTreeSet<String>>,
}

impl HelmBackend {
    fn new(name: &'static str, retry: RetryTracker) -> Self {
        Self {
            name,
            active: true,
            synced: false,
            retry,
            listing: None,
        }
    }
}

struct HelmWatchState<'a> {
    context: &'a HelmWatchContext,
    records: BTreeMap<String, HelmRelease>,
    initialized: Option<oneshot::Sender<()>>,
    backends: [HelmBackend; 2],
}

impl HelmWatchState<'_> {
    async fn send_releases(&self) {
        send_helm_releases(
            &self.context.event_sender,
            self.context.cluster_key,
            &self.context.namespace,
            &self.records,
        )
        .await;
    }

    async fn handle<T, E: Debug>(
        &mut self,
        index: usize,
        item: Option<std::result::Result<Event<T>, E>>,
        apply: impl FnOnce(
            Event<T>,
            &mut BTreeMap<String, HelmRelease>,
            &mut Option<BTreeSet<String>>,
        ) -> bool,
    ) {
        let other_synced = self.backends[1 - index].synced;
        let backend = &mut self.backends[index];
        match item {
            Some(item) => {
                let transition = backend.retry.observe(&item, backend.name);
                let name = backend.name;
                match item {
                    Ok(event) => {
                        if transition == RetryTransition::Recovered {
                            self.context
                                .event_sender
                                .send(HelmReleaseBackendRecovered {
                                    cluster_key: self.context.cluster_key,
                                    namespace: self.context.namespace.clone(),
                                    backend: name,
                                })
                                .await
                                .log_if_error("Failed to send Helm backend recovery");
                        }
                        let backend = &mut self.backends[index];
                        backend.synced |= matches!(&event, Event::InitDone);
                        let changed = apply(event, &mut self.records, &mut backend.listing);
                        if self.backends.iter().all(|backend| backend.synced) {
                            let initial_sync_completed = self.initialized.take().is_some();
                            if changed || initial_sync_completed {
                                self.send_releases().await;
                            }
                        }
                    }
                    Err(error) => {
                        if transition == RetryTransition::Degraded {
                            self.context
                                .event_sender
                                .send(HelmReleaseBackendFailed {
                                    cluster_key: self.context.cluster_key,
                                    namespace: self.context.namespace.clone(),
                                    backend: name,
                                    error: format!("{error:#?}"),
                                })
                                .await
                                .log_if_error("Failed to send Helm backend error");
                        }
                        // Startup must not stay blocked on a backend that cannot
                        // list, even if the failure turns out to be a short blip.
                        let _ = self.initialized.take();
                        // Publishing is separate: a backend that merely hiccuped
                        // is still expected to deliver its list, so only one that
                        // has been failing past the grace period stops holding
                        // back the other backend's releases.
                        if transition == RetryTransition::Degraded {
                            let newly_synced =
                                !std::mem::replace(&mut self.backends[index].synced, true);
                            if newly_synced && other_synced {
                                self.send_releases().await;
                            }
                        }
                    }
                }
            }
            None => {
                backend.active = false;
                backend.synced = true;
                if other_synced {
                    let _ = self.initialized.take();
                    self.send_releases().await;
                }
            }
        }
    }
}

/// Merge the Secret and ConfigMap storage backends into one release list. A
/// backend that keeps failing is reported after the grace period, but both
/// streams keep running so the backend recovers on its own.
pub(crate) async fn consume_helm_streams<SE: Debug, CE: Debug>(
    secret_stream: impl futures_util::Stream<Item = std::result::Result<Event<Secret>, SE>>,
    config_map_stream: impl futures_util::Stream<Item = std::result::Result<Event<ConfigMap>, CE>>,
    context: &HelmWatchContext,
    initialized: Option<oneshot::Sender<()>>,
    [secret_retry, config_map_retry]: [RetryTracker; 2],
) {
    pin_mut!(secret_stream);
    pin_mut!(config_map_stream);
    let mut state = HelmWatchState {
        context,
        records: BTreeMap::new(),
        initialized,
        backends: [
            HelmBackend::new("Secrets", secret_retry),
            HelmBackend::new("ConfigMaps", config_map_retry),
        ],
    };

    while state.backends.iter().any(|backend| backend.active) {
        tokio::select! {
            item = secret_stream.next(), if state.backends[0].active => {
                state.handle(0, item, apply_helm_secret_event).await;
            }
            item = config_map_stream.next(), if state.backends[1].active => {
                state.handle(1, item, apply_helm_config_map_event).await;
            }
        }
    }
}

pub(crate) async fn send_helm_releases(
    event_sender: &WorkerResultSender,
    cluster_key: i32,
    namespace: &str,
    records: &BTreeMap<String, HelmRelease>,
) {
    let releases = merged_helm_releases(records);
    event_sender
        .send(HelmReleasesReplaced {
            cluster_key,
            namespace: namespace.to_owned(),
            releases,
        })
        .await
        .log_if_error("Failed to send Helm releases");
}

pub(crate) fn merged_helm_releases(records: &BTreeMap<String, HelmRelease>) -> Vec<HelmRelease> {
    let mut releases = BTreeMap::<(String, String, i64), HelmRelease>::new();
    for release in records.values().cloned() {
        let key = (
            release.namespace.clone(),
            release.name.clone(),
            release.revision,
        );
        match releases.get(&key) {
            Some(existing) if existing.storage == StorageDriver::Secret => {}
            _ => {
                releases.insert(key, release);
            }
        }
    }
    releases.into_values().collect()
}

pub(crate) fn helm_storage_key(prefix: &str, name: &str) -> String {
    format!("{prefix}/{name}")
}

/// Apply one watch event to a backend's records and report whether the merged
/// release list should be republished. Relist events only stage: records stay
/// visible until `InitDone` removes the ones the relist did not return.
fn apply_helm_event<T>(
    event: Event<T>,
    records: &mut BTreeMap<String, HelmRelease>,
    listing: &mut Option<BTreeSet<String>>,
    prefix: &str,
    storage_key: impl Fn(&T) -> String,
    upsert: impl Fn(T, &mut BTreeMap<String, HelmRelease>) -> bool,
) -> bool {
    match event {
        Event::Apply(object) => upsert(object, records),
        Event::Delete(object) => records.remove(&storage_key(&object)).is_some(),
        Event::Init => {
            *listing = Some(BTreeSet::new());
            false
        }
        Event::InitApply(object) => {
            if let Some(seen) = listing {
                seen.insert(storage_key(&object));
            }
            upsert(object, records);
            false
        }
        Event::InitDone => {
            if let Some(seen) = listing.take() {
                let prefix = format!("{prefix}/");
                records.retain(|key, _| !key.starts_with(&prefix) || seen.contains(key));
            }
            true
        }
    }
}

pub(crate) fn apply_helm_secret_event(
    event: Event<Secret>,
    records: &mut BTreeMap<String, HelmRelease>,
    listing: &mut Option<BTreeSet<String>>,
) -> bool {
    apply_helm_event(
        event,
        records,
        listing,
        "secret",
        |secret| {
            helm_storage_key(
                "secret",
                secret.metadata.name.as_deref().unwrap_or_default(),
            )
        },
        upsert_helm_secret,
    )
}

pub(crate) fn apply_helm_config_map_event(
    event: Event<ConfigMap>,
    records: &mut BTreeMap<String, HelmRelease>,
    listing: &mut Option<BTreeSet<String>>,
) -> bool {
    apply_helm_event(
        event,
        records,
        listing,
        "configmap",
        |config_map| {
            helm_storage_key(
                "configmap",
                config_map.metadata.name.as_deref().unwrap_or_default(),
            )
        },
        upsert_helm_config_map,
    )
}

pub(crate) fn upsert_helm_secret(
    secret: Secret,
    records: &mut BTreeMap<String, HelmRelease>,
) -> bool {
    let name = secret.metadata.name.unwrap_or_default();
    let key = helm_storage_key("secret", &name);
    let Some(encoded) = secret.data.and_then(|data| data.get("release").cloned()) else {
        return records.remove(&key).is_some();
    };
    match decode_release(StorageDriver::Secret, name, &encoded.0) {
        Ok(mut release) => {
            release.storage_labels = secret.metadata.labels.unwrap_or_default();
            release.storage_annotations = secret.metadata.annotations.unwrap_or_default();
            records.insert(key, release);
            true
        }
        Err(_) => records.remove(&key).is_some(),
    }
}

pub(crate) fn upsert_helm_config_map(
    config_map: ConfigMap,
    records: &mut BTreeMap<String, HelmRelease>,
) -> bool {
    let name = config_map.metadata.name.unwrap_or_default();
    let key = helm_storage_key("configmap", &name);
    let Some(encoded) = config_map
        .data
        .and_then(|data| data.get("release").cloned())
    else {
        return records.remove(&key).is_some();
    };
    match decode_release(StorageDriver::ConfigMap, name, encoded.as_bytes()) {
        Ok(mut release) => {
            release.storage_labels = config_map.metadata.labels.unwrap_or_default();
            release.storage_annotations = config_map.metadata.annotations.unwrap_or_default();
            records.insert(key, release);
            true
        }
        Err(_) => records.remove(&key).is_some(),
    }
}

pub(crate) type ResourceWatcherFuture = Pin<Box<dyn Future<Output = ()> + Send>>;

/// Object-safe dispatch point for concrete Kubernetes resource watchers.
pub(crate) trait ResourceWatcher: Send {
    fn watch_resources(
        self: Box<Self>,
        initialized: Option<oneshot::Sender<()>>,
    ) -> ResourceWatcherFuture;
}

#[derive(Clone)]
pub(crate) struct TypedWatcherContext {
    pub(crate) client: kube::Client,
    pub(crate) event_sender: WorkerResultSender,
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) watched_namespaces: Option<BTreeSet<String>>,
}
