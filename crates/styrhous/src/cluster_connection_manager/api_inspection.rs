use super::*;

const MAX_CONCURRENT_API_DISCOVERY_REQUESTS: usize = 4;
const API_DISCOVERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(25);
/// Time limits for one discovery run. `group` bounds each API group request and
/// `crd_metadata` is reserved at the end of `total` for the optional CRD listing, so a
/// slow group list or CRD listing cannot turn a partial discovery into a failure.
#[derive(Clone, Copy)]
struct DiscoveryTimeouts {
    total: std::time::Duration,
    group: std::time::Duration,
    crd_metadata: std::time::Duration,
}

const DISCOVERY_TIMEOUTS: DiscoveryTimeouts = DiscoveryTimeouts {
    total: API_DISCOVERY_TIMEOUT,
    // Healthy groups stalled for over 10s were observed on a loaded local cluster, so a
    // group is only dropped once it has clearly stopped answering. A stalled group
    // occupies one of the concurrent request slots without blocking the others.
    group: std::time::Duration::from_secs(15),
    crd_metadata: std::time::Duration::from_secs(5),
};

const _: () = {
    assert!(DISCOVERY_TIMEOUTS.crd_metadata.as_nanos() < DISCOVERY_TIMEOUTS.total.as_nanos());
    assert!(
        DISCOVERY_TIMEOUTS.group.as_nanos()
            <= DISCOVERY_TIMEOUTS.total.as_nanos() - DISCOVERY_TIMEOUTS.crd_metadata.as_nanos()
    );
};

pub(crate) struct KubernetesApiInspector {
    pub(crate) client: kube::Client,
}

pub(crate) struct ApiInspection {
    pub(crate) api_resources: Vec<ApiResource>,
    pub(crate) scalable_api_resources: BTreeSet<ApiResource>,
    pub(crate) failed_api_groups: Vec<FailedApiGroup>,
    pub(crate) pod_metrics_api_available: bool,
    pub(crate) node_metrics_api_available: bool,
    pub(crate) custom_resource_columns: BTreeMap<ApiResource, Vec<CustomResourceColumn>>,
    pub(crate) resource_schemas: BTreeMap<ApiResource, ResourceSchema>,
}

pub(crate) struct DiscoveredApiResources {
    api_resources: Vec<ApiResource>,
    scalable_api_resources: BTreeSet<ApiResource>,
}

impl KubernetesApiInspector {
    async fn get_api_resources_for_group_versions(
        &self,
        api_group: APIGroup,
        versions: Vec<GroupVersionForDiscovery>,
    ) -> Result<DiscoveredApiResources> {
        let api_group_name = api_group.name;
        let mut resource_lists = Vec::with_capacity(versions.len());
        for version in &versions {
            resource_lists.push(
                self.client
                    .list_api_group_resources(&version.group_version)
                    .await?,
            );
        }
        let resources = resource_lists
            .iter()
            .zip(versions)
            .map(|(resources, version)| {
                let version_name = version.version.clone();

                let mut api_resources = Vec::new();
                let mut scalable_api_resources = BTreeSet::new();

                for resource in &resources.resources {
                    // Skip resources like "Status" and "Scale"
                    if resource.name.contains('/') {
                        continue;
                    }

                    let api_resource = ApiResource {
                        group: api_group_name.clone(),
                        version: version_name.clone(),
                        kind: resource.kind.clone(),
                        name: resource.name.clone(),
                        namespaced: resource.namespaced,
                    };
                    if supports_scale_subresource(&resources.resources, &resource.name) {
                        scalable_api_resources.insert(api_resource.clone());
                    }
                    api_resources.push(api_resource);
                }

                DiscoveredApiResources {
                    api_resources,
                    scalable_api_resources,
                }
            })
            .fold(
                DiscoveredApiResources {
                    api_resources: Vec::new(),
                    scalable_api_resources: BTreeSet::new(),
                },
                |mut all, discovered| {
                    all.api_resources.extend(discovered.api_resources);
                    all.scalable_api_resources
                        .extend(discovered.scalable_api_resources);
                    all
                },
            );

        Ok(resources)
    }

