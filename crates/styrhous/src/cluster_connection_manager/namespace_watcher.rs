use super::*;

pub(crate) struct KubernetesNamespaceWatcher {
    pub(crate) client: kube::Client,
    pub(crate) event_sender: WorkerResultSender,
    pub(crate) cluster_key: i32,
}

impl KubernetesNamespaceWatcher {
    pub(crate) async fn watch_namespaces(self) {
        let namespace_api = Api::<Namespace>::all(self.client.clone());
        consume_namespace_stream(
            resilient_watcher(namespace_api, watcher_config()),
            &self.event_sender,
            self.cluster_key,
            RetryTracker::new(),
        )
        .await;
    }
}

pub(crate) async fn consume_namespace_stream<E: Debug>(
    stream: impl futures_util::Stream<Item = std::result::Result<Event<Namespace>, E>>,
    event_sender: &WorkerResultSender,
    cluster_key: i32,
    mut retry: RetryTracker,
) {
    let mut buffer = Vec::<MinimalNamespace>::new();
    pin_mut!(stream);

    while let Some(event) = stream.next().await {
        let transition = retry.observe(&event, "Namespace");
        let ev = match event {
            Ok(event) => event,
            Err(error) => {
                if transition == RetryTransition::Degraded {
                    event_sender
                        .send(KubernetesNamespacesReconnecting {
                            cluster_key,
                            error: format!("{error:#?}"),
                        })
                        .await
                        .log_if_error("Failed to send namespace watcher reconnecting");
                }
                continue;
            }
        };
        if transition == RetryTransition::Recovered {
            event_sender
                .send(KubernetesNamespacesRecovered { cluster_key })
                .await
                .log_if_error("Failed to send namespace watcher recovery");
        }
        match ev {
            Event::Apply(item) => {
                event_sender
                    .send(KubernetesNamespacesAdded {
                        namespace: item.into(),
                        cluster_key,
                    })
                    .await
                    .log_if_error("Failed to send updated namespace");
            }
            Event::Delete(item) => {
                let Some(namespace_name) = item.metadata.name else {
                    warn!("Ignoring deleted Namespace without a name from the api server");
                    continue;
                };
                event_sender
                    .send(KubernetesNamespacesDeleted {
                        cluster_key,
                        namespace_name,
                    })
                    .await
                    .log_if_error("Failed to send notification about deleted namespace");
            }
            Event::Init => {
                buffer.clear();
            }
            Event::InitApply(item) => {
                buffer.push(item.into());
            }
            Event::InitDone => {
                event_sender
                    .send(KubernetesNamespacesReplaced {
                        cluster_key,
                        namespaces: std::mem::take(&mut buffer),
                    })
                    .await
                    .log_if_error("Failed to send entire replaced namespace list");
            }
        }
    }
}

pub(crate) fn watcher_config() -> watcher::Config {
    watcher::Config {
        list_semantic: ListSemantic::Any,
        initial_list_strategy: watcher::InitialListStrategy::ListWatch,
        ..Default::default()
    }
}
