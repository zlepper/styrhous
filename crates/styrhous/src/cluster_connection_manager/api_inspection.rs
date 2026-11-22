use super::*;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{APIGroupList, APIVersions};
use kube::client::{APIGroupDiscoveryList, APIResourceDiscovery};
use kube::core::discovery::v2::ACCEPT_AGGREGATED_DISCOVERY_V2;

/// Asks for the legacy discovery documents only.
const ACCEPT_LEGACY_DISCOVERY: &str = "application/json";
use kube::runtime::watcher::DefaultBackoff;

/// Legacy discovery asks for each group separately. The requests are small, so many can be
/// in flight at once without loading the API server.
const MAX_CONCURRENT_API_DISCOVERY_REQUESTS: usize = 16;
/// A custom resource table waits for its printer columns before showing rows, so a CRD
/// lookup that stops answering only costs its optional columns.
const CRD_COLUMNS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Time limits for one discovery run.
#[derive(Clone, Copy)]
struct DiscoveryTimeouts {
    /// Bounds each discovery request: the core API, the group list and every group.
    request: std::time::Duration,
    /// Bounds legacy per-group discovery as a whole, so a slow group list cannot keep the
    /// remaining groups pending indefinitely.
    groups: std::time::Duration,
}

const DISCOVERY_TIMEOUTS: DiscoveryTimeouts = DiscoveryTimeouts {
    // Healthy groups stalled for over 10s were observed on a loaded local cluster, so a
    // group is only dropped once it has clearly stopped answering. A stalled group
    // occupies one of the concurrent request slots without blocking the others.
    request: std::time::Duration::from_secs(15),
    groups: std::time::Duration::from_secs(20),
};

const _: () =
    assert!(DISCOVERY_TIMEOUTS.request.as_nanos() <= DISCOVERY_TIMEOUTS.groups.as_nanos());

#[derive(Clone)]
pub(crate) struct KubernetesApiInspector {
    pub(crate) client: kube::Client,
}

/// One API group to discover: its name and the versions whose resources are listed.
/// `group_version` labels it the way failures are reported (its first version).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ApiGroupTarget {
    pub(crate) group_version: String,
    pub(crate) group_name: String,
    pub(crate) versions: Vec<GroupVersionForDiscovery>,
}

impl ApiGroupTarget {
    fn from_group(api_group: APIGroup) -> Self {
        let versions = api_group
            .preferred_version
            .clone()
            .map(|v| vec![v])
            .unwrap_or_else(|| api_group.versions.clone());
        let group_version = versions
            .first()
            .map_or_else(|| api_group.name.clone(), |v| v.group_version.clone());
        Self {
            group_version,
            group_name: api_group.name,
            versions,
        }
    }

    fn single_version(group_name: &str, version: &str) -> Self {
        let group_version = format!("{group_name}/{version}");
        Self {
            group_version: group_version.clone(),
            group_name: group_name.to_owned(),
            versions: vec![GroupVersionForDiscovery {
                group_version,
                version: version.to_owned(),
            }],
        }
    }
}

/// The API groups, as `/apis` described them.
enum GroupListing {
    /// Aggregated discovery: every group's resources, the group versions they came from,
    /// and the groups whose aggregated API is currently unavailable.
    Aggregated {
        discovered: ApiResourceSet,
        group_versions: Vec<String>,
        stale: Vec<ApiGroupTarget>,
    },
    /// Legacy discovery: each group has to be asked for its resources.
    Legacy(Vec<ApiGroupTarget>),
}

/// What discovering the groups of one listing found.
#[derive(Default)]
struct GroupDiscovery {
    /// The group versions whose resources were reported.
    discovered: Vec<String>,
    /// The groups that failed, with why.
    failed: Vec<(ApiGroupTarget, String)>,
}

impl GroupDiscovery {
    fn failures(&self) -> Vec<FailedApiGroup> {
        let mut failures = self
            .failed
            .iter()
            .map(|(target, error)| FailedApiGroup {
                group_version: target.group_version.clone(),
                error: error.clone(),
            })
            .collect::<Vec<_>>();
        failures.sort_by(|a, b| a.group_version.cmp(&b.group_version));
        failures
    }

    fn retry_targets(self) -> Vec<ApiGroupTarget> {
        self.failed.into_iter().map(|(target, _)| target).collect()
    }
}

/// How a failed group list is reported in place of a group version.
const GROUP_LIST_FAILURE: &str = "API group list";

/// Discovers the cluster's API resources and reports them to the UI as they arrive.
///
/// `/api` and `/apis` are requested concurrently with aggregated discovery, which answers
/// with every resource at once; older servers answer the same requests with the legacy
/// lists, so falling back costs no extra round trip. The core API also proves that the
/// cluster accepts the credentials: if it fails, the connection has failed. Groups that
/// fail are reported in `KubernetesApiDiscoveryCompleted` and retried in the background,
/// and so is the group list itself when it cannot be listed in time.
pub(crate) async fn discover_cluster_api(
    cluster_key: i32,
    client: kube::Client,
    events: WorkerResultSender,
) {
    discover_cluster_api_with(
        cluster_key,
        client,
        events,
        DISCOVERY_TIMEOUTS,
        DefaultBackoff::default(),
        DefaultBackoff::default(),
    )
    .await;
}

/// `discover_cluster_api` with its time limits and retry schedules supplied.
async fn discover_cluster_api_with(
    cluster_key: i32,
    client: kube::Client,
    events: WorkerResultSender,
    timeouts: DiscoveryTimeouts,
    mut group_list_delays: impl Iterator<Item = std::time::Duration>,
    group_retry_delays: impl Iterator<Item = std::time::Duration>,
) {
    let inspector = KubernetesApiInspector { client };
    let core = async {
        match inspector.discover_core(timeouts).await {
            Ok(core) => {
                send_discovered(cluster_key, &events, core).await;
                true
            }
            Err(error) => {
                events
                    .send(ClusterConnectionFailed {
                        cluster_key,
                        error: format!("{error:#}"),
                    })
                    .await
                    .log_if_error("Failed to send the core API discovery failure");
                false
            }
        }
    };
    let groups = inspector.discover_groups(timeouts, &mut group_list_delays, |discovered| {
        send_discovered(cluster_key, &events, discovered)
    });

    let groups = {
        let mut core = std::pin::pin!(core);
        let mut groups = std::pin::pin!(groups);
        let mut group_outcome = None;
        // A failed core API ends the connection, so stop discovering groups as well.
        let core_succeeded = loop {
            tokio::select! {
                succeeded = &mut core => break succeeded,
                outcome = &mut groups, if group_outcome.is_none() => group_outcome = Some(outcome),
            }
        };
        if !core_succeeded {
            return;
        }
        match group_outcome {
            Some(outcome) => outcome,
            None => groups.await,
        }
    };
    let (failed_api_groups, group_list_failed) = match &groups {
        Ok(discovery) => (discovery.failures(), false),
        Err(error) => (
            vec![FailedApiGroup {
                group_version: GROUP_LIST_FAILURE.to_owned(),
                error: format!("{error:#}"),
            }],
            true,
        ),
    };
    for failure in &failed_api_groups {
        tracing::warn!(
            group_version = %failure.group_version,
            error = %failure.error,
            "API group discovery failed; continuing without it"
        );
    }
    if events
        .send(KubernetesApiDiscoveryCompleted {
            cluster_key,
            failed_api_groups,
            group_list_failed,
        })
        .await
        .is_err()
    {
        return;
    }
    let retry_targets = match groups {
        Ok(discovery) => discovery.retry_targets(),
        Err(_) => {
            let Some(targets) = inspector
                .recover_group_list(cluster_key, &events, timeouts, group_list_delays)
                .await
            else {
                return;
            };
            targets
        }
    };
    if !retry_targets.is_empty() {
        retry_cluster_api_groups(
            cluster_key,
            inspector.client,
            events,
            retry_targets,
            group_retry_delays,
        )
        .await;
    }
}

