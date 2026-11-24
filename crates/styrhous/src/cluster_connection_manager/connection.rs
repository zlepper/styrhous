//! Kubeconfig discovery and the lifetime owner for a connected cluster.

use super::{KubernetesNamespaceWatcher, discover_cluster_api};
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
    /// The API server URL of the context's cluster, when the kubeconfig names one.
    pub server: Option<String>,
}

pub async fn reload_kubeconfig() -> Result<KubernetesClustersUpdated> {
    let cfg = Kubeconfig::read().with_context(|| "Error reading kubeconfig")?;
    Ok(KubernetesClustersUpdated(clusters_of(cfg)))
}

/// The contexts of `cfg`, each with the API server its cluster entry names.
fn clusters_of(cfg: Kubeconfig) -> Vec<Cluster> {
    let current_context = cfg.current_context.clone();
    let servers = cfg
        .clusters
        .iter()
        .filter_map(|named| Some((named.name.clone(), named.cluster.as_ref()?.server.clone()?)))
        .collect::<std::collections::HashMap<_, _>>();
    cfg.contexts
        .into_iter()
        .map(|named_context| Cluster {
            server: named_context
                .context
                .as_ref()
                .and_then(|context| servers.get(&context.cluster))
                .cloned(),
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
        .collect()
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

/// Runs blocking work, such as file I/O or an exec credential plugin, off the async runtime.
async fn run_blocking<T, E>(
    what: &'static str,
    work: impl FnOnce() -> std::result::Result<T, E> + Send + 'static,
) -> Result<T>
where
    T: Send + 'static,
    E: std::error::Error + Send + Sync + 'static,
{
    tokio::task::spawn_blocking(work)
        .await
        .with_context(|| format!("{what} was cancelled"))?
        .with_context(|| format!("Error {what}"))
}

/// Builds a client, which loads certificates and may run an exec credential plugin.
async fn build_client(config: kube::Config) -> Result<kube::Client> {
    run_blocking("creating the Kubernetes client", move || {
        kube::Client::try_from(config)
    })
    .await
}

/// Reads the kubeconfig and the certificate files it references for one context.
async fn load_context_config(context_name: &str) -> Result<kube::Config> {
    let options = KubeConfigOptions {
        context: Some(context_name.to_owned()),
        ..Default::default()
    };
    run_blocking("creating the Kubernetes config", move || {
        // Nothing in it awaits I/O; it is async only by signature.
        tokio::runtime::Handle::current().block_on(kube::Config::from_kubeconfig(&options))
    })
    .await
}

impl Debug for ClusterConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClusterStateRunner")
            .field("cluster_key", &self.cluster_key)
            .finish()
    }
}

impl ClusterConnection {
    /// A connection to a fake API server, without watchers.
    #[cfg(test)]
    pub(crate) fn for_test(cluster_key: i32, config: kube::Config) -> Self {
        Self {
            client: kube::Client::try_from(config.clone()).expect("test client builds"),
            log_client: LazyLogClient::new(&config),
            join_handles: Vec::new(),
            cluster_key,
        }
    }

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
        let client = build_client(config).await?;

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
        let api_resources_handle = tokio::spawn(discover_cluster_api(
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
    async fn a_log_client_that_cannot_be_built_reports_why_when_it_is_first_used() {
        let mut config = kube::Config::new(
            "https://kubernetes.example"
                .parse()
                .expect("test cluster URL is valid"),
        );
        config.root_cert = Some(vec![b"not a certificate".to_vec()]);
        let log_client = LazyLogClient::new(&config);

        let Err(error) = log_client.get().await else {
            panic!("an unusable certificate fails the build");
        };

        assert!(
            format!("{error:#}").contains("Kubernetes log client"),
            "{error:#}"
        );
        assert!(!log_client.is_built(), "a later use tries again");
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

#[cfg(test)]
mod kubeconfig_tests {
    use super::*;

    #[test]
    fn a_context_carries_the_server_of_the_cluster_it_names() {
        let cfg = Kubeconfig::from_yaml(
            r#"
apiVersion: v1
kind: Config
current-context: kind-kind
clusters:
- name: kind-kind
  cluster: {server: "https://127.0.0.1:41207"}
- name: no-server
  cluster: {}
contexts:
- name: kind-kind
  context: {cluster: kind-kind, user: u}
- name: bare
  context: {cluster: no-server, user: u}
- name: dangling
  context: {cluster: missing, user: u}
users:
- name: u
  user: {}
"#,
        )
        .expect("a valid kubeconfig");

        let clusters = clusters_of(cfg)
            .into_iter()
            .map(|cluster| (cluster.name, cluster.is_current, cluster.server))
            .collect::<Vec<_>>();
        assert_eq!(
            clusters,
            [
                (
                    "kind-kind".to_owned(),
                    true,
                    Some("https://127.0.0.1:41207".to_owned())
                ),
                ("bare".to_owned(), false, None),
                ("dangling".to_owned(), false, None),
            ]
        );
    }
}