    async fn get_core_api_resources(&self) -> Result<DiscoveredApiResources> {
        let core_api_versions = self.client.list_core_api_versions().await?;

        let mut discovered = DiscoveredApiResources {
            api_resources: Vec::new(),
            scalable_api_resources: BTreeSet::new(),
        };

        for version in &core_api_versions.versions {
            let api_resources = self.client.list_core_api_resources(version).await?;

            for resource in &api_resources.resources {
                if resource.name.contains("/") {
                    continue;
                }

                let api_resource = ApiResource {
                    group: "core".to_string(),
                    version: version.clone(),
                    kind: resource.kind.clone(),
                    name: resource.name.clone(),
                    namespaced: resource.namespaced,
                };
                if supports_scale_subresource(&api_resources.resources, &resource.name) {
                    discovered
                        .scalable_api_resources
                        .insert(api_resource.clone());
                }
                discovered.api_resources.push(api_resource);
            }
        }

        Ok(discovered)
    }

    pub(crate) async fn inspect_api(&self) -> Result<ApiInspection> {
        self.inspect_api_with_timeouts(DISCOVERY_TIMEOUTS).await
    }

    async fn inspect_api_with_timeouts(
        &self,
        timeouts: DiscoveryTimeouts,
    ) -> Result<ApiInspection> {
        let started = tokio::time::Instant::now();
        let groups_deadline = started + (timeouts.total - timeouts.crd_metadata);
        let crd_deadline = started + timeouts.total;

        // Without the group list and the core API nothing works, so these stay fatal.
        let (api_groups, core_resources) = await_discovery_with_timeout(
            timeouts.total - timeouts.crd_metadata,
            "Kubernetes API discovery",
            async {
                let api_groups = self.client.list_api_groups().await?;
                let core_resources = self.get_core_api_resources().await?;
                Ok((api_groups, core_resources))
            },
        )
        .await?;

        let client = self.client.clone();
        let tasks = api_groups.groups.into_iter().map(move |api_group| {
            let versions = api_group
                .preferred_version
                .clone()
                .map(|v| vec![v])
                .unwrap_or_else(|| api_group.versions.clone());
            let label = versions
                .first()
                .map_or_else(|| api_group.name.clone(), |v| v.group_version.clone());
            let inspector = KubernetesApiInspector {
                client: client.clone(),
            };

            (label, async move {
                inspector
                    .get_api_resources_for_group_versions(api_group, versions)
                    .await
            })
        });

        let GroupDiscovery {
            discovered,
            failures: failed_api_groups,
        } = collect_tolerant_group_discovery(
            tasks,
            MAX_CONCURRENT_API_DISCOVERY_REQUESTS,
            timeouts.group,
            groups_deadline,
        )
        .await;
        for failure in &failed_api_groups {
            tracing::warn!(
                group_version = %failure.group_version,
                error = %failure.error,
                "API group discovery failed; continuing without it"
            );
        }
        let discovered_resources =
            discovered
                .into_iter()
                .fold(core_resources, |mut all, discovered| {
                    all.api_resources.extend(discovered.api_resources);
                    all.scalable_api_resources
                        .extend(discovered.scalable_api_resources);
                    all
                });

        let pod_metrics_api_available =
            pod_metrics_api_available(&discovered_resources.api_resources);
        let node_metrics_api_available =
            node_metrics_api_available(&discovered_resources.api_resources);
        let (custom_resource_columns, resource_schemas) = match tokio::time::timeout_at(
            crd_deadline,
            self.custom_resource_metadata(),
        )
        .await
        {
            Ok(metadata) => metadata,
            Err(_) => {
                tracing::warn!(
                    "listing CustomResourceDefinitions timed out; continuing without custom resource metadata"
                );
                Default::default()
            }
        };
        Ok(ApiInspection {
            api_resources: discovered_resources.api_resources,
            scalable_api_resources: discovered_resources.scalable_api_resources,
            failed_api_groups,
            pod_metrics_api_available,
            node_metrics_api_available,
            custom_resource_columns,
            resource_schemas,
        })
    }

