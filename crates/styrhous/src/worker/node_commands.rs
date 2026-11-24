//! Node maintenance commands and the Nodes table's Pod count watch.

use super::*;
use crate::cluster_connection_manager::{
    DrainOptions, drain_node, set_node_unschedulable, watch_node_pod_counts,
};

impl CordonNode {
    fn failure(&self) -> NodeCordonFailed {
        NodeCordonFailed {
            cluster_key: self.cluster_key,
            operation_id: self.operation_id,
            node_name: self.node_name.clone(),
            cordon: self.cordon,
            error: String::new(),
        }
    }
}

#[async_trait]
impl WorkerCommand for CordonNode {
    type Output = Result<NodeCordonCompleted, NodeCordonFailed>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let failure = self.failure();
        state
            .api_call(
                self.cluster_key,
                ApiCall::mutation("Updating the Node"),
                |client| set_node_unschedulable(client, &self.node_name, self.cordon),
            )
            .await
            .map(|()| NodeCordonCompleted {
                cluster_key: self.cluster_key,
                operation_id: self.operation_id,
                node_name: self.node_name,
                cordon: self.cordon,
            })
            .map_err(|error| NodeCordonFailed {
                error: format!("{error:#}"),
                ..failure
            })
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }

    fn failure_if_abandoned(&self) -> Option<AbandonedFailure> {
        let failure = self.failure();
        abandoned_failure(move |error| NodeCordonFailed { error, ..failure })
    }
}

#[async_trait]
impl WorkerCommand for DrainNode {
    type Output = Result<NoResult, NodeDrainFailed>;

    /// A drain can wait minutes for PodDisruptionBudgets, so it runs as a
    /// task of its own: commands that stop or reconnect the session do not
    /// wait for it, and stopping the session aborts it.
    async fn execute(self, state: &WorkerState) -> Self::Output {
        let DrainNode {
            cluster_key,
            operation_id,
            node_name,
        } = self;
        let failed = move |node_name, error: anyhow::Error| NodeDrainFailed {
            cluster_key,
            operation_id,
            node_name,
            error: format!("{error:#}"),
        };
        let client = match state.client_for_cluster(cluster_key).await {
            Ok(client) => client,
            Err(error) => return Err(failed(node_name, error)),
        };
        let results = state.results.clone();
        let task = tokio::spawn(async move {
            let outcome = match drain_node(client, &node_name, DrainOptions::default()).await {
                Ok(report) => Ok(NodeDrainCompleted {
                    cluster_key,
                    operation_id,
                    node_name,
                    report,
                }),
                Err(error) => Err(failed(node_name, error)),
            };
            results
                .send(outcome)
                .await
                .log_if_error("Failed to send Node drain outcome");
        });
        state.node_drains.prune_finished().await;
        state
            .node_drains
            .replace((cluster_key, operation_id), task)
            .await;
        Ok(NoResult)
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}

#[async_trait]
impl WorkerCommand for StartNodePodCountWatch {
    type Output = Result<NoResult, NodePodCountsWatchFailed>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let client = state
            .client_for_cluster(self.cluster_key)
            .await
            .map_err(|error| NodePodCountsWatchFailed {
                cluster_key: self.cluster_key,
                generation: self.generation,
                error: format!("{error:#}"),
            })?;
        // The previous watch stops before this one can publish.
        state
            .node_pod_count_watches
            .replace_after_abort(self.cluster_key, || {
                tokio::spawn(watch_node_pod_counts(
                    self.cluster_key,
                    self.generation,
                    client,
                    state.results.clone(),
                ))
            })
            .await;
        Ok(NoResult)
    }

    /// A Stop must not overtake the Start before it, or the watch of every Pod would keep running.
    fn dispatch_in_order(&self) -> bool {
        true
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }

    fn failure_if_abandoned(&self) -> Option<AbandonedFailure> {
        let (cluster_key, generation) = (self.cluster_key, self.generation);
        abandoned_failure(move |error| NodePodCountsWatchFailed {
            cluster_key,
            generation,
            error,
        })
    }
}

#[async_trait]
impl WorkerCommand for StopNodePodCountWatch {
    type Output = Result<NoResult, WorkerError>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        state.node_pod_count_watches.abort(&self.cluster_key).await;
        Ok(NoResult)
    }

    fn dispatch_in_order(&self) -> bool {
        true
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}
