use super::global_blade::{
    GlobalBladeContent, GlobalBladeEffect, GlobalBladeEffectContext, GlobalBladeNavigation,
    GlobalBladeRenderContext, GlobalBladeRenderResult,
};
use super::resource_actions::show_resource_action_items;
use super::resource_owner;
use super::state::{
    PendingCronJobRun, PendingDelete, PendingDeploymentRestart, PendingForceDelete, ResourceAction,
    ResourceDetailHistoryEntry, UiState,
};
use super::table_preferences::{ResourceTableKey, TableColumnDefinition};
use super::widgets::show_resource_cell;
use crate::minimal_resource::{MinimalResource, format_age};
use crate::pod_metrics::{
    ContainerUsage, NodeUsage, POD_USAGE_HISTORY_WINDOW, PodUsage, format_cpu, format_memory,
};
use crate::resource_catalog::ResourceNavigation;
use crate::resource_detail::{
    ConfigMapDetail, DiagnosticDetail, ManagedResource, NodeDetail, PodContainerDetail, PodDetail,
    PodResourceThresholds, ResourceDetail, ResourceDetailPayload, ResourceEvent, SecretDetail,
};
use crate::resource_handlers::table_definition;
use crate::resource_table::{
    CONTAINERS_COLUMN, CPU_COLUMN, MEMORY_COLUMN, NODE_COLUMN, READY_COLUMN, RESTARTS_COLUMN,
    ResourceTableDefinition, STATUS_COLUMN, SortValue, cell_sort_value, compare_sort_values,
};
use crate::terminal_launcher::DebugImagePreset;
use crate::worker::{
    GetResourceScale, ResourceDataUpdate, ResourceDataUpdateCompleted, ResourceDataUpdateFailed,
    UpdateResourceData, WorkerCommandBox, WorkerResult,
};
use components::colors::{WHITE, gray, indigo};
use components::design::{radius, spacing, status, typography};
use components::{
    ButtonSize, DetailCell, DetailColumn, DetailRow, DetailTableCell, DetailTableRow, DetailTone,
    DetailValue, InspectorDetails, MoreButton, PointingHand, TableRowBuilder, TailwindButton,
    TailwindTable, TailwindTextArea, WorkspaceCard,
};
use std::cell::RefCell;
use std::collections::BTreeMap;

const CARD_CONTENT_PADDING: i8 = spacing::MD as i8;
const CARD_HEADER_HEIGHT: f32 = 40.0;
const CARD_HEADER_PADDING: f32 = spacing::LG;
const CARD_GAP: f32 = spacing::MD;
const USAGE_CHART_HEIGHT: f32 = 80.0;
const USAGE_CHART_LEFT_INSET: f32 = 30.0;
const USAGE_CHART_TOP_INSET: f32 = 3.0;
const USAGE_CHART_RIGHT_INSET: f32 = 2.0;
const USAGE_CHART_BOTTOM_INSET: f32 = 16.0;
const USAGE_CHART_AREA_OPACITY: f32 = 0.14;
const USAGE_CHART_REFERENCE_OPACITY: f32 = 0.8;

impl WorkerResult for ResourceDataUpdateFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(editor) = ui
            .resource_detail_entry_mut(self.history_entry_id)
            .filter(|entry| entry.cluster_key == self.cluster_key)
            .and_then(|entry| entry.data_editor.as_mut())
            && editor.pending_save_request_id == Some(self.request_id)
        {
            editor.saving = false;
            editor.pending_save_request_id = None;
            editor.save_error = Some(self.error);
        }
    }
}

impl WorkerResult for ResourceDataUpdateCompleted {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let ResourceDataUpdateCompleted {
            cluster_key,
            history_entry_id,
            request_id,
        } = self;
        if let Some(editor) = ui
            .resource_detail_entry_mut(history_entry_id)
            .filter(|entry| entry.cluster_key == cluster_key)
            .and_then(|entry| entry.data_editor.as_mut())
            && editor.pending_save_request_id == Some(request_id)
        {
            editor.mark_saved();
        }
    }
}

