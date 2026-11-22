//! Kubeconfig discovery and the lifetime owner for a connected cluster.

use super::{KubernetesApiInspector, KubernetesNamespaceWatcher};
use crate::helpers::ResultExt;
use crate::worker::*;
use anyhow::{Context, Result};
use kube::config::{KubeConfigOptions, Kubeconfig};
use std::fmt::Debug;
use tokio::task::JoinHandle;

#[derive(Debug, Clone, Default)]
pub struct Cluster {
    pub name: String,
    pub is_current: bool,
    pub presentation: crate::context_presentation::ContextPresentation,
}

pub async fn reload_kubeconfig() -> Result<KubernetesClustersUpdated> {
    let cfg = Kubeconfig::read().with_context(|| "Error reading kubeconfig")?;
    let current_context = cfg.current_context.clone();
    let clusters = cfg
        .contexts
        .into_iter()
        .map(|named_context| Cluster {
            is_current: current_context.as_deref() == Some(named_context.name.as_str()),
            presentation: named_context
                .context
                .as_ref()
                .and_then(|context| context.extensions.as_ref())
                .and_then(|extensions| {
                    extensions.iter().find(|extension| {
                        extension.name
                            == crate::context_presentation::CONTEXT_PRESENTATION_EXTENSION
                    })
                })
                .map(|extension| {
                    crate::context_presentation::ContextPresentation::from_extension(
                        &extension.extension,
                    )
                })
                .unwrap_or_default(),
            name: named_context.name,
        })
        .collect();
    Ok(KubernetesClustersUpdated(clusters))
}

/// Context names and their referenced cluster names, used to match managed
/// discovery candidates against the user's authoritative kubeconfig.
pub fn kubeconfig_context_references() -> Result<Vec<String>> {
    let cfg = Kubeconfig::read().with_context(|| "Error reading kubeconfig")?;
    Ok(cfg
        .contexts
        .into_iter()
        .flat_map(|context| {
            std::iter::once(context.name)
                .chain(context.context.into_iter().map(|context| context.cluster))
        })
        .collect())
}

pub struct ClusterConnection {
    client: kube::Client,
    log_client: LazyLogClient,
    join_handles: Vec<JoinHandle<()>>,
    cluster_key: i32,
}

/// The client for Pod log follows, built on first use. Building a client runs the
/// kubeconfig's exec credential plugin, which can take seconds, so connecting must not
/// pay for a client that most sessions never use.
#[derive(Clone)]
pub struct LazyLogClient {
    config: kube::Config,
    client: std::sync::Arc<tokio::sync::OnceCell<kube::Client>>,
}

impl LazyLogClient {
    fn new(config: &kube::Config) -> Self {
        Self {
            config: log_stream_config(config),
            client: Default::default(),
        }
    }

    pub async fn get(&self) -> Result<kube::Client> {
        self.client
            .get_or_try_init(|| build_client(self.config.clone()))
            .await
            .cloned()
            .with_context(|| "Error creating Kubernetes log client")
    }

    #[cfg(test)]
    fn is_built(&self) -> bool {
        self.client.initialized()
    }
}

/// Builds a client off the async runtime: loading certificates and running an exec
/// credential plugin are blocking operations.
async fn build_client(config: kube::Config) -> Result<kube::Client> {
    tokio::task::spawn_blocking(move || kube::Client::try_from(config))
        .await
        .with_context(|| "Kubernetes client construction was cancelled")?
        .map_err(Into::into)
}

async fn load_context_config(context_name: &str) -> Result<kube::Config> {
    let options = KubeConfigOptions {
        context: Some(context_name.to_owned()),
        ..Default::default()
    };
    // Reading the kubeconfig and its certificate files is synchronous file I/O.
    tokio::task::spawn_blocking(move || {
        tokio::runtime::Handle::current().block_on(kube::Config::from_kubeconfig(&options))
    })
    .await
    .with_context(|| "Kubernetes config loading was cancelled")?
    .with_context(|| "Error creating Kubernetes config")
}

impl Debug for ClusterConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClusterStateRunner")
            .field("cluster_key", &self.cluster_key)
            .finish()
    }
}

impl ClusterConnection {
    /// Get a clone of the kube client for starting additional watchers.
    pub fn client(&self) -> kube::Client {
        self.client.clone()
    }

    /// Get the client whose reads may remain idle for the lifetime of a Pod
    /// log follow request.
    pub fn log_client(&self) -> LazyLogClient {
        self.log_client.clone()
    }