async fn send_discovered(cluster_key: i32, events: &WorkerResultSender, resources: ApiResourceSet) {
    if resources.api_resources.is_empty() {
        return;
    }
    events
        .send(KubernetesApiResourcesDiscovered {
            cluster_key,
            resources,
        })
        .await
        .log_if_error("Failed to send discovered API resources");
}

/// `future`, or an error naming `what` once `timeout` has passed.
pub(super) async fn within<T>(
    timeout: std::time::Duration,
    what: &str,
    future: impl Future<Output = Result<T>>,
) -> Result<T> {
    match tokio::time::timeout(timeout, future).await {
        Ok(result) => result,
        Err(_) => bail!("timed out waiting for the API server to answer {what}"),
    }
}

impl KubernetesApiInspector {
    /// GETs a discovery document with the given `Accept` header.
    async fn discovery_document(
        &self,
        path: &str,
        accept: &str,
        timeout: std::time::Duration,
    ) -> Result<k8s_openapi::serde_json::Value> {
        let request = Request::get(path)
            .header(http::header::ACCEPT, accept)
            .body(Vec::new())?;
        within(timeout, path, async {
            Ok(self.client.request(request).await?)
        })
        .await
    }

    /// The discovery document at `path`, preferring the aggregated form. A document that
    /// `parse` cannot read (for example a proxy that answered with something else) is
    /// asked for once more in the legacy form only.
    async fn discovery<T>(
        &self,
        path: &str,
        timeout: std::time::Duration,
        parse: impl Fn(k8s_openapi::serde_json::Value) -> Result<T>,
    ) -> Result<T> {
        let document = match self
            .discovery_document(path, ACCEPT_AGGREGATED_DISCOVERY_V2, timeout)
            .await
        {
            Ok(document) => document,
            // A proxy may refuse the aggregated media type outright.
            Err(error) if refuses_media_type(&error) => {
                return parse(
                    self.discovery_document(path, ACCEPT_LEGACY_DISCOVERY, timeout)
                        .await?,
                );
            }
            Err(error) => return Err(error),
        };
        match parse(document) {
            Ok(parsed) => Ok(parsed),
            Err(error) => {
                tracing::debug!(%path, error = %format!("{error:#}"), "unusable discovery document; asking for the legacy form");
                parse(
                    self.discovery_document(path, ACCEPT_LEGACY_DISCOVERY, timeout)
                        .await?,
                )
            }
        }
    }

    async fn discover_core(&self, timeouts: DiscoveryTimeouts) -> Result<ApiResourceSet> {
        let listing = self
            .discovery("/api", timeouts.request, parse_core_listing)
            .await
            .with_context(|| "Error discovering the core Kubernetes API")?;
        let versions = match listing {
            CoreListing::Aggregated(resources) => return Ok(resources),
            CoreListing::Legacy(versions) => versions,
        };
        let lists = futures_util::future::try_join_all(versions.versions.iter().map(
            |version| async move {
                let resources = within(timeouts.request, version, async {
                    Ok(self.client.list_core_api_resources(version).await?)
                })
                .await?;
                anyhow::Ok((version, resources))
            },
        ))
        .await
        .with_context(|| "Error discovering the core Kubernetes API")?;
        let mut discovered = ApiResourceSet::default();
        for (version, resources) in lists {
            legacy_resources(&mut discovered, "core", version, &resources.resources);
        }
        Ok(discovered)
    }

    /// Lists the API groups, retrying the list on the supplied delays for as long as the
    /// groups may take, then reports every group's resources through `publish` as they
    /// arrive. Fails with the last error when the groups could not be listed in time.
    async fn discover_groups<Publish>(
        &self,
        timeouts: DiscoveryTimeouts,
        retry_delays: &mut impl Iterator<Item = std::time::Duration>,
        publish: impl FnMut(ApiResourceSet) -> Publish,
    ) -> Result<GroupDiscovery>
    where
        Publish: Future<Output = ()>,
    {
        let deadline = tokio::time::Instant::now() + timeouts.groups;
        let listing = loop {
            match self.list_groups(timeouts).await {
                Ok(listing) => break listing,
                Err(error) => match retry_delays.next() {
                    Some(delay) if tokio::time::Instant::now() + delay < deadline => {
                        tracing::warn!(error = %format!("{error:#}"), "listing the API groups failed; retrying");
                        tokio::time::sleep(delay).await;
                    }
                    _ => return Err(error),
                },
            }
        };
        Ok(self
            .discover_listed_groups(listing, timeouts, publish)
            .await)
    }

    /// Keeps listing the API groups after discovery gave up on them, and reports every
    /// group's resources at once when the list answers. Returns the groups that still need
    /// retrying, or `None` when nobody listens any more or the delays ran out.
    async fn recover_group_list(
        &self,
        cluster_key: i32,
        events: &WorkerResultSender,
        timeouts: DiscoveryTimeouts,
        mut delays: impl Iterator<Item = std::time::Duration>,
    ) -> Option<Vec<ApiGroupTarget>> {
        loop {
            tokio::time::sleep(delays.next()?).await;
            let listing = match self.list_groups(timeouts).await {
                Ok(listing) => listing,
                Err(error) => {
                    tracing::debug!(error = %format!("{error:#}"), "listing the API groups failed again");
                    continue;
                }
            };
            let mut resources = ApiResourceSet::default();
            let discovery = self
                .discover_listed_groups(listing, timeouts, |discovered| {
                    resources.extend(discovered);
                    std::future::ready(())
                })
                .await;
            let recovered = RecoveredApiGroups {
                group_versions: discovery.discovered.clone(),
                resources,
                still_failing: discovery
                    .failed
                    .iter()
                    .map(|(target, _)| target.group_version.clone())
                    .collect(),
            };
            if !send_recovered(cluster_key, events, recovered).await {
                return None;
            }
            return Some(discovery.retry_targets());
        }
    }

