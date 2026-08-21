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
    mut initialized: Option<oneshot::Sender<()>>,
) {
    let secrets = Api::<Secret>::namespaced(client.clone(), &namespace);
    let config_maps = Api::<ConfigMap>::namespaced(client, &namespace);
    let secret_stream = watcher(secrets, watcher_config().labels("owner=helm")).boxed();
    let config_map_stream = watcher(config_maps, watcher_config().labels("owner=helm")).boxed();
    futures_util::pin_mut!(secret_stream);
    futures_util::pin_mut!(config_map_stream);
    let mut records = BTreeMap::<String, HelmRelease>::new();
    let mut secrets_active = true;
    let mut config_maps_active = true;
    let mut secrets_synced = false;
    let mut config_maps_synced = false;

    while secrets_active || config_maps_active {
        tokio::select! {
            event = secret_stream.next(), if secrets_active => match event {
                Some(Ok(event)) => {
                    secrets_synced |= matches!(&event, Event::InitDone);
                    let changed = apply_helm_secret_event(event, &mut records);
                    if changed && secrets_synced && config_maps_synced {
                        send_helm_releases(&event_sender, cluster_key, &namespace, &records).await;
                    }
                    if secrets_synced && config_maps_synced {
                        let initial_sync_completed = initialized.take().is_some();
                        if initial_sync_completed && !changed {
                            send_helm_releases(&event_sender, cluster_key, &namespace, &records).await;
                        }
                    }
                }
                Some(Err(error)) => {
                    secrets_active = false;
                    secrets_synced = true;
                    event_sender.send(HelmReleaseBackendFailed { cluster_key, namespace: namespace.clone(), backend: "Secrets", error: format!("{error:#}") }).await.log_if_error("Failed to send Helm Secrets error");
                    if config_maps_synced {
                        let _ = initialized.take();
                        send_helm_releases(&event_sender, cluster_key, &namespace, &records).await;
                    }
                }
                None => {
                    secrets_active = false;
                    secrets_synced = true;
                    if config_maps_synced {
                        let _ = initialized.take();
                        send_helm_releases(&event_sender, cluster_key, &namespace, &records).await;
                    }
                },
            },
            event = config_map_stream.next(), if config_maps_active => match event {
                Some(Ok(event)) => {
                    config_maps_synced |= matches!(&event, Event::InitDone);
                    let changed = apply_helm_config_map_event(event, &mut records);
                    if changed && secrets_synced && config_maps_synced {
                        send_helm_releases(&event_sender, cluster_key, &namespace, &records).await;
                    }
                    if secrets_synced && config_maps_synced {
                        let initial_sync_completed = initialized.take().is_some();
                        if initial_sync_completed && !changed {
                            send_helm_releases(&event_sender, cluster_key, &namespace, &records).await;
                        }
                    }
                }
                Some(Err(error)) => {
                    config_maps_active = false;
                    config_maps_synced = true;
                    event_sender.send(HelmReleaseBackendFailed { cluster_key, namespace: namespace.clone(), backend: "ConfigMaps", error: format!("{error:#}") }).await.log_if_error("Failed to send Helm ConfigMaps error");
                    if secrets_synced {
                        let _ = initialized.take();
                        send_helm_releases(&event_sender, cluster_key, &namespace, &records).await;
                    }
                }
                None => {
                    config_maps_active = false;
                    config_maps_synced = true;
                    if secrets_synced {
                        let _ = initialized.take();
                        send_helm_releases(&event_sender, cluster_key, &namespace, &records).await;
                    }
                },
            },
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

pub(crate) fn apply_helm_secret_event(
    event: Event<Secret>,
    records: &mut BTreeMap<String, HelmRelease>,
) -> bool {
    match event {
        Event::Apply(secret) => upsert_helm_secret(secret, records),
        Event::Delete(secret) => records
            .remove(&helm_storage_key(
                "secret",
                secret.metadata.name.as_deref().unwrap_or_default(),
            ))
            .is_some(),
        Event::Init => {
            records.retain(|key, _| !key.starts_with("secret/"));
            false
        }
        Event::InitApply(secret) => upsert_helm_secret(secret, records),
        Event::InitDone => true,
    }
}

pub(crate) fn apply_helm_config_map_event(
    event: Event<ConfigMap>,
    records: &mut BTreeMap<String, HelmRelease>,
) -> bool {
    match event {
        Event::Apply(config_map) => upsert_helm_config_map(config_map, records),
        Event::Delete(config_map) => records
            .remove(&helm_storage_key(
                "configmap",
                config_map.metadata.name.as_deref().unwrap_or_default(),
            ))
            .is_some(),
        Event::Init => {
            records.retain(|key, _| !key.starts_with("configmap/"));
            false
        }
        Event::InitApply(config_map) => upsert_helm_config_map(config_map, records),
        Event::InitDone => true,
    }
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
