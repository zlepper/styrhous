use super::*;

const MAX_CONCURRENT_API_DISCOVERY_REQUESTS: usize = 4;
const API_DISCOVERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(25);

pub(crate) struct KubernetesApiInspector {
    pub(crate) client: kube::Client,
}

pub(crate) struct ApiInspection {
    pub(crate) api_resources: Vec<ApiResource>,
    pub(crate) scalable_api_resources: BTreeSet<ApiResource>,
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
        await_discovery_with_timeout(
            API_DISCOVERY_TIMEOUT,
            "Kubernetes API discovery",
            self.inspect_api_without_timeout(),
        )
        .await
    }

    async fn inspect_api_without_timeout(&self) -> Result<ApiInspection> {
        let api_groups = self.client.list_api_groups().await?;

        let client = self.client.clone();
        let tasks = api_groups.groups.into_iter().map(move |api_group| {
            let versions = api_group
                .preferred_version
                .clone()
                .map(|v| vec![v])
                .unwrap_or_else(|| api_group.versions.clone());
            let inspector = KubernetesApiInspector {
                client: client.clone(),
            };

            async move {
                inspector
                    .get_api_resources_for_group_versions(api_group, versions)
                    .await
            }
        });

        let core_resources = self.get_core_api_resources().await?;

        let discovered_resources =
            collect_bounded_discovery_results(tasks, MAX_CONCURRENT_API_DISCOVERY_REQUESTS)
                .await?
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
        let (custom_resource_columns, resource_schemas) = self.custom_resource_metadata().await;
        Ok(ApiInspection {
            api_resources: discovered_resources.api_resources,
            scalable_api_resources: discovered_resources.scalable_api_resources,
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

async fn collect_bounded_discovery_results<I, F, T, E>(
    requests: I,
    max_concurrent_requests: usize,
) -> Result<Vec<T>>
where
    I: IntoIterator<Item = F> + Send,
    I::IntoIter: Send,
    F: Future<Output = std::result::Result<T, E>> + Send,
    T: Send,
    E: Into<anyhow::Error> + Send,
{
    let requests = requests
        .into_iter()
        .enumerate()
        .map(|(index, request)| async move {
            request
                .await
                .map(|result| (index, result))
                .map_err(Into::into)
        });
    let requests = futures_util::stream::iter(requests).buffer_unordered(max_concurrent_requests);
    pin_mut!(requests);

    let mut indexed_results = Vec::new();
    while let Some(result) = requests.next().await {
        indexed_results.push(result?);
    }
    indexed_results.sort_unstable_by_key(|(index, _)| *index);
    Ok(indexed_results
        .into_iter()
        .map(|(_, result)| result)
        .collect())
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
        let requests = (0..REQUEST_COUNT).map(move |_| {
            let active_requests = Arc::clone(&request_active_requests);
            let maximum_active_requests = Arc::clone(&request_maximum_active_requests);
            let started_requests = Arc::clone(&request_started_requests);
            let completion_gate = Arc::clone(&request_completion_gate);
            async move {
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
            }
        });

        let collection = tokio::spawn(collect_bounded_discovery_results(
            requests,
            MAX_CONCURRENT_REQUESTS,
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
        collection
            .await
            .expect("collection task should finish")
            .expect("discovery requests should succeed");
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
    async fn api_discovery_surfaces_a_later_error_without_waiting_for_an_earlier_request() {
        let requests: Vec<Pin<Box<dyn Future<Output = Result<()>> + Send>>> = vec![
            Box::pin(std::future::pending()),
            Box::pin(async { bail!("API discovery was forbidden") }),
        ];

        let error = tokio::time::timeout(
            Duration::from_millis(100),
            collect_bounded_discovery_results(requests, 2),
        )
        .await
        .expect("a concrete API error should not wait for an earlier stalled request")
        .expect_err("API discovery should fail");

        assert!(
            format!("{error:#}").contains("API discovery was forbidden"),
            "{error:#}"
        );
    }

    #[tokio::test]
    async fn api_discovery_preserves_input_order_when_requests_finish_out_of_order() {
        const REQUEST_COUNT: usize = 4;
        let requests = (0..REQUEST_COUNT).map(|index| async move {
            tokio::time::sleep(Duration::from_millis(((REQUEST_COUNT - index) * 5) as u64)).await;
            Ok::<_, anyhow::Error>(index)
        });

        let results = collect_bounded_discovery_results(requests, REQUEST_COUNT)
            .await
            .expect("discovery requests should succeed");

        assert_eq!(results, (0..REQUEST_COUNT).collect::<Vec<_>>());
    }
}