    pub async fn new(
        cluster_key: i32,
        context_name: &str,
        event_output: WorkerResultSender,
    ) -> Result<Self> {
        let config = load_context_config(context_name).await?;
        let log_client = LazyLogClient::new(&config);
        let client = build_client(config)
            .await
            .with_context(|| "Error creating Kubernetes client")?;

        let namespaces_handle = tokio::spawn(
            KubernetesNamespaceWatcher {
                event_sender: event_output.clone(),
                client: client.clone(),
                cluster_key,
            }
            .watch_namespaces(),
        );
        // The same task retries groups that failed discovery after reporting the initial
        // result, so it lives and dies with the connection like the namespace watcher.
        let api_resources_handle = tokio::spawn(load_api_resources(
            cluster_key,
            client.clone(),
            event_output,
        ));

        Ok(Self {
            client,
            log_client,
            join_handles: vec![namespaces_handle, api_resources_handle],
            cluster_key,
        })
    }
}

pub(crate) fn log_stream_config(config: &kube::Config) -> kube::Config {
    let mut config = config.clone();
    config.read_timeout = None;
    config
}

impl Drop for ClusterConnection {
    fn drop(&mut self) {
        for handle in self.join_handles.drain(..) {
            handle.abort_handle().abort();
        }
    }
}

async fn load_api_resources(
    cluster_key: i32,
    client: kube::Client,
    event_output: WorkerResultSender,
) {
    let inspection = (KubernetesApiInspector {
        client: client.clone(),
    })
    .inspect_api()
    .await;
    match inspection {
        Err(error) => event_output
            .send(KubernetesApisLoadFailed {
                cluster_key,
                error: format!("{error:#?}"),
            })
            .await
            .log_if_error("Failed to send error from inspecting resource api"),
        Ok(inspection) => {
            event_output
                .send(KubernetesApisLoaded {
                    cluster_key,
                    api_resources: inspection.api_resources,
                    scalable_api_resources: inspection.scalable_api_resources,
                    failed_api_groups: inspection.failed_api_groups,
                    pod_metrics_api_available: inspection.pod_metrics_api_available,
                    node_metrics_api_available: inspection.node_metrics_api_available,
                })
                .await
                .log_if_error("Failed to send kubernetes API resources");
            if !inspection.retry_targets.is_empty() {
                super::retry_failed_api_groups_for_cluster(
                    cluster_key,
                    client,
                    event_output,
                    inspection.retry_targets,
                )
                .await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster_connection_manager::{ApiGroupTarget, retry_failed_api_groups};
    use std::time::Duration;

    #[test]
    fn log_stream_config_disables_only_the_read_timeout() {
        let mut config = kube::Config::new(
            "https://kubernetes.example"
                .parse()
                .expect("test cluster URL is valid"),
        );
        config.connect_timeout = Some(Duration::from_secs(7));
        config.read_timeout = Some(Duration::from_millis(50));
        config.write_timeout = Some(Duration::from_secs(9));

        let log_config = log_stream_config(&config);

        assert_eq!(config.read_timeout, Some(Duration::from_millis(50)));
        assert_eq!(log_config.read_timeout, None);
        assert_eq!(log_config.connect_timeout, config.connect_timeout);
        assert_eq!(log_config.write_timeout, config.write_timeout);
        assert_eq!(log_config.cluster_url, config.cluster_url);
    }

    #[tokio::test]
    async fn the_log_client_is_built_on_first_use_with_the_log_stream_timeouts() {
        let mut config = kube::Config::new(
            "https://kubernetes.example"
                .parse()
                .expect("test cluster URL is valid"),
        );
        config.read_timeout = Some(Duration::from_millis(50));
        let log_client = LazyLogClient::new(&config);
        let shared = log_client.clone();

        assert!(!log_client.is_built(), "connecting must not build it");
        assert_eq!(log_client.config.read_timeout, None);
        log_client.get().await.expect("the log client builds");
        assert!(shared.is_built(), "clones share the built client");
    }

    #[tokio::test]
    async fn dropping_the_connection_stops_the_api_group_retry_task() {
        let (alive, retry_stopped) = tokio::sync::oneshot::channel::<()>();
        let retry = tokio::spawn(async move {
            let _alive = alive;
            retry_failed_api_groups(
                std::iter::repeat(Duration::from_millis(5)),
                tokio::time::sleep,
                |_| async { anyhow::bail!("still failing") },
                |_| async { true },
                vec![ApiGroupTarget {
                    group_version: "metrics.k8s.io/v1beta1".to_owned(),
                    group_name: "metrics.k8s.io".to_owned(),
                    versions: Vec::new(),
                }],
            )
            .await;
        });
        let config = kube::Config::new(
            "https://kubernetes.example"
                .parse()
                .expect("test cluster URL is valid"),
        );
        let connection = ClusterConnection {
            client: kube::Client::try_from(config.clone()).expect("client"),
            log_client: LazyLogClient::new(&config),
            join_handles: vec![retry],
            cluster_key: 1,
        };
        tokio::time::sleep(Duration::from_millis(30)).await;

        drop(connection);

        tokio::time::timeout(Duration::from_secs(2), retry_stopped)
            .await
            .expect("the retry loop should be aborted with its connection")
            .expect_err("the loop's state is dropped without ever signalling");
    }
}
