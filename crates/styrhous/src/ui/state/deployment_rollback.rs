use super::*;
use crate::cluster_connection_manager::DeploymentRevisionHistory;
use crate::worker::GetDeploymentRevisions;

impl ClusterResourceState {
    /// Opens the rollback dialog for a Deployment and loads its revisions.
    pub(crate) fn request_deployment_rollback(
        &mut self,
        cluster_key: i32,
        namespace: String,
        resource_name: String,
        commands_to_send: &mut Vec<WorkerCommandBox>,
    ) {
        self.next_deployment_revisions_request_id += 1;
        let request_id = self.next_deployment_revisions_request_id;
        self.pending_deployment_rollback = Some(PendingDeploymentRollback {
            deployment: DeploymentKey {
                namespace: namespace.clone(),
                resource_name: resource_name.clone(),
            },
            request_id,
            revisions: DeploymentRevisionsLoad::Loading,
        });
        commands_to_send.push(Box::new(GetDeploymentRevisions {
            cluster_key,
            request_id,
            namespace,
            resource_name,
        }));
    }
}

impl DeploymentRevisionsLoad {
    pub(crate) fn loaded(history: DeploymentRevisionHistory) -> Self {
        // Default to the newest revision that is not running: what
        // `kubectl rollout undo` picks without `--to-revision`.
        let selected = history
            .revisions
            .iter()
            .find(|revision| !revision.is_current)
            .map(|revision| revision.revision);
        Self::Loaded { history, selected }
    }
}

impl UiState {
    /// Applies a revision load to the dialog that requested it; a reply for a
    /// dialog that was closed or reopened since is ignored.
    pub(crate) fn settle_deployment_revisions(
        &mut self,
        cluster_key: i32,
        request_id: u64,
        result: Result<DeploymentRevisionHistory, String>,
    ) {
        let Some(pending) = self
            .clusters
            .get_mut(&cluster_key)
            .and_then(|cluster| cluster.pending_deployment_rollback.as_mut())
            .filter(|pending| pending.request_id == request_id)
        else {
            return;
        };
        pending.revisions = match result {
            Ok(history) => DeploymentRevisionsLoad::loaded(history),
            Err(error) => DeploymentRevisionsLoad::Failed(error),
        };
    }
}
