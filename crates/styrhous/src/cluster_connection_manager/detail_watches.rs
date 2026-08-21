use super::*;

pub(crate) struct ResourceDetailWatchRequest {
    pub cluster_key: i32,
    pub client: kube::Client,
    pub api_resource: ApiResource,
    pub namespace: Option<String>,
    pub resource_name: String,
    pub resource_uid: String,
    pub history_entry_id: u64,
    pub pod_metrics_api_available: bool,
    pub node_metrics_api_available: bool,
    pub event_sender: WorkerResultSender,
}

/// Keep one inspector history entry current independently of the compact
/// resource-table watcher. The worker owns it until that entry leaves history.
pub(crate) async fn watch_resource_detail(request: ResourceDetailWatchRequest) {
    let ResourceDetailWatchRequest {
        cluster_key,
        client,
        api_resource,
        namespace,
        resource_name,
        resource_uid,
        history_entry_id,
        pod_metrics_api_available,
        node_metrics_api_available,
        event_sender,
    } = request;
    let root_name = resource_name.clone();
    let metrics_api_resource = api_resource.clone();
    tokio::join!(
        watch_detail_object(
            cluster_key,
            client.clone(),
            api_resource.clone(),
            namespace.clone(),
            resource_name,
            history_entry_id,
            event_sender.clone(),
        ),
        watch_detail_events(
            cluster_key,
            client.clone(),
            namespace.clone(),
            resource_uid.clone(),
            history_entry_id,
            event_sender.clone(),
        ),
        watch_managed_resources(ManagedResourceWatchRequest {
            cluster_key,
            client: client.clone(),
            root_api_resource: api_resource.clone(),
            namespace: namespace.clone(),
            root_name: root_name.clone(),
            root_uid: resource_uid,
            history_entry_id,
            event_sender: event_sender.clone(),
        }),
        watch_pod_detail_metrics(PodDetailMetricsWatchRequest {
            cluster_key,
            client: client.clone(),
            api_resource: metrics_api_resource,
            namespace: namespace.clone(),
            resource_name: root_name.clone(),
            history_entry_id,
            pod_metrics_api_available,
            event_sender: event_sender.clone(),
        }),
        watch_node_detail_metrics(NodeDetailMetricsWatchRequest {
            cluster_key,
            client,
            api_resource,
            resource_name: root_name,
            history_entry_id,
            node_metrics_api_available,
            event_sender,
        }),
    );
}

/// Poll a visible namespace rather than watching the Metrics API: metrics-server publishes
/// sampled values and is not a source of Kubernetes object lifecycle events.
pub(crate) async fn watch_pod_metrics_namespace(
    cluster_key: i32,
    client: kube::Client,
    namespace: String,
    event_sender: WorkerResultSender,
) {
    let mut interval = tokio::time::interval(POD_METRICS_POLL_INTERVAL);
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        match list_pod_metrics(&client, &namespace).await {
            Ok(usages) => event_sender
                .send(PodMetricsUpdated {
                    cluster_key,
                    namespace: namespace.clone(),
                    usages,
                })
                .await
                .log_if_error("Failed to send Pod metrics update"),
            Err(error) if is_metrics_api_not_found(&error) => {
                report_metrics_api_unavailable(
                    &event_sender,
                    PodMetricsApiUnavailable { cluster_key },
                    "Failed to send Metrics API unavailable",
                )
                .await;
                return;
            }
            Err(error) => event_sender
                .send(PodMetricsWatchFailed {
                    cluster_key,
                    namespace: namespace.clone(),
                    error: format!("{error:#?}"),
                })
                .await
                .log_if_error("Failed to send Pod metrics error"),
        }
    }
}