#[derive(Default)]
struct BladeResult {
    action: Option<ResourceAction>,
    close: bool,
}

#[derive(Clone, Copy)]
struct PodUsageDisplay<'a> {
    usage: Option<&'a PodUsage>,
    history: &'a [PodUsage],
    missing: bool,
    metrics_api_unavailable: bool,
    error: Option<&'a str>,
}

#[derive(Clone, Copy)]
struct NodeUsageDisplay<'a> {
    usage: Option<&'a NodeUsage>,
    history: &'a [NodeUsage],
    metrics_api_unavailable: bool,
    error: Option<&'a str>,
}

impl GlobalBladeContent for ResourceDetailHistoryEntry {
    fn resource_detail(&self) -> Option<&ResourceDetailHistoryEntry> {
        Some(self)
    }

    fn resource_detail_mut(&mut self) -> Option<&mut ResourceDetailHistoryEntry> {
        Some(self)
    }

    fn render_header(
        &mut self,
        ui: &mut egui::Ui,
        layer: components::BladeLayer,
        context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult {
        let supports_scale = context.supports_scale(self.cluster_key, &self.api_resource);
        let result = show_resource_detail_header(
            ui,
            self,
            layer.is_foreground,
            supports_scale,
            context.debug_image_presets(),
        );
        if layer.is_foreground
            && let Some(action) = result.action
        {
            self.pending_action = Some(action);
        }
        GlobalBladeRenderResult {
            close: result.close,
            ..Default::default()
        }
    }

    fn render_body(
        &mut self,
        ui: &mut egui::Ui,
        layer: components::BladeLayer,
        context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult {
        let resource_navigation = context.resource_navigation(self.cluster_key);
        let mut column_settings = None;
        let result = show_resource_detail_blade(
            ui,
            &resource_navigation,
            &self.api_resource,
            &self.namespace,
            &self.resource_name,
            &self.resource_uid,
            &self.detail,
            &self.events,
            self.detail_error.as_deref(),
            self.events_error.as_deref(),
            &self.managed_resources,
            self.managed_resources_error.as_deref(),
            self.pod_usage.as_ref(),
            &self.pod_usage_history,
            self.pod_usage_missing,
            self.pod_metrics_api_unavailable,
            self.pod_usage_error.as_deref(),
            self.node_usage.as_ref(),
            &self.node_usage_history,
            self.node_metrics_api_unavailable,
            self.node_usage_error.as_deref(),
            if layer.is_foreground {
                self.data_editor.as_mut()
            } else {
                None
            },
            Some(context.table_preferences()),
            Some(&mut column_settings),
        );
        if let Some(column_settings) = column_settings.as_mut() {
            column_settings.set_resource_detail_owner(self.history_entry_id);
        }
        if layer.is_foreground
            && let Some(action) = result.action
        {
            self.pending_action = Some(action);
        }
        GlobalBladeRenderResult {
            close: result.close,
            next_content: column_settings
                .map(|target| Box::new(target) as Box<dyn GlobalBladeContent>),
        }
    }

    fn take_effect(&mut self) -> Option<Box<dyn GlobalBladeEffect>> {
        self.pending_action.take().map(|action| {
            Box::new(ResourceDetailEffect {
                cluster_key: self.cluster_key,
                api_resource: self.api_resource.clone(),
                action,
            }) as Box<dyn GlobalBladeEffect>
        })
    }

    fn show_overlay(&mut self, ctx: &egui::Context) {
        show_data_conflict_dialog(ctx, self.data_editor.as_mut());
    }
}

#[derive(Debug)]
struct ResourceDetailEffect {
    cluster_key: i32,
    api_resource: crate::api_resource::ApiResource,
    action: ResourceAction,
}

impl GlobalBladeEffect for ResourceDetailEffect {
    fn apply(
        self: Box<Self>,
        context: &mut GlobalBladeEffectContext<'_>,
        navigation: &mut GlobalBladeNavigation<'_>,
    ) {
        let Self {
            cluster_key,
            api_resource,
            action,
        } = *self;
        match action {
            ResourceAction::NavigateDetails {
                api_resource,
                name,
                namespace,
                uid,
            } => {
                navigate_resource_detail_in_navigator(
                    context.ui_state,
                    navigation,
                    cluster_key,
                    api_resource,
                    name,
                    namespace,
                    uid,
                );
            }
            ResourceAction::EditYaml { name, namespace } => context.ui_state.open_yaml_editor(
                context.ctx,
                cluster_key,
                api_resource,
                namespace,
                name,
                navigation.commands_to_send(),
            ),
            ResourceAction::RequestDelete { name, namespace } => {
                if let Some(cluster) = context.ui_state.clusters.get_mut(&cluster_key) {
                    cluster.pending_delete =
                        Some(PendingDelete::new(api_resource, name, namespace));
                }
            }
            ResourceAction::RequestForceDelete {
                name,
                uid,
                namespace,
                finalizers,
            } => {
                if let Some(cluster) = context.ui_state.clusters.get_mut(&cluster_key) {
                    cluster.pending_force_delete = Some(PendingForceDelete::new(
                        api_resource,
                        name,
                        uid,
                        namespace,
                        finalizers,
                    ));
                }
            }
            ResourceAction::RequestDeploymentRestart { name, namespace } => {
                if let Some(cluster) = context.ui_state.clusters.get_mut(&cluster_key) {
                    cluster.pending_deployment_restart = Some(PendingDeploymentRestart {
                        resource_name: name,
                        namespace,
                    });
                }
            }
            ResourceAction::RequestCronJobRun { name, namespace } => {
                if let Some(cluster) = context.ui_state.clusters.get_mut(&cluster_key) {
                    cluster.pending_cron_job_run = Some(PendingCronJobRun {
                        resource_name: name,
                        namespace,
                    });
                }
            }
            ResourceAction::RequestScale { name, namespace } => {
                if let Some(cluster) = context.ui_state.clusters.get(&cluster_key) {
                    navigation
                        .commands_to_send()
                        .push(Box::new(GetResourceScale {
                            cluster_key: cluster.cluster_key,
                            api_resource,
                            namespace,
                            resource_name: name,
                        }));
                }
            }
            ResourceAction::SaveData {
                expected_values,
                updated_values,
            } => {
                if let Some(cluster) = context.ui_state.clusters.get_mut(&cluster_key) {
                    cluster.next_data_save_request_id += 1;
                    let request_id = cluster.next_data_save_request_id;
                    let cluster_key = cluster.cluster_key;
                    let update = navigation
                        .current_mut()
                        .resource_detail_mut()
                        .and_then(|entry| {
                            let history_entry_id = entry.history_entry_id;
                            let api_resource = entry.api_resource.clone();
                            let resource_name = entry.resource_name.clone();
                            if let (Some(namespace), Some(editor)) =
                                (entry.namespace.clone(), entry.data_editor.as_mut())
                            {
                                editor.pending_save_request_id = Some(request_id);
                                Some(UpdateResourceData {
                                    cluster_key,
                                    history_entry_id,
                                    request_id,
                                    api_resource,
                                    namespace,
                                    resource_name,
                                    update: ResourceDataUpdate {
                                        expected_resource_version: editor.resource_version.clone(),
                                        expected_values,
                                        updated_values,
                                    },
                                })
                            } else {
                                None
                            }
                        });
                    if let Some(update) = update {
                        navigation.commands_to_send().push(Box::new(update));
                    }
                }
            }
            ResourceAction::ViewLogs {
                name,
                namespace,
                container,
            } => context.ui_state.open_pod_log_window(
                cluster_key,
                name,
                namespace,
                container,
                navigation.commands_to_send(),
            ),
            action @ (ResourceAction::Shell { .. }
            | ResourceAction::PodDebugShell { .. }
            | ResourceAction::NodeShell { .. }) => {
                if let Some(cluster) = context.ui_state.clusters.get(&cluster_key)
                    && let Some(request) = action.shell_request(&cluster.name)
                {
                    context.shell_requests.push(request);
                }
            }
            ResourceAction::OpenDetails { .. } => {
                unreachable!("inspector actions cannot open detail")
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;

    fn container(requests: PodResourceThresholds) -> PodContainerDetail {
        PodContainerDetail {
            name: "app".to_owned(),
            image: "example/app".to_owned(),
            ready: true,
            restart_count: 0,
            state: "Running".to_owned(),
            reason: None,
            message: None,
            command: Vec::new(),
            args: Vec::new(),
            ports: Vec::new(),
            environment_variables: Vec::new(),
            resource_requests: requests,
            resource_limits: PodResourceThresholds::default(),
        }
    }

    #[test]
    fn app_resource_thresholds_sum_known_values_and_reject_overflow() {
        let containers = [
            container(PodResourceThresholds {
                cpu_nanocores: Some(25_000_000),
                memory_bytes: None,
            }),
            container(PodResourceThresholds {
                cpu_nanocores: None,
                memory_bytes: Some(32 * 1024 * 1024),
            }),
        ];

        assert_eq!(
            total_resource_thresholds(&containers, |container| container.resource_requests),
            PodResourceThresholds {
                cpu_nanocores: Some(25_000_000),
                memory_bytes: Some(32 * 1024 * 1024),
            }
        );
        assert_eq!(sum_resource_quantities([None, None].into_iter()), None);
        assert_eq!(sum_resource_quantities([Some(0)].into_iter()), Some(0));
        assert_eq!(
            sum_resource_quantities([Some(i64::MAX), Some(1)].into_iter()),
            None
        );
    }

    #[test]
    fn usage_area_mesh_tessellates_each_non_monotonic_segment_independently() {
        let points = [
            egui::pos2(0.0, 6.0),
            egui::pos2(10.0, 30.0),
            egui::pos2(20.0, 8.0),
            egui::pos2(30.0, 24.0),
        ];
        let baseline = 40.0;
        let mesh = usage_area_mesh(&points, baseline, indigo::_600);

        assert_eq!(mesh.vertices.len(), 4 * (points.len() - 1));
        assert_eq!(mesh.indices.len(), 6 * (points.len() - 1));
        assert!(
            mesh.indices
                .iter()
                .all(|index| (*index as usize) < mesh.vertices.len())
        );
        for (segment, vertices) in points.windows(2).zip(mesh.vertices.chunks_exact(4)) {
            assert_eq!(vertices[0].pos, segment[0]);
            assert_eq!(vertices[1].pos, egui::pos2(segment[0].x, baseline));
            assert_eq!(vertices[2].pos, segment[1]);
            assert_eq!(vertices[3].pos, egui::pos2(segment[1].x, baseline));
        }
    }

    #[test]
    fn inspector_status_tones_reflect_kubernetes_status_values() {
        assert_eq!(pod_phase_tone("Running"), DetailTone::Success);
        assert_eq!(pod_phase_tone("Pending"), DetailTone::Warning);
        assert_eq!(pod_phase_tone("Failed"), DetailTone::Danger);
        assert_eq!(pod_phase_tone("Unknown"), DetailTone::Neutral);

        assert_eq!(event_tone("Normal"), DetailTone::Success);
        assert_eq!(event_tone("Warning"), DetailTone::Warning);
        assert_eq!(event_tone("Other"), DetailTone::Neutral);

        assert_eq!(condition_tone("True"), DetailTone::Success);
        assert_eq!(condition_tone("False"), DetailTone::Neutral);
        assert_eq!(condition_tone("Unknown"), DetailTone::Warning);
    }
}

mod charts;
mod environment;
mod managed;
mod metadata;
mod misc;
mod navigation;
mod summaries;
mod usage;

use charts::*;
use environment::*;
use managed::*;
pub(crate) use metadata::disclosure_card;
use metadata::*;
use misc::*;
use navigation::*;
use summaries::*;
use usage::*;
