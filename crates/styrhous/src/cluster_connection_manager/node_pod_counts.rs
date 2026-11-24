//! How many Pods each Node runs, for the Nodes table.
//!
//! The count is `kubectl describe node`'s "Non-terminated Pods": Pods bound to
//! the Node that have neither succeeded nor failed. Kubernetes cannot count
//! Pods per Node server-side, so the worker watches every such Pod across all
//! namespaces and keeps only each Pod's Node. The watch runs only while the
//! Nodes table is shown; see `UiState::reconcile_node_pod_counts`.

use super::*;

/// Pods that still occupy their Node; finished Pods leave the watch as deletions.
const NON_TERMINATED_PODS: &str = "status.phase!=Succeeded,status.phase!=Failed";

pub(crate) async fn watch_node_pod_counts(
    cluster_key: i32,
    generation: u64,
    client: kube::Client,
    sender: WorkerResultSender,
) {
    let config = watcher_config().fields(NON_TERMINATED_PODS);
    consume_node_pod_count_stream(
        cluster_key,
        generation,
        resilient_watcher(Api::<Pod>::all(client), config),
        &sender,
        RetryTracker::new(),
    )
    .await;
}

/// Publishes the counts once the initial list is complete and then whenever
/// they change, and reports a failure once it outlasts the retry grace period.
pub(crate) async fn consume_node_pod_count_stream<E: Debug>(
    cluster_key: i32,
    generation: u64,
    stream: impl futures_util::Stream<Item = std::result::Result<Event<Pod>, E>>,
    sender: &WorkerResultSender,
    mut retry: RetryTracker,
) {
    pin_mut!(stream);
    let mut state = NodePodCountState::default();
    let mut published = None;
    while let Some(event) = stream.next().await {
        let transition = retry.observe(&event, "Node Pod count");
        let event = match event {
            Ok(event) => event,
            Err(error) => {
                if transition == RetryTransition::Degraded {
                    sender
                        .send(NodePodCountsWatchFailed {
                            cluster_key,
                            generation,
                            error: format!("{error:?}"),
                        })
                        .await
                        .log_if_error("Failed to send Node Pod count error");
                }
                continue;
            }
        };
        state.apply(event);
        let Some(counts) = state.counts() else {
            continue;
        };
        // A recovery is published even when nothing changed, to clear the failure.
        if transition == RetryTransition::Recovered || published.as_ref() != Some(&counts) {
            published = Some(counts.clone());
            sender
                .send(NodePodCountsUpdated {
                    cluster_key,
                    generation,
                    counts,
                })
                .await
                .log_if_error("Failed to send Node Pod counts");
        }
    }
}

/// The Node of every watched Pod, by Pod UID.
#[derive(Default)]
struct NodePodCountState {
    nodes: BTreeMap<String, String>,
    initialized: bool,
}

impl NodePodCountState {
    fn apply(&mut self, event: Event<Pod>) {
        match event {
            Event::Init => {
                self.nodes.clear();
                self.initialized = false;
            }
            Event::InitApply(pod) | Event::Apply(pod) => {
                let uid = get_resource_uid(&pod);
                let finished = is_finished(&pod);
                match pod.spec.and_then(|spec| spec.node_name) {
                    Some(node) if !finished => self.nodes.insert(uid, node),
                    _ => self.nodes.remove(&uid),
                };
            }
            Event::Delete(pod) => {
                self.nodes.remove(&get_resource_uid(&pod));
            }
            Event::InitDone => self.initialized = true,
        }
    }

    /// Pods per Node name, once the initial list is complete. A Node without
    /// Pods is absent.
    fn counts(&self) -> Option<BTreeMap<String, usize>> {
        self.initialized.then(|| {
            let mut counts = BTreeMap::new();
            for node in self.nodes.values() {
                *counts.entry(node.clone()).or_default() += 1;
            }
            counts
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::api::core::v1::{PodSpec, PodStatus};

    fn pod(uid: &str, node: Option<&str>, phase: &str) -> Pod {
        Pod {
            metadata: ObjectMeta {
                name: Some(uid.to_owned()),
                namespace: Some("default".to_owned()),
                uid: Some(uid.to_owned()),
                ..Default::default()
            },
            spec: Some(PodSpec {
                node_name: node.map(str::to_owned),
                ..Default::default()
            }),
            status: Some(PodStatus {
                phase: Some(phase.to_owned()),
                ..Default::default()
            }),
        }
    }

    #[test]
    fn counts_bound_unfinished_pods_per_node_once_the_initial_list_is_complete() {
        let mut state = NodePodCountState::default();
        state.apply(Event::Init);
        state.apply(Event::InitApply(pod("a", Some("node-a"), "Running")));
        state.apply(Event::InitApply(pod("b", Some("node-a"), "Pending")));
        state.apply(Event::InitApply(pod("c", Some("node-b"), "Running")));
        state.apply(Event::InitApply(pod("unscheduled", None, "Pending")));
        assert_eq!(state.counts(), None, "a partial list is not a count");

        state.apply(Event::InitDone);
        assert_eq!(
            state.counts(),
            Some(BTreeMap::from([
                ("node-a".to_owned(), 2),
                ("node-b".to_owned(), 1)
            ]))
        );

        state.apply(Event::Apply(pod("unscheduled", Some("node-b"), "Pending")));
        state.apply(Event::Apply(pod("a", Some("node-a"), "Succeeded")));
        state.apply(Event::Delete(pod("c", Some("node-b"), "Running")));
        assert_eq!(
            state.counts(),
            Some(BTreeMap::from([
                ("node-a".to_owned(), 1),
                ("node-b".to_owned(), 1)
            ])),
            "a bound Pod is counted, a finished or deleted one is not"
        );

        state.apply(Event::Init);
        assert_eq!(state.counts(), None, "a relist starts over");
    }
}
