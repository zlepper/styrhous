use super::state::{BulkDeleteOutcome, BulkDeleteProgress, OperationOutcome, UiState};
use crate::terminal_launcher::TerminalLaunchSettings;
use crate::worker::*;
use components::design::theme::ThemeExt;
use components::design::{space, type_scale};
use components::{
    Button, ConfirmationDialog, ConfirmationDialogAcknowledgement, ConfirmationDialogAction,
    ConfirmationDialogKind, ConfirmationDialogWarning, Dialog, ErrorDialog, ErrorDialogAction,
    FormField, NumberField, Prose,
};
use std::time::Instant;
use tracing::info;

impl WorkerResult for ResourceDeleteFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let ResourceDeleteFailed {
            cluster_key,
            api_resource,
            namespace,
            resource_name,
            bulk_delete_id,
            error,
        } = self;
        if let Some(bulk_delete_id) = bulk_delete_id {
            if let Some(outcome) = ui.settle_bulk_delete_target(
                cluster_key,
                Some(bulk_delete_id),
                &api_resource,
                &resource_name,
                &namespace,
                Some(error),
            ) {
                record_bulk_delete_outcome(ui, cluster_key, outcome);
            }
            return;
        }
        ui.record_resource_operation_failure(
            cluster_key,
            &api_resource,
            format!("Couldn’t delete {}", api_resource.kind),
            namespace.as_deref(),
            resource_name,
            error,
        );
    }
}

impl WorkerResult for ResourceDeleteCompleted {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let ResourceDeleteCompleted {
            cluster_key,
            api_resource,
            namespace,
            resource_name,
            bulk_delete_id,
        } = self;
        if let Some(bulk_delete_id) = bulk_delete_id {
            if let Some(outcome) = ui.settle_bulk_delete_target(
                cluster_key,
                Some(bulk_delete_id),
                &api_resource,
                &resource_name,
                &namespace,
                None,
            ) {
                record_bulk_delete_outcome(ui, cluster_key, outcome);
            }
        } else {
            ui.record_operation_outcome(
                cluster_key,
                OperationOutcome::Success,
                format!("Deleted {}", api_resource.kind),
                namespace.as_deref(),
                resource_name,
                None,
            );
        }
    }
}

impl WorkerResult for ResourceForceDeleteFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let ResourceForceDeleteFailed {
            cluster_key,
            api_resource,
            namespace,
            resource_name,
            error,
        } = self;
        ui.record_resource_operation_failure(
            cluster_key,
            &api_resource,
            format!("Couldn’t remove finalizers from {}", api_resource.kind),
            namespace.as_deref(),
            resource_name,
            error,
        );
    }
}

impl WorkerResult for ResourceForceDeleteCompleted {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let ResourceForceDeleteCompleted {
            cluster_key,
            api_resource,
            namespace,
            resource_name,
        } = self;
        info!("Finalizers removed from resource: {resource_name}");
        ui.record_operation_outcome(
            cluster_key,
            OperationOutcome::Success,
            format!("Finalizers removed from {}", api_resource.kind),
            namespace.as_deref(),
            resource_name,
            None,
        );
    }
}

impl WorkerResult for DeploymentRestartFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let DeploymentRestartFailed {
            cluster_key,
            namespace,
            resource_name,
            error,
        } = self;
        ui.record_operation_outcome(
            cluster_key,
            OperationOutcome::Failure,
            "Couldn’t restart Deployment rollout",
            Some(&namespace),
            resource_name,
            Some(error),
        );
    }
}

impl WorkerResult for DeploymentRestartCompleted {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        info!(
            "Deployment rollout restart requested: {} in {}",
            self.resource_name, self.namespace
        );
        ui.record_operation_outcome(
            self.cluster_key,
            OperationOutcome::Success,
            "Deployment rollout restarted",
            Some(&self.namespace),
            self.resource_name,
            None,
        );
    }
}

impl WorkerResult for CronJobRunFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if ui
            .settle_operation(self.cluster_key, self.operation_id, |cluster| {
                &mut cluster.cron_job_runs
            })
            .is_some()
        {
            ui.record_operation_outcome(
                self.cluster_key,
                OperationOutcome::Failure,
                "Couldn’t run CronJob",
                Some(&self.namespace),
                self.cron_job_name,
                Some(self.error),
            );
        }
    }
}