    async fn custom_resource_metadata(
        &self,
    ) -> (
        BTreeMap<ApiResource, Vec<CustomResourceColumn>>,
        BTreeMap<ApiResource, ResourceSchema>,
    ) {
        let crds = Api::<CustomResourceDefinition>::all(self.client.clone());
        let Ok(crds) = crds.list(&Default::default()).await else {
            // Access to CRDs is commonly restricted. Dynamic resources still work without
            // their optional columns, so do not fail API discovery in that case.
            return (BTreeMap::new(), BTreeMap::new());
        };

        let mut columns_by_resource = BTreeMap::new();
        let mut schemas_by_resource = BTreeMap::new();
        for crd in &crds.items {
            let spec = &crd.spec;
            for version in &spec.versions {
                let api_resource = ApiResource {
                    group: spec.group.clone(),
                    version: version.name.clone(),
                    kind: spec.names.kind.clone(),
                    name: spec.names.plural.clone(),
                    namespaced: spec.scope == "Namespaced",
                };
                if let Some(columns) = &version.additional_printer_columns {
                    columns_by_resource.insert(
                        api_resource.clone(),
                        columns
                            .iter()
                            .enumerate()
                            .map(|(index, column)| CustomResourceColumn {
                                id: format!("crd-{index}"),
                                label: column.name.clone(),
                                json_path: column.json_path.clone(),
                                type_: column.type_.clone(),
                                format: column.format.clone(),
                            })
                            .collect(),
                    );
                }
                if let Some(schema) = version
                    .schema
                    .as_ref()
                    .and_then(|schema| schema.open_api_v3_schema.as_ref())
                    && let Ok(root) = k8s_openapi::serde_json::to_value(schema)
                {
                    schemas_by_resource.insert(api_resource, ResourceSchema::new(root));
                }
            }
        }
        (columns_by_resource, schemas_by_resource)
    }

    pub(crate) async fn custom_resource_columns(
        &self,
    ) -> BTreeMap<ApiResource, Vec<CustomResourceColumn>> {
        self.custom_resource_metadata().await.0
    }
}

struct GroupDiscovery<T> {
    discovered: Vec<T>,
    failures: Vec<FailedApiGroup>,
}

/// Runs the labelled group requests with bounded concurrency. A request that fails or
/// exceeds `per_request_timeout` (or the shared `deadline`) is reported as a
/// `FailedApiGroup` while the others keep their results, in input order.
async fn collect_tolerant_group_discovery<I, F, T>(
    requests: I,
    max_concurrent_requests: usize,
    per_request_timeout: std::time::Duration,
    deadline: tokio::time::Instant,
) -> GroupDiscovery<T>
where
    I: IntoIterator<Item = (String, F)> + Send,
    I::IntoIter: Send,
    F: Future<Output = Result<T>> + Send,
    T: Send,
{
    let requests =
        requests
            .into_iter()
            .enumerate()
            .map(|(index, (group_version, request))| async move {
                let request_deadline =
                    (tokio::time::Instant::now() + per_request_timeout).min(deadline);
                let outcome = match tokio::time::timeout_at(request_deadline, request).await {
                    Ok(Ok(discovered)) => Ok(discovered),
                    Ok(Err(error)) => Err(format!("{error:#}")),
                    Err(_) => Err("timed out waiting for the API server to respond".to_owned()),
                };
                (index, group_version, outcome)
            });
    let requests = futures_util::stream::iter(requests).buffer_unordered(max_concurrent_requests);
    let mut outcomes = requests.collect::<Vec<_>>().await;
    outcomes.sort_unstable_by_key(|(index, ..)| *index);

    let mut discovery = GroupDiscovery {
        discovered: Vec::new(),
        failures: Vec::new(),
    };
    for (_, group_version, outcome) in outcomes {
        match outcome {
            Ok(discovered) => discovery.discovered.push(discovered),
            Err(error) => discovery.failures.push(FailedApiGroup {
                group_version,
                error,
            }),
        }
    }
    discovery
}

async fn await_discovery_with_timeout<T>(
    timeout: std::time::Duration,
    operation: &str,
    discovery: impl Future<Output = Result<T>> + Send,
) -> Result<T>
where
    T: Send,
{
    match tokio::time::timeout(timeout, discovery).await {
        Ok(result) => result,
        Err(_) => bail!("{operation} timed out after {} seconds", timeout.as_secs()),
    }
}