    async fn discover_listed_groups<Publish>(
        &self,
        listing: GroupListing,
        timeouts: DiscoveryTimeouts,
        mut publish: impl FnMut(ApiResourceSet) -> Publish,
    ) -> GroupDiscovery
    where
        Publish: Future<Output = ()>,
    {
        match listing {
            GroupListing::Aggregated {
                discovered,
                group_versions,
                stale,
            } => {
                publish(discovered).await;
                GroupDiscovery {
                    discovered: group_versions,
                    failed: stale
                        .into_iter()
                        .map(|target| {
                            (
                                target,
                                "the aggregated API is currently unavailable".to_owned(),
                            )
                        })
                        .collect(),
                }
            }
            GroupListing::Legacy(targets) => {
                let requests = targets.into_iter().map(|target| {
                    // Owned, so the requests do not borrow `self` across the stream.
                    let inspector = self.clone();
                    let label = target.clone();
                    (label, async move {
                        inspector
                            .get_api_resources_for_group_versions(&target)
                            .await
                    })
                });
                let outcomes = tolerant_group_discovery(
                    requests,
                    MAX_CONCURRENT_API_DISCOVERY_REQUESTS,
                    timeouts.request,
                    tokio::time::Instant::now() + timeouts.groups,
                );
                let mut outcomes = std::pin::pin!(outcomes);
                let mut discovery = GroupDiscovery::default();
                while let Some((target, outcome)) = outcomes.next().await {
                    match outcome {
                        Ok(resources) => {
                            discovery.discovered.push(target.group_version);
                            publish(resources).await;
                        }
                        Err(error) => discovery.failed.push((target, error)),
                    }
                }
                discovery
            }
        }
    }

    async fn list_groups(&self, timeouts: DiscoveryTimeouts) -> Result<GroupListing> {
        self.discovery("/apis", timeouts.request, parse_group_listing)
            .await
            .with_context(|| "Error listing the Kubernetes API groups")
    }

    pub(super) async fn get_api_resources_for_group_versions(
        &self,
        target: &ApiGroupTarget,
    ) -> Result<ApiResourceSet> {
        let lists = futures_util::future::try_join_all(
            target
                .versions
                .iter()
                .map(|version| self.client.list_api_group_resources(&version.group_version)),
        )
        .await?;
        let mut discovered = ApiResourceSet::default();
        for (resources, version) in lists.iter().zip(&target.versions) {
            legacy_resources(
                &mut discovered,
                &target.group_name,
                &version.version,
                &resources.resources,
            );
        }
        Ok(discovered)
    }
}

/// The core API, as `/api` described it.
enum CoreListing {
    /// Aggregated discovery: every core resource.
    Aggregated(ApiResourceSet),
    /// Legacy discovery: the versions whose resources have to be listed.
    Legacy(APIVersions),
}

/// A legacy document is recognised by its kind, or by having none: the typed client used
/// to accept such documents, and some proxies strip the field.
fn parse_core_listing(document: k8s_openapi::serde_json::Value) -> Result<CoreListing> {
    match document_kind(&document) {
        "APIGroupDiscoveryList" => {
            let list: APIGroupDiscoveryList = k8s_openapi::serde_json::from_value(document)?;
            let mut discovered = ApiResourceSet::default();
            for group in list.items {
                for version in group.versions {
                    let version_name = version.version.unwrap_or_default();
                    aggregated_resources(&mut discovered, "core", &version_name, version.resources);
                }
            }
            Ok(CoreListing::Aggregated(discovered))
        }
        "APIVersions" | "" => Ok(CoreListing::Legacy(k8s_openapi::serde_json::from_value(
            document,
        )?)),
        kind => bail!("unexpected core API discovery document {kind:?}"),
    }
}

fn parse_group_listing(document: k8s_openapi::serde_json::Value) -> Result<GroupListing> {
    match document_kind(&document) {
        "APIGroupDiscoveryList" => {
            let list: APIGroupDiscoveryList = k8s_openapi::serde_json::from_value(document)?;
            let mut discovered = ApiResourceSet::default();
            let mut group_versions = Vec::new();
            let mut stale = Vec::new();
            for group in list.items {
                let group_name = group
                    .metadata
                    .and_then(|metadata| metadata.name)
                    .unwrap_or_default();
                // Versions are listed in preference order, like legacy discovery's
                // preferred version.
                let Some(version) = group.versions.into_iter().next() else {
                    continue;
                };
                let version_name = version.version.unwrap_or_default();
                if version.freshness.as_deref() == Some("Stale") {
                    stale.push(ApiGroupTarget::single_version(&group_name, &version_name));
                    continue;
                }
                group_versions.push(format!("{group_name}/{version_name}"));
                aggregated_resources(
                    &mut discovered,
                    &group_name,
                    &version_name,
                    version.resources,
                );
            }
            Ok(GroupListing::Aggregated {
                discovered,
                group_versions,
                stale,
            })
        }
        "APIGroupList" | "" => {
            let list: APIGroupList = k8s_openapi::serde_json::from_value(document)?;
            Ok(GroupListing::Legacy(
                list.groups
                    .into_iter()
                    .map(ApiGroupTarget::from_group)
                    .collect(),
            ))
        }
        kind => bail!("unexpected API group discovery document {kind:?}"),
    }
}

/// Whether a request failed because the server would not answer in the requested form.
fn refuses_media_type(error: &anyhow::Error) -> bool {
    matches!(
        error.downcast_ref::<kube::Error>(),
        Some(kube::Error::Api(status)) if matches!(status.code, 406 | 415)
    )
}

fn document_kind(document: &k8s_openapi::serde_json::Value) -> &str {
    document
        .get("kind")
        .and_then(k8s_openapi::serde_json::Value::as_str)
        .unwrap_or_default()
}

fn aggregated_resources(
    discovered: &mut ApiResourceSet,
    group: &str,
    version: &str,
    resources: Vec<APIResourceDiscovery>,
) {
    for resource in resources {
        let (Some(name), Some(kind)) = (
            resource.resource,
            resource.response_kind.and_then(|kind| kind.kind),
        ) else {
            continue;
        };
        let scalable = resource.subresources.iter().any(|subresource| {
            subresource.subresource.as_deref() == Some("scale")
                && allows_scaling(&subresource.verbs)
        });
        discovered.push(
            ApiResource {
                group: group.to_owned(),
                version: version.to_owned(),
                kind,
                name,
                namespaced: resource.scope.as_deref() == Some("Namespaced"),
            },
            scalable,
        );
    }
}

fn legacy_resources(
    discovered: &mut ApiResourceSet,
    group: &str,
    version: &str,
    resources: &[k8s_openapi::apimachinery::pkg::apis::meta::v1::APIResource],
) {
    for resource in resources {
        // Subresources such as "pods/status" and "deployments/scale" are not listed.
        if resource.name.contains('/') {
            continue;
        }
        discovered.push(
            ApiResource {
                group: group.to_owned(),
                version: version.to_owned(),
                kind: resource.kind.clone(),
                name: resource.name.clone(),
                namespaced: resource.namespaced,
            },
            supports_scale_subresource(resources, &resource.name),
        );
    }
}