/// Nodes are cluster-scoped, so one poller supplies the visible Nodes workspace.
pub(crate) async fn watch_node_metrics(
    cluster_key: i32,
    client: kube::Client,
    event_sender: WorkerResultSender,
) {
    let mut interval = tokio::time::interval(POD_METRICS_POLL_INTERVAL);
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        match list_node_metrics(&client).await {
            Ok(usages) => event_sender
                .send(NodeMetricsUpdated {
                    cluster_key,
                    usages,
                })
                .await
                .log_if_error("Failed to send Node metrics update"),
            Err(error) if is_metrics_api_not_found(&error) => {
                report_metrics_api_unavailable(
                    &event_sender,
                    NodeMetricsApiUnavailable { cluster_key },
                    "Failed to send Node Metrics API unavailable",
                )
                .await;
                return;
            }
            Err(error) => event_sender
                .send(NodeMetricsWatchFailed {
                    cluster_key,
                    error: format!("{error:#?}"),
                })
                .await
                .log_if_error("Failed to send Node metrics error"),
        }
    }
}

pub(crate) struct PodDetailMetricsWatchRequest {
    cluster_key: i32,
    client: kube::Client,
    api_resource: ApiResource,
    namespace: Option<String>,
    resource_name: String,
    history_entry_id: u64,
    pod_metrics_api_available: bool,
    event_sender: WorkerResultSender,
}

pub(crate) async fn watch_pod_detail_metrics(request: PodDetailMetricsWatchRequest) {
    let PodDetailMetricsWatchRequest {
        cluster_key,
        client,
        api_resource,
        namespace,
        resource_name,
        history_entry_id,
        pod_metrics_api_available,
        event_sender,
    } = request;
    if api_resource.kind != "Pod" || api_resource.group != "core" || !pod_metrics_api_available {
        return;
    }
    let Some(namespace) = namespace else {
        return;
    };
    let mut interval = tokio::time::interval(POD_METRICS_POLL_INTERVAL);
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        match get_pod_metrics(&client, &namespace, &resource_name).await {
            Ok(Some(usage)) => event_sender
                .send(ResourceDetailPodUsageUpdated {
                    cluster_key,
                    history_entry_id,
                    usage,
                })
                .await
                .log_if_error("Failed to send Pod detail metrics update"),
            Ok(None) => event_sender
                .send(ResourceDetailPodUsageMissing {
                    cluster_key,
                    history_entry_id,
                })
                .await
                .log_if_error("Failed to send missing Pod detail metrics update"),
            Err(error) if is_metrics_api_not_found(&error) => {
                report_metrics_api_unavailable(
                    &event_sender,
                    PodMetricsApiUnavailable { cluster_key },
                    "Failed to send Metrics API unavailable",
                )
                .await;
                return;
            }
            Err(error) => event_sender
                .send(ResourceDetailPodUsageFailed {
                    cluster_key,
                    history_entry_id,
                    error: format!("{error:#?}"),
                })
                .await
                .log_if_error("Failed to send Pod detail metrics error"),
        }
    }
}

pub(crate) struct NodeDetailMetricsWatchRequest {
    cluster_key: i32,
    client: kube::Client,
    api_resource: ApiResource,
    resource_name: String,
    history_entry_id: u64,
    node_metrics_api_available: bool,
    event_sender: WorkerResultSender,
}

pub(crate) async fn watch_node_detail_metrics(request: NodeDetailMetricsWatchRequest) {
    let NodeDetailMetricsWatchRequest {
        cluster_key,
        client,
        api_resource,
        resource_name,
        history_entry_id,
        node_metrics_api_available,
        event_sender,
    } = request;
    if api_resource.kind != "Node" || api_resource.group != "core" || !node_metrics_api_available {
        return;
    }
    let mut interval = tokio::time::interval(POD_METRICS_POLL_INTERVAL);
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        match get_node_metrics(&client, &resource_name).await {
            Ok(Some(usage)) => event_sender
                .send(ResourceDetailNodeUsageUpdated {
                    cluster_key,
                    history_entry_id,
                    usage,
                })
                .await
                .log_if_error("Failed to send Node detail metrics update"),
            Ok(None) => event_sender
                .send(ResourceDetailNodeUsageMissing {
                    cluster_key,
                    history_entry_id,
                })
                .await
                .log_if_error("Failed to send missing Node detail metrics update"),
            Err(error) if is_metrics_api_not_found(&error) => {
                report_metrics_api_unavailable(
                    &event_sender,
                    NodeMetricsApiUnavailable { cluster_key },
                    "Failed to send Node Metrics API unavailable",
                )
                .await;
                return;
            }
            Err(error) => event_sender
                .send(ResourceDetailNodeUsageFailed {
                    cluster_key,
                    history_entry_id,
                    error: format!("{error:#?}"),
                })
                .await
                .log_if_error("Failed to send Node detail metrics error"),
        }
    }
}