impl WorkerResult for CronJobRunCompleted {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        info!(
            "Created one-off Job {} from CronJob {} in {}",
            self.job_name, self.cron_job_name, self.namespace
        );
        if ui
            .settle_operation(self.cluster_key, self.operation_id, |cluster| {
                &mut cluster.cron_job_runs
            })
            .is_some()
        {
            ui.record_operation_outcome(
                self.cluster_key,
                OperationOutcome::Success,
                "CronJob started",
                Some(&self.namespace),
                self.cron_job_name,
                Some(format!("Created Job {}", self.job_name)),
            );
        }
    }
}

impl WorkerResult for ResourceScaleFetchFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        ui.settle_scale_lookup(self.cluster_key, self.request_id);
        ui.record_resource_operation_failure(
            self.cluster_key,
            &self.api_resource,
            format!("Couldn’t load scale for {}", self.api_resource.kind),
            self.namespace.as_deref(),
            self.resource_name,
            self.error,
        );
    }
}

impl WorkerResult for ResourceScaleFetched {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let ResourceScaleFetched {
            cluster_key,
            api_resource,
            namespace,
            resource_name,
            replicas,
            request_id,
        } = self;
        if ui.settle_scale_lookup(cluster_key, request_id)
            && let Some(cluster) = ui.clusters.get_mut(&cluster_key)
        {
            cluster.pending_scale = Some(super::state::PendingScale {
                api_resource,
                resource_name,
                namespace,
                current_replicas: replicas,
                desired_replicas: replicas.to_string(),
            });
        }
    }
}

impl WorkerResult for ResourceScaleUpdated {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        info!("Scale updated for resource: {}", self.resource_name);
        ui.record_operation_outcome(
            self.cluster_key,
            OperationOutcome::Success,
            format!(
                "{} scaled to {} replicas",
                self.api_resource.kind, self.replicas
            ),
            self.namespace.as_deref(),
            self.resource_name,
            None,
        );
    }
}

impl WorkerResult for ResourceScaleUpdateFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        ui.record_resource_operation_failure(
            self.cluster_key,
            &self.api_resource,
            format!("Couldn’t scale {}", self.api_resource.kind),
            self.namespace.as_deref(),
            self.resource_name,
            self.error,
        );
    }
}

fn record_bulk_delete_outcome(ui: &mut UiState, cluster_key: i32, outcome: BulkDeleteOutcome) {
    let resource_label = outcome.api_resource.display_name();
    let namespace = outcome.namespace.as_deref();
    let target = format!("{} {resource_label}", outcome.target_count);
    if outcome.failures.is_empty() {
        ui.record_operation_outcome(
            cluster_key,
            OperationOutcome::Success,
            format!("Deleted {target}"),
            namespace,
            target,
            None,
        );
        return;
    }

    let failure_count = outcome.failures.len();
    let single_failure = failure_count == 1;
    let details = outcome
        .failures
        .into_iter()
        .map(|(target, error)| {
            let error = UiState::safe_operation_detail(&outcome.api_resource, error);
            // One failure is the whole story; several need telling apart.
            if single_failure {
                error
            } else {
                format!("{}: {error}", target.display_name())
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    // Some objects went; the rest is a warning, not a failed operation.
    let deleted_count = outcome.target_count.saturating_sub(failure_count);
    let (outcome_kind, details) = if deleted_count > 0 {
        (
            OperationOutcome::Warning,
            format!("{deleted_count} deleted, {failure_count} failed: {details}"),
        )
    } else {
        (OperationOutcome::Failure, details)
    };
    ui.record_operation_outcome(
        cluster_key,
        outcome_kind,
        format!(
            "Bulk delete finished with {failure_count} failure{}",
            if failure_count == 1 { "" } else { "s" }
        ),
        namespace,
        target,
        Some(details),
    );
}

mod deletes;
mod deployment_rollback;
mod helm;
mod logs;
mod nodes;
mod scale;
mod terminal;
mod workloads;

pub(super) use deletes::*;
pub(super) use deployment_rollback::*;
pub(super) use helm::*;
pub(super) use logs::*;
pub(super) use nodes::*;
pub(super) use scale::*;
pub(super) use terminal::*;
pub(super) use workloads::*;