/// The printer columns a CustomResourceDefinition declares for `api_resource`'s version.
///
/// Reads only that one CRD, by its `<plural>.<group>` name. A resource that is not a CRD
/// (built-in or served by an aggregated API), missing permission, or a failing lookup all
/// mean the table shows no custom columns.
pub(crate) async fn crd_printer_columns(
    client: kube::Client,
    api_resource: &ApiResource,
) -> Vec<CustomResourceColumn> {
    // Every CRD group contains a dot; the dotless groups are all built in.
    if !api_resource.group.contains('.') {
        return Vec::new();
    }
    let name = format!("{}.{}", api_resource.name, api_resource.group);
    let crds = Api::<CustomResourceDefinition>::all(client);
    let crd = match tokio::time::timeout(CRD_COLUMNS_TIMEOUT, crds.get_opt(&name)).await {
        Ok(Ok(crd)) => crd,
        Ok(Err(kube::Error::Api(status))) if matches!(status.code, 401 | 403) => None,
        Ok(Err(error)) => {
            tracing::warn!(%error, crd = %name, "reading the CustomResourceDefinition failed");
            None
        }
        Err(_) => {
            tracing::warn!(crd = %name, "reading the CustomResourceDefinition timed out");
            None
        }
    };
    crd.into_iter()
        .flat_map(|crd| crd.spec.versions)
        .find(|version| version.name == api_resource.version)
        .and_then(|version| version.additional_printer_columns)
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .map(|(index, column)| CustomResourceColumn {
            id: format!("crd-{index}"),
            label: column.name,
            json_path: column.json_path,
            type_: column.type_,
            format: column.format,
        })
        .collect()
}

