use super::state::{BulkDeleteOutcome, BulkDeleteProgress, OperationOutcome, UiState};
use crate::terminal_launcher::TerminalLaunchSettings;
use crate::worker::*;
use components::colors::{WHITE, gray};
use components::design::{radius, spacing, surface, typography};
use components::{
    ButtonSize, ConfirmationDialog, ConfirmationDialogAcknowledgement, ConfirmationDialogAction,
    ConfirmationDialogKind, ConfirmationDialogWarning, ErrorDialog, ErrorDialogAction,
    PointingHand, TailwindButton,
};
use egui::{Align, Color32, Frame, Key, Margin, Modal, Modifiers, Shadow};
use std::time::Instant;
use tracing::info;

const SCALE_DIALOG_WIDTH: f32 = 530.0;

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
        if let Some(cluster) = ui.clusters.get_mut(&cluster_key) {
            cluster.pending_delete = None;
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
        if let Some(cluster) = ui.clusters.get_mut(&cluster_key) {
            cluster.pending_force_delete = None;
        }
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
        if ui.settle_cron_job_run(self.cluster_key, self.operation_id) {
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
        if ui.settle_cron_job_run(self.cluster_key, self.operation_id) {
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
        } = self;
        if let Some(cluster) = ui.clusters.get_mut(&cluster_key) {
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
        if let Some(cluster) = ui.clusters.get_mut(&self.cluster_key) {
            cluster.pending_scale = None;
        }
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
    let target = format!("{} {resource_label}", outcome.target_count);
    if outcome.failures.is_empty() {
        ui.record_operation_outcome(
            cluster_key,
            OperationOutcome::Success,
            format!("Deleted {target}"),
            None,
            target,
            None,
        );
        return;
    }

    let failure_count = outcome.failures.len();
    let details = outcome
        .failures
        .into_iter()
        .map(|(target, error)| {
            format!(
                "{}: {}",
                target.display_name(),
                UiState::safe_operation_detail(&outcome.api_resource, error)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    ui.record_operation_outcome(
        cluster_key,
        OperationOutcome::Failure,
        format!(
            "Bulk delete completed with {failure_count} failure{}",
            if failure_count == 1 { "" } else { "s" }
        ),
        None,
        target,
        Some(details),
    );
}

mod deletes;
mod logs;
mod scale;
mod terminal;
mod workloads;

pub(super) use deletes::*;
pub(super) use logs::*;
pub(super) use scale::*;
pub(super) use terminal::*;
pub(super) use workloads::*;
