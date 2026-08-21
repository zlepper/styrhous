use super::*;

pub(crate) struct KubernetesNamespaceWatcher {
    pub(crate) client: kube::Client,
    pub(crate) event_sender: WorkerResultSender,
    pub(crate) cluster_key: i32,
}

impl KubernetesNamespaceWatcher {
    pub(crate) async fn watch_namespaces(self) {
        let namespace_api = Api::<Namespace>::all(self.client.clone());

        let mut buffer = Vec::<MinimalNamespace>::new();

        let stream = watcher(namespace_api, watcher_config());
        pin_mut!(stream);

        while let Some(event) = stream.next().await {
            let ev = match event {
                Ok(event) => event,
                Err(error) => {
                    warn!("Namespace watcher error: {error:?}");
                    self.event_sender
                        .send(KubernetesNamespacesLoadFailed {
                            cluster_key: self.cluster_key,
                            error: format!("{error:#?}"),
                        })
                        .await
                        .log_if_error("Failed to send namespace watcher error");
                    return;
                }
            };
            match ev {
                Event::Apply(item) => {
                    self.event_sender
                        .send(KubernetesNamespacesAdded {
                            namespace: item.into(),
                            cluster_key: self.cluster_key,
                        })
                        .await
                        .log_if_error("Failed to send updated namespace");
                }
                Event::Delete(item) => {
                    self.event_sender
                        .send(KubernetesNamespacesDeleted {
                            cluster_key: self.cluster_key,
                            namespace_name: item.metadata.name.expect(
                                "Deleted Namespace from the api server did not have a name",
                            ),
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
                    self.event_sender
                        .send(KubernetesNamespacesReplaced {
                            cluster_key: self.cluster_key,
                            namespaces: buffer,
                        })
                        .await
                        .log_if_error("Failed to send entire replaced namespace list");
                    buffer = Vec::new();
                }
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