pub(crate) fn is_metrics_api_not_found(error: &anyhow::Error) -> bool {
    error.downcast_ref::<kube::Error>().is_some_and(|error| {
        matches!(error, kube::Error::Api(response)
            if response.code == 404
                && response.message.contains("the server could not find the requested resource"))
    })
}

async fn report_metrics_api_unavailable<R: WorkerResult>(
    event_sender: &WorkerResultSender,
    result: R,
    error_context: &'static str,
) {
    event_sender.send(result).await.log_if_error(error_context);
}

pub(crate) fn metrics_pod_api(client: &kube::Client, namespace: &str) -> Api<DynamicObject> {
    let gvk = GroupVersionKind::gvk("metrics.k8s.io", "v1beta1", "PodMetrics");
    let resource = kube::core::ApiResource::from_gvk_with_plural(&gvk, "pods");
    Api::namespaced_with(client.clone(), namespace, &resource)
}

pub(crate) fn metrics_node_api(client: &kube::Client) -> Api<DynamicObject> {
    let gvk = GroupVersionKind::gvk("metrics.k8s.io", "v1beta1", "NodeMetrics");
    let resource = kube::core::ApiResource::from_gvk_with_plural(&gvk, "nodes");
    Api::all_with(client.clone(), &resource)
}

pub(crate) async fn list_pod_metrics(
    client: &kube::Client,
    namespace: &str,
) -> Result<BTreeMap<String, PodUsage>> {
    let metrics = metrics_pod_api(client, namespace)
        .list(&ListParams::default())
        .await?;
    metrics
        .items
        .into_iter()
        .map(|metric| pod_usage_from_value(k8s_openapi::serde_json::to_value(metric)?))
        .collect()
}

pub(crate) async fn get_pod_metrics(
    client: &kube::Client,
    namespace: &str,
    name: &str,
) -> Result<Option<PodUsage>> {
    let metrics = match metrics_pod_api(client, namespace).get(name).await {
        Ok(metrics) => metrics,
        Err(kube::Error::Api(response)) if is_metric_sample_missing(&response, name) => {
            return Ok(None);
        }
        Err(error) => return Err(error.into()),
    };
    let (_, usage) = pod_usage_from_value(k8s_openapi::serde_json::to_value(metrics)?)?;
    Ok(Some(usage))
}

pub(crate) async fn list_node_metrics(
    client: &kube::Client,
) -> Result<BTreeMap<String, NodeUsage>> {
    let metrics = metrics_node_api(client)
        .list(&ListParams::default())
        .await?;
    metrics
        .items
        .into_iter()
        .map(|metric| node_usage_from_value(k8s_openapi::serde_json::to_value(metric)?))
        .collect()
}

pub(crate) async fn get_node_metrics(
    client: &kube::Client,
    name: &str,
) -> Result<Option<NodeUsage>> {
    let metrics = match metrics_node_api(client).get(name).await {
        Ok(metrics) => metrics,
        Err(kube::Error::Api(response)) if is_metric_sample_missing(&response, name) => {
            return Ok(None);
        }
        Err(error) => return Err(error.into()),
    };
    let (_, usage) = node_usage_from_value(k8s_openapi::serde_json::to_value(metrics)?)?;
    Ok(Some(usage))
}

pub(crate) fn is_metric_sample_missing(response: &kube::core::Status, name: &str) -> bool {
    response.code == 404
        && response
            .details
            .as_ref()
            .is_some_and(|details| details.group == "metrics.k8s.io" && details.name == name)
}