pub(crate) fn pod_metrics_api_available(api_resources: &[ApiResource]) -> bool {
    metrics_api_available(api_resources, "PodMetrics", "pods")
}

pub(crate) fn node_metrics_api_available(api_resources: &[ApiResource]) -> bool {
    metrics_api_available(api_resources, "NodeMetrics", "nodes")
}

pub(crate) fn metrics_api_available(api_resources: &[ApiResource], kind: &str, name: &str) -> bool {
    api_resources.iter().any(|resource| {
        resource.group == "metrics.k8s.io"
            && resource.version == "v1beta1"
            && resource.kind == kind
            && resource.name == name
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;
    use tokio::sync::Semaphore;

    const PER_REQUEST_TIMEOUT: Duration = Duration::from_millis(50);

    fn far_deadline() -> tokio::time::Instant {
        tokio::time::Instant::now() + Duration::from_secs(60)
    }

    fn labelled<F>(index: usize, request: F) -> (String, F) {
        (format!("group{index}/v1"), request)
    }

    #[tokio::test]
    async fn api_discovery_limits_concurrent_requests() {
        const REQUEST_COUNT: usize = 12;
        const MAX_CONCURRENT_REQUESTS: usize = 4;

        let active_requests = Arc::new(AtomicUsize::new(0));
        let maximum_active_requests = Arc::new(AtomicUsize::new(0));
        let started_requests = Arc::new(AtomicUsize::new(0));
        let completion_gate = Arc::new(Semaphore::new(0));

        let request_active_requests = Arc::clone(&active_requests);
        let request_maximum_active_requests = Arc::clone(&maximum_active_requests);
        let request_started_requests = Arc::clone(&started_requests);
        let request_completion_gate = Arc::clone(&completion_gate);
        let requests = (0..REQUEST_COUNT).map(move |index| {
            let active_requests = Arc::clone(&request_active_requests);
            let maximum_active_requests = Arc::clone(&request_maximum_active_requests);
            let started_requests = Arc::clone(&request_started_requests);
            let completion_gate = Arc::clone(&request_completion_gate);
            labelled(index, async move {
                let active = active_requests.fetch_add(1, Ordering::SeqCst) + 1;
                maximum_active_requests.fetch_max(active, Ordering::SeqCst);
                started_requests.fetch_add(1, Ordering::SeqCst);

                let permit = completion_gate
                    .acquire()
                    .await
                    .expect("completion gate should remain open");
                active_requests.fetch_sub(1, Ordering::SeqCst);
                drop(permit);
                Ok::<_, anyhow::Error>(())
            })
        });

        let collection = tokio::spawn(collect_tolerant_group_discovery(
            requests,
            MAX_CONCURRENT_REQUESTS,
            Duration::from_secs(30),
            far_deadline(),
        ));

        tokio::time::timeout(Duration::from_secs(1), async {
            while started_requests.load(Ordering::SeqCst) < MAX_CONCURRENT_REQUESTS {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("the first discovery request batch should start");
        tokio::time::sleep(Duration::from_millis(10)).await;

        assert_eq!(
            started_requests.load(Ordering::SeqCst),
            MAX_CONCURRENT_REQUESTS,
            "no later request should start before a running request completes"
        );

        completion_gate.add_permits(REQUEST_COUNT);
        let discovery = collection.await.expect("collection task should finish");
        assert!(discovery.failures.is_empty());
        assert_eq!(discovery.discovered.len(), REQUEST_COUNT);
        assert_eq!(
            maximum_active_requests.load(Ordering::SeqCst),
            MAX_CONCURRENT_REQUESTS
        );
    }

    #[tokio::test]
    async fn stalled_api_discovery_request_returns_a_timeout_error() {
        let error = await_discovery_with_timeout(
            Duration::from_millis(1),
            "listing test API resources",
            std::future::pending::<Result<()>>(),
        )
        .await
        .expect_err("a stalled discovery request should time out");

        let message = format!("{error:#}");
        assert!(message.contains("listing test API resources"), "{message}");
        assert!(message.contains("timed out"), "{message}");
    }

    #[tokio::test]
    async fn failing_and_stalled_groups_are_reported_while_healthy_groups_are_kept() {
        type Request = (
            String,
            Pin<Box<dyn Future<Output = Result<&'static str>> + Send>>,
        );
        let requests: Vec<Request> = vec![
            ("apps/v1".into(), Box::pin(async { Ok("apps") })),
            (
                "metrics.k8s.io/v1beta1".into(),
                Box::pin(async { bail!("the server is currently unable to handle the request") }),
            ),
            (
                "stuck.example.com/v1".into(),
                Box::pin(std::future::pending()),
            ),
            ("batch/v1".into(), Box::pin(async { Ok("batch") })),
        ];

        let discovery = tokio::time::timeout(
            Duration::from_secs(5),
            collect_tolerant_group_discovery(requests, 4, PER_REQUEST_TIMEOUT, far_deadline()),
        )
        .await
        .expect("a stalled group must not block discovery past its own timeout");

        assert_eq!(discovery.discovered, vec!["apps", "batch"]);
        let failures = discovery
            .failures
            .iter()
            .map(|failure| (failure.group_version.as_str(), failure.error.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(failures.len(), 2, "{failures:?}");
        assert_eq!(failures[0].0, "metrics.k8s.io/v1beta1");
        assert!(failures[0].1.contains("unable to handle"), "{failures:?}");
        assert_eq!(failures[1].0, "stuck.example.com/v1");
        assert!(failures[1].1.contains("timed out"), "{failures:?}");
    }

    #[tokio::test]
    async fn a_slow_group_does_not_consume_the_budget_of_groups_waiting_behind_it() {
        let requests = (0..4).map(|index| {
            labelled(index, async move {
                if index == 0 {
                    std::future::pending::<()>().await;
                }
                Ok::<_, anyhow::Error>(index)
            })
        });

        let discovery =
            collect_tolerant_group_discovery(requests, 1, PER_REQUEST_TIMEOUT, far_deadline())
                .await;

        assert_eq!(discovery.discovered, vec![1, 2, 3]);
        assert_eq!(discovery.failures.len(), 1);
        assert_eq!(discovery.failures[0].group_version, "group0/v1");
    }

    #[tokio::test]
    async fn a_pending_group_between_ready_groups_fails_alone_under_a_short_deadline() {
        type Request = (String, Pin<Box<dyn Future<Output = Result<usize>> + Send>>);
        let requests: Vec<Request> = vec![
            ("a/v1".into(), Box::pin(async { Ok(1) })),
            ("b/v1".into(), Box::pin(std::future::pending())),
            ("c/v1".into(), Box::pin(async { Ok(3) })),
        ];

        let discovery = collect_tolerant_group_discovery(
            requests,
            1,
            Duration::from_secs(60),
            tokio::time::Instant::now() + Duration::from_millis(100),
        )
        .await;

        assert_eq!(discovery.discovered, vec![1, 3]);
        assert_eq!(discovery.failures.len(), 1);
        assert_eq!(discovery.failures[0].group_version, "b/v1");
    }

    #[tokio::test]
    async fn failures_keep_input_order_when_requests_fail_out_of_order() {
        let requests = (0..4).map(|index| {
            labelled(index, async move {
                tokio::time::sleep(Duration::from_millis(((4 - index) * 10) as u64)).await;
                bail!("failure {index}") as Result<()>
            })
        });

        let discovery =
            collect_tolerant_group_discovery(requests, 4, Duration::from_secs(30), far_deadline())
                .await;

        assert_eq!(
            discovery
                .failures
                .iter()
                .map(|failure| failure.group_version.as_str())
                .collect::<Vec<_>>(),
            vec!["group0/v1", "group1/v1", "group2/v1", "group3/v1"]
        );
        assert_eq!(discovery.failures[2].error, "failure 2");
    }

    #[tokio::test]
    async fn groups_not_settled_by_the_shared_deadline_are_reported_as_failed() {
        let requests = (0..3).map(|index| labelled(index, std::future::pending::<Result<()>>()));

        let discovery = tokio::time::timeout(
            Duration::from_secs(5),
            collect_tolerant_group_discovery(
                requests,
                1,
                Duration::from_secs(60),
                tokio::time::Instant::now() + Duration::from_millis(30),
            ),
        )
        .await
        .expect("the shared deadline should bound every group, including queued ones");

        assert!(discovery.discovered.is_empty());
        assert_eq!(discovery.failures.len(), 3);
    }

    #[tokio::test]
    async fn api_discovery_preserves_input_order_when_requests_finish_out_of_order() {
        const REQUEST_COUNT: usize = 4;
        let requests = (0..REQUEST_COUNT).map(|index| {
            labelled(index, async move {
                tokio::time::sleep(Duration::from_millis(((REQUEST_COUNT - index) * 5) as u64))
                    .await;
                Ok::<_, anyhow::Error>(index)
            })
        });

        let discovery = collect_tolerant_group_discovery(
            requests,
            REQUEST_COUNT,
            Duration::from_secs(30),
            far_deadline(),
        )
        .await;

        assert_eq!(discovery.discovered, (0..REQUEST_COUNT).collect::<Vec<_>>());
    }

    enum ScriptedReply {
        Json(&'static str, &'static str),
        Hang,
    }

    const EMPTY_CRD_LIST: &str = r#"{"apiVersion":"apiextensions.k8s.io/v1","kind":"CustomResourceDefinitionList","metadata":{},"items":[]}"#;
    const CRD_PATH: &str = "/apis/apiextensions.k8s.io/v1/customresourcedefinitions";
    const API_FAILURE: &str = r#"{"kind":"Status","apiVersion":"v1","status":"Failure","message":"the server is currently unable to handle the request","reason":"ServiceUnavailable","code":503}"#;

    fn group_list(groups: &[(&str, &str)]) -> String {
        let groups = groups
            .iter()
            .map(|(name, version)| {
                let group_version = format!(r#"{{"groupVersion":"{name}/{version}","version":"{version}"}}"#);
                format!(
                    r#"{{"name":"{name}","versions":[{group_version}],"preferredVersion":{group_version}}}"#
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        format!(r#"{{"kind":"APIGroupList","apiVersion":"v1","groups":[{groups}]}}"#)
    }

    fn resource_list(group_version: &str, resources: &[(&str, &str)]) -> String {
        let resources = resources
            .iter()
            .map(|(name, kind)| {
                format!(
                    r#"{{"name":"{name}","singularName":"","namespaced":true,"kind":"{kind}","verbs":["list"]}}"#
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        format!(
            r#"{{"kind":"APIResourceList","apiVersion":"v1","groupVersion":"{group_version}","resources":[{resources}]}}"#
        )
    }

    /// Serves the scripted replies by request path until the returned task is dropped.
    async fn scripted_api_server(
        replies: Vec<(&'static str, ScriptedReply)>,
    ) -> (kube::Client, tokio::task::JoinHandle<()>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test server should bind");
        let address = listener.local_addr().expect("test server has an address");
        let replies = Arc::new(replies);
        let server = tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let replies = Arc::clone(&replies);
                tokio::spawn(async move {
                    let mut request = Vec::new();
                    let mut buffer = [0_u8; 1_024];
                    while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                        match socket.read(&mut buffer).await {
                            Ok(0) | Err(_) => return,
                            Ok(read) => request.extend_from_slice(&buffer[..read]),
                        }
                    }
                    let request = String::from_utf8_lossy(&request);
                    let path = request
                        .split_whitespace()
                        .nth(1)
                        .and_then(|target| target.split('?').next())
                        .unwrap_or_default()
                        .to_owned();
                    let (status, body) = match replies.iter().find(|(route, _)| *route == path) {
                        Some((_, ScriptedReply::Json(status, body))) => (*status, *body),
                        Some((_, ScriptedReply::Hang)) => {
                            std::future::pending::<()>().await;
                            return;
                        }
                        None => ("404 Not Found", API_FAILURE),
                    };
                    let response = format!(
                        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                    let _ = socket.shutdown().await;
                });
            }
        });
        let config = kube::Config::new(
            format!("http://{address}")
                .parse()
                .expect("test server URL is valid"),
        );
        let client = kube::Client::try_from(config).expect("test client should build");
        (client, server)
    }

    fn leak(text: String) -> &'static str {
        Box::leak(text.into_boxed_str())
    }

    const CORE_VERSIONS: &str = r#"{"kind":"APIVersions","versions":["v1"]}"#;

    fn healthy_replies() -> Vec<(&'static str, ScriptedReply)> {
        vec![
            ("/api", ScriptedReply::Json("200 OK", CORE_VERSIONS)),
            (
                "/api/v1",
                ScriptedReply::Json("200 OK", leak(resource_list("v1", &[("pods", "Pod")]))),
            ),
            (
                "/apis",
                ScriptedReply::Json(
                    "200 OK",
                    leak(group_list(&[("apps", "v1"), ("metrics.k8s.io", "v1beta1")])),
                ),
            ),
            (
                "/apis/apps/v1",
                ScriptedReply::Json(
                    "200 OK",
                    leak(resource_list("apps/v1", &[("deployments", "Deployment")])),
                ),
            ),
            (CRD_PATH, ScriptedReply::Json("200 OK", EMPTY_CRD_LIST)),
        ]
    }

    fn with_reply(
        mut replies: Vec<(&'static str, ScriptedReply)>,
        path: &'static str,
        reply: ScriptedReply,
    ) -> Vec<(&'static str, ScriptedReply)> {
        replies.retain(|(route, _)| *route != path);
        replies.push((path, reply));
        replies
    }

    #[tokio::test]
    async fn inspection_fails_when_the_core_api_cannot_be_listed() {
        let replies = with_reply(
            healthy_replies(),
            "/api",
            ScriptedReply::Json("500 Internal Server Error", API_FAILURE),
        );
        let (client, _server) = scripted_api_server(replies).await;

        let result = (KubernetesApiInspector { client }).inspect_api().await;

        assert!(result.is_err(), "the core API is required");
    }

    #[tokio::test]
    async fn inspection_fails_when_the_api_group_list_cannot_be_listed() {
        let replies = with_reply(
            healthy_replies(),
            "/apis",
            ScriptedReply::Json("500 Internal Server Error", API_FAILURE),
        );
        let (client, _server) = scripted_api_server(replies).await;

        let result = (KubernetesApiInspector { client }).inspect_api().await;

        assert!(result.is_err(), "the API group list is required");
    }

    #[tokio::test]
    async fn inspection_keeps_healthy_groups_when_the_metrics_group_is_unavailable() {
        let replies = with_reply(
            healthy_replies(),
            "/apis/metrics.k8s.io/v1beta1",
            ScriptedReply::Json("503 Service Unavailable", API_FAILURE),
        );
        let (client, _server) = scripted_api_server(replies).await;

        let inspection = (KubernetesApiInspector { client })
            .inspect_api()
            .await
            .expect("a failing aggregated API must not fail discovery");

        let names = inspection
            .api_resources
            .iter()
            .map(|resource| resource.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["pods", "deployments"]);
        assert_eq!(inspection.failed_api_groups.len(), 1);
        assert_eq!(
            inspection.failed_api_groups[0].group_version,
            "metrics.k8s.io/v1beta1"
        );
        assert!(!inspection.pod_metrics_api_available);
        assert!(!inspection.node_metrics_api_available);
    }

    #[tokio::test]
    async fn stalled_group_and_crd_listing_degrade_instead_of_failing_the_inspection() {
        let replies = with_reply(
            with_reply(
                healthy_replies(),
                "/apis/metrics.k8s.io/v1beta1",
                ScriptedReply::Hang,
            ),
            CRD_PATH,
            ScriptedReply::Hang,
        );
        let (client, _server) = scripted_api_server(replies).await;
        let timeouts = DiscoveryTimeouts {
            total: Duration::from_millis(1_500),
            group: Duration::from_millis(200),
            crd_metadata: Duration::from_millis(300),
        };

        let started = std::time::Instant::now();
        let inspection = (KubernetesApiInspector { client })
            .inspect_api_with_timeouts(timeouts)
            .await
            .expect("slow optional discovery must not fail the inspection");

        assert!(started.elapsed() < timeouts.total + Duration::from_millis(500));
        assert_eq!(inspection.api_resources.len(), 2);
        assert_eq!(inspection.failed_api_groups.len(), 1);
        assert!(inspection.custom_resource_columns.is_empty());
        assert!(inspection.resource_schemas.is_empty());
    }
}
