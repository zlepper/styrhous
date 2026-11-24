//! UI state of Node maintenance (cordon, uncordon, drain) and of the Nodes
//! table's Pod counts.

use super::namespaces::toggle_watch;
use super::*;

/// A drain awaiting confirmation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PendingNodeDrain {
    pub(crate) node_name: String,
}

/// Non-terminated Pods per Node, watched while the Nodes table is shown.
#[derive(Debug, Default)]
pub(crate) struct NodePodCountsState {
    /// `None` until the watch has listed every Pod once.
    pub(crate) counts: Option<BTreeMap<String, usize>>,
    pub(crate) error: Option<String>,
    pub(crate) revision: u64,
    /// The watch whose results are accepted. Every start and stop of the
    /// watch, and every reconnect, moves it on, so a result a previous watch
    /// had already queued is dropped.
    pub(crate) generation: u64,
}

impl NodePodCountsState {
    /// The Pods on `node_name`: `Ok(None)` while the first list is loading.
    pub(crate) fn count(&self, node_name: &str) -> Result<Option<usize>, &str> {
        match (&self.error, &self.counts) {
            (Some(error), _) => Err(error),
            (None, Some(counts)) => Ok(Some(counts.get(node_name).copied().unwrap_or(0))),
            (None, None) => Ok(None),
        }
    }

    /// Nothing counted, for the next watch. The revision and generation keep
    /// counting, so a sorted table cached before and a result of the watch
    /// before cannot match.
    pub(crate) fn restarted(&self) -> Self {
        Self {
            revision: self.revision.wrapping_add(1),
            generation: self.generation.wrapping_add(1),
            ..Default::default()
        }
    }

    /// Applies `update` for a result of the watch `generation`, unless a newer watch replaced it.
    fn update(&mut self, generation: u64, update: impl FnOnce(&mut Self)) {
        if generation == self.generation {
            update(self);
            self.revision = self.revision.wrapping_add(1);
        }
    }
}

impl UiState {
    /// Counts Pods per Node only while the Nodes table is shown: the count
    /// needs a watch of every Pod in the cluster.
    pub(crate) fn reconcile_node_pod_counts(
        cluster: &mut ClusterState,
        commands_to_send: &mut Vec<WorkerCommandBox>,
    ) {
        let desired = cluster
            .selected_api_resource
            .as_ref()
            .is_some_and(ApiResource::is_node);
        let cluster_key = cluster.cluster_key;
        let resources = &mut cluster.resources;
        toggle_watch(
            &mut resources.node_pod_counts_active,
            desired,
            commands_to_send,
            |start| {
                resources.node_pod_counts = resources.node_pod_counts.restarted();
                if start {
                    Box::new(crate::worker::StartNodePodCountWatch {
                        cluster_key,
                        generation: resources.node_pod_counts.generation,
                    })
                } else {
                    Box::new(crate::worker::StopNodePodCountWatch { cluster_key })
                }
            },
        );
    }

    /// Cordons or uncordons `node_name`, tracked so its outcome is matched to this request.
    pub(crate) fn request_node_cordon(
        &mut self,
        cluster_key: i32,
        node_name: String,
        cordon: bool,
    ) -> Option<crate::worker::CordonNode> {
        let cluster = self.clusters.get_mut(&cluster_key)?;
        let operation_id = cluster.node_cordons.start(node_name.clone());
        Some(crate::worker::CordonNode {
            cluster_key,
            operation_id,
            node_name,
            cordon,
        })
    }
}

impl WorkerResult for crate::worker::NodePodCountsUpdated {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(cluster) = ui.clusters.get_mut(&self.cluster_key)
            && cluster.node_pod_counts_active
        {
            cluster.node_pod_counts.update(self.generation, |state| {
                state.counts = Some(self.counts);
                state.error = None;
            });
        }
    }
}

impl WorkerResult for crate::worker::NodePodCountsWatchFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(cluster) = ui.clusters.get_mut(&self.cluster_key)
            && cluster.node_pod_counts_active
        {
            cluster
                .node_pod_counts
                .update(self.generation, |state| state.error = Some(self.error));
        }
    }
}