/// Runs the group requests with bounded concurrency and yields each outcome, with its
/// request's label, as it settles. A request that fails or exceeds `per_request_timeout` (or the shared
/// `deadline`) yields its error.
fn tolerant_group_discovery<I, L, F, T>(
    requests: I,
    max_concurrent_requests: usize,
    per_request_timeout: std::time::Duration,
    deadline: tokio::time::Instant,
) -> impl futures_util::Stream<Item = (L, std::result::Result<T, String>)>
where
    I: IntoIterator<Item = (L, F)>,
    F: Future<Output = Result<T>>,
{
    futures_util::stream::iter(
        requests
            .into_iter()
            .map(move |(label, request)| async move {
                let request_deadline =
                    (tokio::time::Instant::now() + per_request_timeout).min(deadline);
                let outcome = match tokio::time::timeout_at(request_deadline, request).await {
                    Ok(Ok(discovered)) => Ok(discovered),
                    Ok(Err(error)) => Err(format!("{error:#}")),
                    Err(_) => Err("timed out waiting for the API server to respond".to_owned()),
                };
                (label, outcome)
            }),
    )
    .buffer_unordered(max_concurrent_requests)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;
    use tokio::sync::Semaphore;

    const PER_REQUEST_TIMEOUT: Duration = Duration::from_millis(50);
    const FAST_TIMEOUTS: DiscoveryTimeouts = DiscoveryTimeouts {
        request: Duration::from_millis(300),
        groups: Duration::from_millis(600),
    };

    fn far_deadline() -> tokio::time::Instant {
        tokio::time::Instant::now() + Duration::from_secs(60)
    }

    fn labelled<F>(index: usize, request: F) -> (String, F) {
        (format!("group{index}/v1"), request)
    }

    async fn collect_outcomes<T>(
        outcomes: impl futures_util::Stream<Item = (String, std::result::Result<T, String>)>,
    ) -> (Vec<T>, Vec<(String, String)>) {
        let mut discovered = Vec::new();
        let mut failures = Vec::new();
        let mut outcomes = std::pin::pin!(outcomes);
        while let Some((group_version, outcome)) = outcomes.next().await {
            match outcome {
                Ok(value) => discovered.push(value),
                Err(error) => failures.push((group_version, error)),
            }
        }
        (discovered, failures)
    }

    #[tokio::test]
    async fn api_discovery_limits_concurrent_requests() {
        const REQUEST_COUNT: usize = 12;
        const MAX_CONCURRENT_REQUESTS: usize = 4;

        let active_requests = Arc::new(AtomicUsize::new(0));
        let maximum_active_requests = Arc::new(AtomicUsize::new(0));
        let started_requests = Arc::new(AtomicUsize::new(0));
        let completion_gate = Arc::new(Semaphore::new(0));

        let requests = (0..REQUEST_COUNT).map({
            let active_requests = Arc::clone(&active_requests);
            let maximum_active_requests = Arc::clone(&maximum_active_requests);
            let started_requests = Arc::clone(&started_requests);
            let completion_gate = Arc::clone(&completion_gate);
            move |index| {
                let active_requests = Arc::clone(&active_requests);
                let maximum_active_requests = Arc::clone(&maximum_active_requests);
                let started_requests = Arc::clone(&started_requests);
                let completion_gate = Arc::clone(&completion_gate);
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
            }
        });

        let collection = tokio::spawn(collect_outcomes(tolerant_group_discovery(
            requests,
            MAX_CONCURRENT_REQUESTS,
            Duration::from_secs(30),
            far_deadline(),
        )));

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
        let (discovered, failures) = collection.await.expect("collection task should finish");
        assert!(failures.is_empty());
        assert_eq!(discovered.len(), REQUEST_COUNT);
        assert_eq!(
            maximum_active_requests.load(Ordering::SeqCst),
            MAX_CONCURRENT_REQUESTS
        );
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

        let (mut discovered, mut failures) = tokio::time::timeout(
            Duration::from_secs(5),
            collect_outcomes(tolerant_group_discovery(
                requests,
                4,
                PER_REQUEST_TIMEOUT,
                far_deadline(),
            )),
        )
        .await
        .expect("a stalled group must not block discovery past its own timeout");

        discovered.sort_unstable();
        failures.sort_unstable();
        assert_eq!(discovered, vec!["apps", "batch"]);
        assert_eq!(failures.len(), 2, "{failures:?}");
        assert_eq!(failures[0].0, "metrics.k8s.io/v1beta1");
        assert!(failures[0].1.contains("unable to handle"), "{failures:?}");
        assert_eq!(failures[1].0, "stuck.example.com/v1");
        assert!(failures[1].1.contains("timed out"), "{failures:?}");
    }

    #[tokio::test]
    async fn outcomes_are_yielded_as_soon_as_each_group_settles() {
        let requests = (0..3).map(|index| {
            labelled(index, async move {
                if index == 0 {
                    std::future::pending::<()>().await;
                }
                tokio::time::sleep(Duration::from_millis(20 * index as u64)).await;
                Ok::<_, anyhow::Error>(index)
            })
        });
        let outcomes =
            tolerant_group_discovery(requests, 3, Duration::from_secs(30), far_deadline());
        let mut outcomes = std::pin::pin!(outcomes);

        let first = tokio::time::timeout(Duration::from_secs(1), outcomes.next())
            .await
            .expect("a settled group must not wait for a stalled one")
            .expect("an outcome");
        let second = outcomes.next().await.expect("an outcome");

        assert_eq!(first, ("group1/v1".to_owned(), Ok(1)));
        assert_eq!(second, ("group2/v1".to_owned(), Ok(2)));
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

        let (discovered, failures) = collect_outcomes(tolerant_group_discovery(
            requests,
            1,
            PER_REQUEST_TIMEOUT,
            far_deadline(),
        ))
        .await;

        assert_eq!(discovered, vec![1, 2, 3]);
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].0, "group0/v1");
    }

    #[tokio::test]
    async fn groups_not_settled_by_the_shared_deadline_are_reported_as_failed() {
        let requests = (0..3).map(|index| labelled(index, std::future::pending::<Result<()>>()));

        let (discovered, failures) = tokio::time::timeout(
            Duration::from_secs(5),
            collect_outcomes(tolerant_group_discovery(
                requests,
                1,
                Duration::from_secs(60),
                tokio::time::Instant::now() + Duration::from_millis(30),
            )),
        )
        .await
        .expect("the shared deadline should bound every group, including queued ones");

        assert!(discovered.is_empty());
        assert_eq!(failures.len(), 3);
    }

    enum ScriptedReply {
        /// Refuses aggregated discovery with 406, and answers 200 with `legacy` otherwise.
        RefusesAggregated(&'static str),
        /// Answers 200 with `aggregated` when aggregated discovery is asked for, else `legacy`.
        ByAccept {
            aggregated: &'static str,
            legacy: &'static str,
        },
        Json(&'static str, &'static str),
        /// Answers 200 with the body after the delay.
        Delayed(Duration, &'static str),
        /// Answers 503 until `failures` requests have been served, then the body.
        FailsFirst {
            failures: usize,
            served: AtomicUsize,
            body: &'static str,
        },
        Hang,
    }

    const API_FAILURE: &str = r#"{"kind":"Status","apiVersion":"v1","status":"Failure","message":"the server is currently unable to handle the request","reason":"ServiceUnavailable","code":503}"#;
    const NOT_ACCEPTABLE: &str = r#"{"kind":"Status","apiVersion":"v1","status":"Failure","message":"only legacy discovery is served","reason":"NotAcceptable","code":406}"#;
    const UNAUTHORIZED: &str = r#"{"kind":"Status","apiVersion":"v1","status":"Failure","message":"Unauthorized","reason":"Unauthorized","code":401}"#;
    const CORE_VERSIONS: &str = r#"{"kind":"APIVersions","versions":["v1"]}"#;

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

    /// One group of an aggregated discovery document: (name, version, freshness, resources
    /// as (plural, kind, scope, scalable)).
    type AggregatedGroup<'a> = (
        &'a str,
        &'a str,
        &'a str,
        &'a [(&'a str, &'a str, &'a str, bool)],
    );

    fn aggregated(groups: &[AggregatedGroup]) -> String {
        let items = groups
            .iter()
            .map(|(name, version, freshness, resources)| {
                let resources = resources
                    .iter()
                    .map(|(plural, kind, scope, scalable)| {
                        let subresources = if *scalable {
                            r#"[{"subresource":"scale","responseKind":{"group":"autoscaling","version":"v1","kind":"Scale"},"verbs":["get","patch","update"]}]"#
                        } else {
                            "[]"
                        };
                        format!(
                            r#"{{"resource":"{plural}","responseKind":{{"group":"","version":"","kind":"{kind}"}},"scope":"{scope}","verbs":["list","watch"],"subresources":{subresources}}}"#
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                format!(
                    r#"{{"metadata":{{"name":"{name}"}},"versions":[{{"version":"{version}","resources":[{resources}],"freshness":"{freshness}"}}]}}"#
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        format!(
            r#"{{"kind":"APIGroupDiscoveryList","apiVersion":"apidiscovery.k8s.io/v2","metadata":{{}},"items":[{items}]}}"#
        )
    }

    fn aggregated_core() -> &'static str {
        leak(aggregated(&[(
            "",
            "v1",
            "Current",
            &[
                ("pods", "Pod", "Namespaced", false),
                (
                    "replicationcontrollers",
                    "ReplicationController",
                    "Namespaced",
                    true,
                ),
                ("nodes", "Node", "Cluster", false),
            ],
        )]))
    }

    fn aggregated_groups(metrics_freshness: &str) -> &'static str {
        leak(aggregated(&[
            (
                "apps",
                "v1",
                "Current",
                &[("deployments", "Deployment", "Namespaced", true)],
            ),
            (
                "metrics.k8s.io",
                "v1beta1",
                metrics_freshness,
                &[
                    ("pods", "PodMetrics", "Namespaced", false),
                    ("nodes", "NodeMetrics", "Cluster", false),
                ],
            ),
        ]))
    }

    #[derive(Default)]
    struct Recorder {
        paths: std::sync::Mutex<Vec<String>>,
        accept_headers: std::sync::Mutex<Vec<String>>,
        in_flight: AtomicUsize,
        max_in_flight: AtomicUsize,
    }

    impl Recorder {
        fn paths(&self) -> Vec<String> {
            self.paths.lock().unwrap().clone()
        }
    }

    /// Serves the scripted replies by request path until the returned task is dropped,
    /// recording what was requested.
    async fn scripted_api_server(
        replies: Vec<(&'static str, ScriptedReply)>,
    ) -> (kube::Client, tokio::task::JoinHandle<()>, Arc<Recorder>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test server should bind");
        let address = listener.local_addr().expect("test server has an address");
        let replies = Arc::new(replies);
        let recorder = Arc::new(Recorder::default());
        let recorded = Arc::clone(&recorder);
        let server = tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let replies = Arc::clone(&replies);
                let recorder = Arc::clone(&recorded);
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
                    let accept = request
                        .lines()
                        .find_map(|line| line.strip_prefix("accept: "))
                        .unwrap_or_default()
                        .to_owned();
                    recorder.paths.lock().unwrap().push(path.clone());
                    let wants_aggregated = accept.contains("g=apidiscovery.k8s.io");
                    recorder.accept_headers.lock().unwrap().push(accept);
                    let in_flight = recorder.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                    recorder
                        .max_in_flight
                        .fetch_max(in_flight, Ordering::SeqCst);
                    let (status, body) = match replies.iter().find(|(route, _)| *route == path) {
                        Some((_, ScriptedReply::Json(status, body))) => (*status, *body),
                        Some((_, ScriptedReply::RefusesAggregated(legacy))) => {
                            if wants_aggregated {
                                ("406 Not Acceptable", NOT_ACCEPTABLE)
                            } else {
                                ("200 OK", *legacy)
                            }
                        }
                        Some((_, ScriptedReply::ByAccept { aggregated, legacy })) => (
                            "200 OK",
                            if wants_aggregated {
                                *aggregated
                            } else {
                                *legacy
                            },
                        ),
                        Some((_, ScriptedReply::Delayed(delay, body))) => {
                            tokio::time::sleep(*delay).await;
                            ("200 OK", *body)
                        }
                        Some((
                            _,
                            ScriptedReply::FailsFirst {
                                failures,
                                served,
                                body,
                            },
                        )) => {
                            if served.fetch_add(1, Ordering::SeqCst) < *failures {
                                ("503 Service Unavailable", API_FAILURE)
                            } else {
                                ("200 OK", *body)
                            }
                        }
                        Some((_, ScriptedReply::Hang)) => {
                            std::future::pending::<()>().await;
                            return;
                        }
                        None => ("404 Not Found", API_FAILURE),
                    };
                    recorder.in_flight.fetch_sub(1, Ordering::SeqCst);
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
        (client, server, recorder)
    }

    fn leak(text: String) -> &'static str {
        Box::leak(text.into_boxed_str())
    }

    fn legacy_replies() -> Vec<(&'static str, ScriptedReply)> {
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
            (
                "/apis/metrics.k8s.io/v1beta1",
                ScriptedReply::Json(
                    "200 OK",
                    leak(resource_list(
                        "metrics.k8s.io/v1beta1",
                        &[("pods", "PodMetrics"), ("nodes", "NodeMetrics")],
                    )),
                ),
            ),
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

    /// What one discovery run reported, in order.
    #[derive(Debug, PartialEq)]
    enum Reported {
        Discovered(Vec<String>),
        Completed(Vec<String>),
        GroupListFailed,
        Recovered(Vec<String>),
        ConnectionFailed(String),
    }

    fn reported(result: &WorkerResultBox) -> Reported {
        let any = result.as_ref().as_any();
        let names = |resources: &[ApiResource]| {
            resources
                .iter()
                .map(|resource| format!("{}/{}", resource.group, resource.name))
                .collect()
        };
        if let Some(discovered) = any.downcast_ref::<KubernetesApiResourcesDiscovered>() {
            Reported::Discovered(names(&discovered.resources.api_resources))
        } else if let Some(completed) = any.downcast_ref::<KubernetesApiDiscoveryCompleted>()
            && completed.group_list_failed
        {
            Reported::GroupListFailed
        } else if let Some(completed) = any.downcast_ref::<KubernetesApiDiscoveryCompleted>() {
            Reported::Completed(
                completed
                    .failed_api_groups
                    .iter()
                    .map(|failure| failure.group_version.clone())
                    .collect(),
            )
        } else if let Some(recovered) = any.downcast_ref::<KubernetesApiGroupsRecovered>() {
            Reported::Recovered(recovered.group_versions.clone())
        } else if let Some(failed) = any.downcast_ref::<ClusterConnectionFailed>() {
            Reported::ConnectionFailed(failed.error.clone())
        } else {
            panic!("unexpected discovery result {result:?}")
        }
    }

    /// Runs discovery against the scripted server with fast timeouts and retries, until it
    /// ends or `max` results were reported.
    async fn run_discovery(
        replies: Vec<(&'static str, ScriptedReply)>,
        max: usize,
    ) -> (Vec<Reported>, Vec<WorkerResultBox>, Arc<Recorder>) {
        run_discovery_with(replies, max, FAST_TIMEOUTS).await
    }

    async fn run_discovery_with(
        replies: Vec<(&'static str, ScriptedReply)>,
        max: usize,
        timeouts: DiscoveryTimeouts,
    ) -> (Vec<Reported>, Vec<WorkerResultBox>, Arc<Recorder>) {
        let (client, _server, recorder) = scripted_api_server(replies).await;
        let (sender, mut receiver) = tokio::sync::mpsc::channel(64);
        let discovery = tokio::spawn(discover_cluster_api_with(
            7,
            client,
            WorkerResultSender::new(sender, None),
            timeouts,
            std::iter::repeat(Duration::from_millis(5)),
            std::iter::repeat(Duration::from_millis(5)),
        ));
        let mut results = Vec::new();
        while results.len() < max {
            match tokio::time::timeout(Duration::from_secs(5), receiver.recv()).await {
                Ok(Some(result)) => results.push(result),
                Ok(None) => break,
                Err(_) => panic!("discovery stalled after {results:?}"),
            }
        }
        discovery.abort();
        (results.iter().map(reported).collect(), results, recorder)
    }

    fn discovered(names: &[&str]) -> Reported {
        Reported::Discovered(names.iter().map(|name| (*name).to_owned()).collect())
    }

    #[tokio::test]
    async fn aggregated_discovery_needs_only_two_concurrent_requests() {
        let replies = vec![
            (
                "/api",
                ScriptedReply::Delayed(Duration::from_millis(100), aggregated_core()),
            ),
            (
                "/apis",
                ScriptedReply::Delayed(Duration::from_millis(100), aggregated_groups("Current")),
            ),
        ];

        let (reported, results, recorder) = run_discovery(replies, 3).await;

        assert_eq!(reported.len(), 3, "{reported:?}");
        assert!(reported.contains(&discovered(&[
            "core/pods",
            "core/replicationcontrollers",
            "core/nodes"
        ])));
        assert!(reported.contains(&discovered(&[
            "apps/deployments",
            "metrics.k8s.io/pods",
            "metrics.k8s.io/nodes"
        ])));
        assert_eq!(reported[2], Reported::Completed(Vec::new()));
        let mut paths = recorder.paths();
        paths.sort();
        assert_eq!(paths, vec!["/api", "/apis"], "no per-group or CRD requests");
        assert_eq!(
            recorder.max_in_flight.load(Ordering::SeqCst),
            2,
            "requested concurrently"
        );
        assert!(
            recorder
                .accept_headers
                .lock()
                .unwrap()
                .iter()
                .all(|accept| accept.contains("g=apidiscovery.k8s.io;v=v2")),
            "aggregated discovery is requested"
        );
        let scalable = results
            .iter()
            .filter_map(|result| {
                result
                    .as_ref()
                    .as_any()
                    .downcast_ref::<KubernetesApiResourcesDiscovered>()
            })
            .flat_map(|discovered| {
                discovered
                    .resources
                    .scalable_api_resources
                    .iter()
                    .map(|resource| resource.name.clone())
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(
            scalable,
            BTreeSet::from([
                "deployments".to_owned(),
                "replicationcontrollers".to_owned()
            ])
        );
        let namespaced = results
            .iter()
            .filter_map(|result| {
                result
                    .as_ref()
                    .as_any()
                    .downcast_ref::<KubernetesApiResourcesDiscovered>()
            })
            .flat_map(|discovered| discovered.resources.api_resources.iter())
            .map(|resource| {
                (
                    resource.name.as_str(),
                    resource.group.as_str(),
                    resource.namespaced,
                )
            })
            .collect::<Vec<_>>();
        assert!(namespaced.contains(&("nodes", "core", false)));
        assert!(namespaced.contains(&("pods", "core", true)));
    }

    #[tokio::test]
    async fn a_stale_aggregated_group_is_reported_failed_and_recovered_by_the_legacy_retry() {
        let replies = vec![
            ("/api", ScriptedReply::Json("200 OK", aggregated_core())),
            (
                "/apis",
                ScriptedReply::Json("200 OK", aggregated_groups("Stale")),
            ),
            (
                "/apis/metrics.k8s.io/v1beta1",
                ScriptedReply::FailsFirst {
                    failures: 1,
                    served: AtomicUsize::new(0),
                    body: leak(resource_list(
                        "metrics.k8s.io/v1beta1",
                        &[("pods", "PodMetrics"), ("nodes", "NodeMetrics")],
                    )),
                },
            ),
        ];

        let (reported, _, _) = run_discovery(replies, 4).await;

        assert!(
            reported.contains(&discovered(&["apps/deployments"])),
            "{reported:?}"
        );
        assert_eq!(
            reported[2],
            Reported::Completed(vec!["metrics.k8s.io/v1beta1".to_owned()])
        );
        assert_eq!(
            reported[3],
            Reported::Recovered(vec!["metrics.k8s.io/v1beta1".to_owned()])
        );
    }

    #[tokio::test]
    async fn legacy_servers_report_each_group_as_it_answers() {
        let replies = with_reply(
            with_reply(
                legacy_replies(),
                "/apis",
                ScriptedReply::Json(
                    "200 OK",
                    leak(group_list(&[
                        ("apps", "v1"),
                        ("stuck.example.com", "v1"),
                        ("metrics.k8s.io", "v1beta1"),
                    ])),
                ),
            ),
            "/apis/stuck.example.com/v1",
            ScriptedReply::Hang,
        );

        let (reported, _, recorder) = run_discovery(replies, 4).await;

        let completed_at = reported
            .iter()
            .position(|reported| matches!(reported, Reported::Completed(_)))
            .expect("discovery completes despite the stalled group");
        assert_eq!(
            completed_at, 3,
            "core and both healthy groups come first: {reported:?}"
        );
        assert!(reported.contains(&discovered(&["core/pods"])));
        assert!(reported.contains(&discovered(&["apps/deployments"])));
        assert!(reported.contains(&discovered(&[
            "metrics.k8s.io/pods",
            "metrics.k8s.io/nodes"
        ])));
        assert_eq!(
            reported[completed_at],
            Reported::Completed(vec!["stuck.example.com/v1".to_owned()])
        );
        assert!(
            !recorder
                .paths()
                .iter()
                .any(|path| path.contains("customresourcedefinitions"))
        );
    }

    #[tokio::test]
    async fn a_failing_core_api_fails_the_connection_without_waiting_for_the_groups() {
        let replies = vec![
            (
                "/api",
                ScriptedReply::Json("401 Unauthorized", UNAUTHORIZED),
            ),
            ("/apis", ScriptedReply::Hang),
        ];

        let (reported, _, _) = run_discovery(replies, 2).await;

        assert_eq!(
            reported.len(),
            1,
            "discovery ends with the failure: {reported:?}"
        );
        let Reported::ConnectionFailed(error) = &reported[0] else {
            panic!("expected a connection failure: {reported:?}");
        };
        assert!(error.contains("core Kubernetes API"), "{error}");
    }

    #[tokio::test]
    async fn a_failing_group_list_is_retried_without_failing_the_connection() {
        let replies = vec![
            ("/api", ScriptedReply::Json("200 OK", aggregated_core())),
            (
                "/apis",
                ScriptedReply::FailsFirst {
                    failures: 2,
                    served: AtomicUsize::new(0),
                    body: aggregated_groups("Current"),
                },
            ),
        ];

        let (reported, _, recorder) = run_discovery(replies, 3).await;

        assert_eq!(
            reported[0],
            discovered(&["core/pods", "core/replicationcontrollers", "core/nodes"])
        );
        assert_eq!(
            reported[1],
            discovered(&[
                "apps/deployments",
                "metrics.k8s.io/pods",
                "metrics.k8s.io/nodes"
            ])
        );
        assert_eq!(reported[2], Reported::Completed(Vec::new()));
        assert_eq!(
            recorder
                .paths()
                .iter()
                .filter(|path| *path == "/apis")
                .count(),
            3
        );
    }

    #[tokio::test]
    async fn an_unusable_aggregated_document_is_asked_for_again_in_the_legacy_form() {
        let replies = vec![
            // No kind at all, as some proxies answer: read as the legacy list.
            (
                "/api",
                ScriptedReply::Json("200 OK", r#"{"versions":["v1"]}"#),
            ),
            (
                "/api/v1",
                ScriptedReply::Json("200 OK", leak(resource_list("v1", &[("pods", "Pod")]))),
            ),
            (
                "/apis",
                ScriptedReply::ByAccept {
                    aggregated: r#"{"kind":"SomethingElse"}"#,
                    legacy: leak(group_list(&[("apps", "v1")])),
                },
            ),
            (
                "/apis/apps/v1",
                ScriptedReply::Json(
                    "200 OK",
                    leak(resource_list("apps/v1", &[("deployments", "Deployment")])),
                ),
            ),
        ];

        let (reported, _, recorder) = run_discovery(replies, 3).await;

        assert!(
            reported.contains(&discovered(&["core/pods"])),
            "{reported:?}"
        );
        assert!(
            reported.contains(&discovered(&["apps/deployments"])),
            "{reported:?}"
        );
        assert_eq!(reported[2], Reported::Completed(Vec::new()));
        let accepts = recorder.accept_headers.lock().unwrap().clone();
        let paths = recorder.paths();
        let apis_accepts = paths
            .iter()
            .zip(&accepts)
            .filter(|(path, _)| *path == "/apis")
            .map(|(_, accept)| accept.contains("g=apidiscovery.k8s.io"))
            .collect::<Vec<_>>();
        assert_eq!(
            apis_accepts,
            vec![true, false],
            "one aggregated, then one legacy request"
        );
    }

    #[tokio::test]
    async fn a_refused_aggregated_request_is_asked_for_again_in_the_legacy_form() {
        let replies = with_reply(
            with_reply(
                legacy_replies(),
                "/api",
                ScriptedReply::RefusesAggregated(CORE_VERSIONS),
            ),
            "/apis",
            ScriptedReply::RefusesAggregated(leak(group_list(&[("apps", "v1")]))),
        );

        let (reported, _, _) = run_discovery(replies, 3).await;

        assert!(
            reported.contains(&discovered(&["core/pods"])),
            "{reported:?}"
        );
        assert!(
            reported.contains(&discovered(&["apps/deployments"])),
            "{reported:?}"
        );
        assert_eq!(reported[2], Reported::Completed(Vec::new()));
    }

    #[tokio::test]
    async fn a_group_list_that_does_not_answer_in_time_completes_discovery_and_recovers_later() {
        let replies = vec![
            ("/api", ScriptedReply::Json("200 OK", aggregated_core())),
            (
                "/apis",
                ScriptedReply::FailsFirst {
                    failures: 40,
                    served: AtomicUsize::new(0),
                    body: aggregated_groups("Current"),
                },
            ),
        ];
        // Forty failures, five milliseconds apart, outlast the group list's window.
        let timeouts = DiscoveryTimeouts {
            groups: Duration::from_millis(100),
            ..FAST_TIMEOUTS
        };

        let (reported, results, _) = run_discovery_with(replies, 3, timeouts).await;

        assert_eq!(
            reported,
            vec![
                discovered(&["core/pods", "core/replicationcontrollers", "core/nodes"]),
                Reported::GroupListFailed,
                Reported::Recovered(vec![
                    "apps/v1".to_owned(),
                    "metrics.k8s.io/v1beta1".to_owned()
                ]),
            ]
        );
        let recovered = results[2]
            .as_ref()
            .as_any()
            .downcast_ref::<KubernetesApiGroupsRecovered>()
            .expect("the recovery");
        assert_eq!(recovered.resources.api_resources.len(), 3);
        assert!(recovered.still_failing_group_versions.is_empty());
    }

    #[tokio::test]
    async fn legacy_core_discovery_merges_every_version_and_fails_with_any_of_them() {
        let core_versions = r#"{"kind":"APIVersions","versions":["v1","v2"]}"#;
        let replies = |v2: ScriptedReply| {
            vec![
                ("/api", ScriptedReply::Json("200 OK", core_versions)),
                (
                    "/api/v1",
                    ScriptedReply::Json("200 OK", leak(resource_list("v1", &[("pods", "Pod")]))),
                ),
                ("/api/v2", v2),
                (
                    "/apis",
                    ScriptedReply::Json("200 OK", leak(group_list(&[]))),
                ),
            ]
        };

        let (reported, _, _) = run_discovery(
            replies(ScriptedReply::Json(
                "200 OK",
                leak(resource_list("v2", &[("widgets", "Widget")])),
            )),
            2,
        )
        .await;
        let (failed, _, _) = run_discovery(
            replies(ScriptedReply::Json(
                "500 Internal Server Error",
                API_FAILURE,
            )),
            2,
        )
        .await;

        assert_eq!(reported[0], discovered(&["core/pods", "core/widgets"]));
        assert!(
            matches!(failed.as_slice(), [Reported::ConnectionFailed(_)]),
            "{failed:?}"
        );
    }

    #[tokio::test]
    async fn a_failed_legacy_group_is_retried_and_reported_when_it_recovers() {
        let replies = with_reply(
            legacy_replies(),
            "/apis/metrics.k8s.io/v1beta1",
            ScriptedReply::FailsFirst {
                failures: 1,
                served: AtomicUsize::new(0),
                body: leak(resource_list(
                    "metrics.k8s.io/v1beta1",
                    &[("pods", "PodMetrics")],
                )),
            },
        );

        let (reported, _, _) = run_discovery(replies, 4).await;

        assert_eq!(
            reported[2],
            Reported::Completed(vec!["metrics.k8s.io/v1beta1".to_owned()])
        );
        assert_eq!(
            reported[3],
            Reported::Recovered(vec!["metrics.k8s.io/v1beta1".to_owned()])
        );
    }

    #[tokio::test]
    async fn crd_printer_columns_are_empty_when_the_crd_cannot_be_read() {
        const CRD: &str =
            "/apis/apiextensions.k8s.io/v1/customresourcedefinitions/widgets.example.com";
        for status in ["403 Forbidden", "500 Internal Server Error"] {
            let (client, _server, recorder) =
                scripted_api_server(vec![(CRD, ScriptedReply::Json(status, API_FAILURE))]).await;

            let columns = crd_printer_columns(client, &widgets("v1")).await;

            assert!(columns.is_empty(), "{status}");
            assert_eq!(recorder.paths(), vec![CRD], "{status}");
        }
    }

    fn crd_with_printer_columns(name: &str) -> String {
        format!(
            r#"{{"apiVersion":"apiextensions.k8s.io/v1","kind":"CustomResourceDefinition","metadata":{{"name":"{name}"}},"spec":{{"group":"example.com","names":{{"plural":"widgets","kind":"Widget"}},"scope":"Namespaced","versions":[{{"name":"v1alpha1","served":true,"storage":false}},{{"name":"v1","served":true,"storage":true,"additionalPrinterColumns":[{{"name":"Size","type":"integer","jsonPath":".spec.size"}},{{"name":"Phase","type":"string","jsonPath":".status.phase"}}]}}]}}}}"#
        )
    }

    fn widgets(version: &str) -> ApiResource {
        ApiResource {
            group: "example.com".to_owned(),
            version: version.to_owned(),
            kind: "Widget".to_owned(),
            name: "widgets".to_owned(),
            namespaced: true,
        }
    }

    #[tokio::test]
    async fn crd_printer_columns_reads_the_one_crd_for_the_requested_version() {
        let (client, _server, recorder) = scripted_api_server(vec![(
            "/apis/apiextensions.k8s.io/v1/customresourcedefinitions/widgets.example.com",
            ScriptedReply::Json(
                "200 OK",
                leak(crd_with_printer_columns("widgets.example.com")),
            ),
        )])
        .await;

        let columns = crd_printer_columns(client.clone(), &widgets("v1")).await;
        let other_version = crd_printer_columns(client, &widgets("v1alpha1")).await;

        assert_eq!(
            columns
                .iter()
                .map(|column| (
                    column.id.as_str(),
                    column.label.as_str(),
                    column.json_path.as_str()
                ))
                .collect::<Vec<_>>(),
            vec![
                ("crd-0", "Size", ".spec.size"),
                ("crd-1", "Phase", ".status.phase")
            ]
        );
        assert!(other_version.is_empty());
        assert_eq!(recorder.paths().len(), 2, "only the named CRD is read");
    }

    #[tokio::test]
    async fn crd_printer_columns_are_empty_for_resources_that_are_not_crds() {
        let (client, _server, recorder) = scripted_api_server(Vec::new()).await;

        let aggregated = crd_printer_columns(client.clone(), &widgets("v1")).await;
        let built_in = crd_printer_columns(
            client,
            &ApiResource {
                group: "apps".to_owned(),
                version: "v1".to_owned(),
                kind: "Deployment".to_owned(),
                name: "deployments".to_owned(),
                namespaced: true,
            },
        )
        .await;

        assert!(aggregated.is_empty(), "a 404 means no columns");
        assert!(built_in.is_empty());
        assert_eq!(
            recorder.paths().len(),
            1,
            "a dotless built-in group is never looked up"
        );
    }
}
