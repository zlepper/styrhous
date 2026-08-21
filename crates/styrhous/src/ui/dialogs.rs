use super::state::{BulkDeleteProgress, UiState};
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
        ui.settle_bulk_delete_target(
            self.cluster_key,
            self.bulk_delete_id,
            &self.api_resource,
            &self.resource_name,
            &self.namespace,
            Some(self.error),
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
        ui.settle_bulk_delete_target(
            cluster_key,
            bulk_delete_id,
            &api_resource,
            &resource_name,
            &namespace,
            None,
        );
        if let Some(cluster) = ui.clusters.get_mut(&cluster_key) {
            cluster.pending_delete = None;
        }
    }
}

impl WorkerResult for ResourceForceDeleteFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(cluster) = ui.clusters.get_mut(&self.cluster_key) {
            cluster.force_delete_error = Some(self.error);
        }
    }
}

impl WorkerResult for ResourceForceDeleteCompleted {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let ResourceForceDeleteCompleted {
            cluster_key,
            resource_name,
        } = self;
        info!("Finalizers removed from resource: {resource_name}");
        if let Some(cluster) = ui.clusters.get_mut(&cluster_key) {
            cluster.pending_force_delete = None;
        }
    }
}

impl WorkerResult for DeploymentRestartFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(cluster) = ui.clusters.get_mut(&self.cluster_key) {
            cluster.deployment_restart_error = Some(self.error);
        }
    }
}

impl WorkerResult for DeploymentRestartCompleted {
    fn apply(self, _ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        info!(
            "Deployment rollout restart requested: {} in {}",
            self.resource_name, self.namespace
        );
    }
}

impl WorkerResult for CronJobRunFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(cluster) = ui.clusters.get_mut(&self.cluster_key) {
            cluster.cron_job_run_error = Some(self.error);
        }
    }
}

impl WorkerResult for CronJobRunCompleted {
    fn apply(self, _ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        info!(
            "Created one-off Job {} from CronJob {} in {}",
            self.job_name, self.cron_job_name, self.namespace
        );
    }
}

impl WorkerResult for ResourceScaleFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(cluster) = ui.clusters.get_mut(&self.cluster_key) {
            cluster.scale_error = Some(self.error);
        }
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
        if let Some(cluster) = ui.clusters.get_mut(&self.cluster_key) {
            cluster.pending_scale = None;
        }
    }
}

mod deletes;
mod scale;
mod terminal;
mod workloads;

pub(super) use deletes::*;
pub(super) use scale::*;
pub(super) use terminal::*;
pub(super) use workloads::*;
