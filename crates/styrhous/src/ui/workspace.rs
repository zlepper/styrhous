use super::resource_actions::show_resource_action_items;
use super::resource_owner;
use super::state::{
    BulkDeleteTarget, ClusterConnectionState, ClusterLoadState, PendingBulkDelete,
    PendingCronJobRun, PendingDelete, PendingDeploymentRestart, PendingForceDelete, ResourceAction,
    ResourceSearchState, UiState,
};
use super::table_preferences::{
    MetadataColumnSource, PersistedResourceTablePreferences, ResourceTableKey,
    TableColumnDefinition,
};
use super::widgets::{
    show_resource_cell, workspace_empty_state, workspace_error_state, workspace_loading_state,
    workspace_search_error_state,
};
use crate::api_resource::ApiResource;
use crate::helm_release::HelmRelease;
use crate::minimal_namespace::MinimalNamespace;
use crate::minimal_resource::MinimalResource;
use crate::pod_metrics::{format_cpu_cores, format_memory};
use crate::resource_catalog::ResourceNavigation;
use crate::resource_handlers::table_definition;
use crate::resource_table::{
    CPU_COLUMN, CellValue, CustomResourceColumn, MEMORY_COLUMN, NODE_COLUMN, SortValue,
    cell_sort_value, compare_sort_values,
};
use crate::terminal_launcher::{DebugImagePreset, ShellRequest};
use crate::worker::{GetResourceScale, WorkerCommandBox};
use components::colors::{TOOLBAR_BACKGROUND, gray};
use components::design::{spacing, typography};
use components::fuzzy::{FuzzyMatchScore, fuzzy_match_score, normalize_for_search};
use components::{
    ButtonSize, MoreButton, SelectionAction, TableRowBuilder, TailwindButton, TailwindCombobox,
    TailwindSearchInput, TailwindTable, WorkspacePage,
};
use egui_extras::{Size, StripBuilder};
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap, HashSet};

const RESOURCE_SEARCH_WIDTH: f32 = 210.0;
const TOOLBAR_RIGHT_INSET: f32 = spacing::XL;
const TOOLBAR_HEIGHT: f32 = 52.0;
const TOOLBAR_CONTENT_HEIGHT: f32 = 36.0;
const TOOLBAR_VERTICAL_PADDING: f32 = (TOOLBAR_HEIGHT - TOOLBAR_CONTENT_HEIGHT) / 2.0;
const RESOURCE_TABLE_SELECTION_WIDTH: f32 = 48.0;

struct FilteredResources {
    resources: Vec<MinimalResource>,
    regex_error: Option<String>,
    fuzzy_scores: Option<HashMap<String, FuzzyMatchScore>>,
}

enum ResourceTableRow<'a> {
    Resource(&'a MinimalResource),
    HiddenBySearch(usize),
}

#[derive(Clone, Copy)]
struct ResourceActionAvailability {
    enabled: bool,
    supports_scale: bool,
}

struct ResourceSelectionControls<'a> {
    selected_count: usize,
    actions_enabled: bool,
    action: &'a mut Option<ResourceSelectionAction>,
}

struct ResourceTableOptions<'a> {
    custom_columns: &'a [CustomResourceColumn],
    metadata_suggestion_resources: &'a [MinimalResource],
    resource_navigation: &'a ResourceNavigation,
    hidden_resource_count: usize,
    show_namespace_column: bool,
    actions: ResourceActionAvailability,
    debug_image_presets: &'a [DebugImagePreset],
    fuzzy_scores: Option<&'a HashMap<String, FuzzyMatchScore>>,
}

enum NamespaceSelection {
    Replace(String),
    Toggle(String),
    SelectAll,
    ClearAll,
}

enum ResourceSelectionAction {
    Clear,
    Delete,
}

#[derive(Default)]
struct WorkspaceEffects {
    namespace_selection: Option<NamespaceSelection>,
    retry_requested: bool,
    detail_to_open: Option<ResourceDetailTarget>,
    log_to_open: Option<PodLogTarget>,
    yaml_to_open: Option<YamlEditorTarget>,
    shell_request: Option<ShellRequest>,
    column_settings_to_open: Option<super::resource_table_settings::ResourceTableSettingsTarget>,
    helm_detail_to_open: Option<HelmReleaseDetailTarget>,
}

struct ResourceDetailTarget {
    api_resource: ApiResource,
    name: String,
    namespace: Option<String>,
    uid: String,
}

struct PodLogTarget {
    cluster_key: i32,
    name: String,
    namespace: Option<String>,
    container: crate::minimal_resource::PodLogContainer,
}

struct YamlEditorTarget {
    cluster_key: i32,
    api_resource: ApiResource,
    namespace: Option<String>,
    resource_name: String,
}

struct HelmReleaseDetailTarget {
    name: String,
    namespace: String,
}

impl WorkspaceEffects {
    fn apply(
        self,
        ui_state: &mut UiState,
        ctx: &egui::Context,
        commands_to_send: &mut Vec<WorkerCommandBox>,
        shell_requests: &mut Vec<ShellRequest>,
    ) {
        if let (Some(cluster_key), Some(target)) = (ui_state.selected_cluster, self.detail_to_open)
        {
            ui_state.open_resource_detail(
                cluster_key,
                target.api_resource,
                target.name,
                target.namespace,
                target.uid,
                commands_to_send,
            );
        }
        if let (Some(cluster_key), Some(target)) =
            (ui_state.selected_cluster, self.helm_detail_to_open)
        {
            ui_state.open_helm_release_detail(
                cluster_key,
                target.name,
                target.namespace,
                commands_to_send,
            );
        }
        if let Some(target) = self.column_settings_to_open {
            ui_state.replace_global_blade(Box::new(target), commands_to_send);
        }
        if let Some(target) = self.log_to_open {
            ui_state.open_pod_log_window(
                target.cluster_key,
                target.name,
                target.namespace,
                target.container,
                commands_to_send,
            );
        }
        if let Some(target) = self.yaml_to_open {
            ui_state.open_yaml_editor(
                ctx,
                target.cluster_key,
                target.api_resource,
                target.namespace,
                target.resource_name,
                commands_to_send,
            );
        }
        shell_requests.extend(self.shell_request);
        if let (Some(cluster_key), Some(selection)) =
            (ui_state.selected_cluster, self.namespace_selection)
        {
            match selection {
                NamespaceSelection::Replace(namespace) => {
                    ui_state.replace_selected_namespaces(
                        cluster_key,
                        [namespace],
                        commands_to_send,
                    );
                }
                NamespaceSelection::Toggle(namespace) => {
                    ui_state.toggle_namespace(cluster_key, namespace, commands_to_send);
                }
                NamespaceSelection::SelectAll => {
                    ui_state.select_all_namespaces(cluster_key, commands_to_send);
                }
                NamespaceSelection::ClearAll => {
                    ui_state.clear_selected_namespaces(cluster_key, commands_to_send);
                }
            }
            if let Some(cluster) = ui_state.clusters.get_mut(&cluster_key)
                && let Some(api_resource) = cluster.selected_api_resource.clone()
            {
                let visible_uids = selected_resources(cluster, Some(&api_resource))
                    .into_iter()
                    .map(|resource| resource.uid)
                    .collect::<HashSet<_>>();
                if let Some(resource_selection) = cluster.resource_selections.get_mut(&api_resource)
                {
                    resource_selection.retain(|uid| visible_uids.contains(uid));
                }
            }
        }
        if self.retry_requested
            && let Some(cluster_key) = ui_state.selected_cluster
        {
            ui_state.retry_selected_load(cluster_key, commands_to_send);
        }
    }
}

pub(super) fn show(
    ui: &mut egui::Ui,
    ui_state: &mut UiState,
    commands_to_send: &mut Vec<WorkerCommandBox>,
    shell_requests: &mut Vec<ShellRequest>,
    debug_image_presets: &[DebugImagePreset],
    table_preferences: &mut PersistedResourceTablePreferences,
) {
    let ctx = ui.ctx().clone();
    let mut effects = WorkspaceEffects::default();
    let mut resource_selection_action = None;
    egui::CentralPanel::default()
        .frame(WorkspacePage::frame())
        .show(ui, |ui| {
            WorkspacePage::show(ui, |ui| {
                let toolbar_rect = egui::Rect::from_min_size(
                    ui.max_rect().min,
                    egui::vec2(ui.available_width(), TOOLBAR_HEIGHT),
                );
                ui.painter()
                    .rect_filled(toolbar_rect, 0.0, TOOLBAR_BACKGROUND);

                let Some(selected_cluster_id) = ui_state.selected_cluster else {
                    workspace_empty_state(
                        ui,
                        "Choose a cluster",
                        "Select a Kubernetes context from the cluster rail to begin exploring.",
                    );
                    return;
                };
                let Some(cluster) = ui_state.clusters.get_mut(&selected_cluster_id) else {
                    workspace_empty_state(
                        ui,
                        "Choose a cluster",
                        "Select a Kubernetes context from the cluster rail to begin exploring.",
                    );
                    return;
                };

                match &cluster.connection {
                    ClusterConnectionState::Connecting => {
                        workspace_loading_state(
                            ui,
                            "Connecting to cluster",
                            "Establishing a connection to the selected Kubernetes context.",
                        );
                        return;
                    }
                    ClusterConnectionState::Failed(error) => {
                        effects.retry_requested =
                            workspace_error_state(ui, "Unable to connect", error);
                        return;
                    }
                    ClusterConnectionState::Disconnected => {
                        workspace_empty_state(
                            ui,
                            "Choose a cluster",
                            "Select a Kubernetes context from the cluster rail to begin exploring.",
                        );
                        return;
                    }
                    ClusterConnectionState::Connected => {}
                }

                match (&cluster.namespaces_load, &cluster.api_resources_load) {
                    (ClusterLoadState::Failed(error), _) | (_, ClusterLoadState::Failed(error)) => {
                        effects.retry_requested =
                            workspace_error_state(ui, "Unable to load cluster data", error);
                        return;
                    }
                    (ClusterLoadState::Ready, ClusterLoadState::Ready) => {}
                    _ => {
                        workspace_loading_state(
                            ui,
                            "Loading cluster data",
                            "Discovering namespaces and API resources.",
                        );
                        return;
                    }
                }

                let selected_api_resource = cluster.selected_api_resource.clone();
                if selected_api_resource
                    .as_ref()
                    .is_some_and(crate::api_resource::ApiResource::is_helm_releases)
                {
                    effects.helm_detail_to_open = show_helm_releases_workspace(
                        ui,
                        cluster,
                        &mut effects.namespace_selection,
                        table_preferences,
                        &mut effects.column_settings_to_open,
                    )
                    .map(|(name, namespace)| HelmReleaseDetailTarget { name, namespace });
                    return;
                }
                let all_resources = decorate_usage_rows(
                    cluster,
                    selected_api_resource.as_ref(),
                    selected_resources(cluster, selected_api_resource.as_ref()),
                );
                let selected_resource_count = selected_api_resource
                    .as_ref()
                    .and_then(|api_resource| cluster.resource_selections.get(api_resource))
                    .map_or(0, HashSet::len);
                let resource_actions_enabled = cluster.resource_detail_panel.is_none()
                    && cluster.bulk_delete_progress.is_none();
                let mut resource_search = selected_api_resource
                    .as_ref()
                    .and_then(|api_resource| cluster.resource_searches.get(api_resource))
                    .cloned()
                    .unwrap_or_default();
                let filtered_resources = show_toolbar(
                    ui,
                    cluster,
                    selected_api_resource.as_ref(),
                    &all_resources,
                    &mut resource_search,
                    &mut effects.namespace_selection,
                    ResourceSelectionControls {
                        selected_count: selected_resource_count,
                        actions_enabled: resource_actions_enabled,
                        action: &mut resource_selection_action,
                    },
                );
                if let Some(api_resource) = &selected_api_resource {
                    cluster
                        .resource_searches
                        .insert(api_resource.clone(), resource_search);
                }
                ui.add_space(TOOLBAR_VERTICAL_PADDING);
                ui.painter().line_segment(
                    [toolbar_rect.left_bottom(), toolbar_rect.right_bottom()],
                    ui.visuals().widgets.noninteractive.bg_stroke,
                );

                let Some(api_resource) = &selected_api_resource else {
                    workspace_empty_state(
                        ui,
                        "Select a resource",
                        "Choose an API resource from the navigator to inspect it.",
                    );
                    return;
                };
                if api_resource.namespaced && cluster.selected_namespaces.is_empty() {
                    workspace_empty_state(
                        ui,
                        "Choose a namespace",
                        "Select one or more namespaces to start watching resources.",
                    );
                } else if let Some(error) = selected_watch_error(cluster, api_resource) {
                    effects.retry_requested =
                        workspace_error_state(ui, "Unable to load resources", &error);
                } else if selected_watches_are_loading(cluster, api_resource) {
                    workspace_loading_state(
                        ui,
                        "Loading resources",
                        "Waiting for the selected namespace resources to synchronize.",
                    );
                } else if all_resources.is_empty() {
                    workspace_empty_state(
                        ui,
                        "No resources found",
                        "This resource type has no items in the selected namespace scope.",
                    );
                } else if let Some(error) = filtered_resources.regex_error {
                    workspace_search_error_state(ui, &error);
                } else if filtered_resources.resources.is_empty() {
                    workspace_empty_state(
                        ui,
                        "No matching resources",
                        "Try a different search term.",
                    );
                } else if let Some(action) = {
                    let resources = &mut cluster.resources;
                    show_resource_table(
                        ui,
                        api_resource,
                        &filtered_resources.resources,
                        ResourceTableOptions {
                            custom_columns: resources
                                .custom_resource_columns
                                .get(api_resource)
                                .map(Vec::as_slice)
                                .unwrap_or_default(),
                            metadata_suggestion_resources: &all_resources,
                            resource_navigation: &resources.resource_navigation,
                            hidden_resource_count: all_resources.len()
                                - filtered_resources.resources.len(),
                            show_namespace_column: api_resource.namespaced
                                && cluster.selected_namespaces.len() > 1,
                            actions: ResourceActionAvailability {
                                enabled: resource_actions_enabled,
                                supports_scale: resources
                                    .scalable_api_resources
                                    .contains(api_resource),
                            },
                            debug_image_presets,
                            fuzzy_scores: filtered_resources.fuzzy_scores.as_ref(),
                        },
                        resources
                            .resource_selections
                            .entry(api_resource.clone())
                            .or_default(),
                        table_preferences,
                        &mut effects.column_settings_to_open,
                    )
                } {
                    match action {
                        ResourceAction::OpenDetails {
                            name,
                            namespace,
                            uid,
                        } => {
                            effects.detail_to_open = Some(ResourceDetailTarget {
                                api_resource: api_resource.clone(),
                                name,
                                namespace,
                                uid,
                            });
                        }
                        ResourceAction::EditYaml { name, namespace } => {
                            effects.yaml_to_open = Some(YamlEditorTarget {
                                cluster_key: cluster.cluster_key,
                                api_resource: api_resource.clone(),
                                namespace,
                                resource_name: name,
                            });
                        }
                        ResourceAction::RequestDelete { name, namespace } => {
                            cluster.pending_delete =
                                Some(PendingDelete::new(api_resource.clone(), name, namespace));
                        }
                        ResourceAction::RequestForceDelete {
                            name,
                            uid,
                            namespace,
                            finalizers,
                        } => {
                            cluster.pending_force_delete = Some(PendingForceDelete::new(
                                api_resource.clone(),
                                name,
                                uid,
                                namespace,
                                finalizers,
                            ));
                        }
                        ResourceAction::RequestDeploymentRestart { name, namespace } => {
                            cluster.pending_deployment_restart = Some(PendingDeploymentRestart {
                                resource_name: name,
                                namespace,
                            });
                        }
                        ResourceAction::RequestCronJobRun { name, namespace } => {
                            cluster.pending_cron_job_run = Some(PendingCronJobRun {
                                resource_name: name,
                                namespace,
                            });
                        }
                        ResourceAction::RequestScale { name, namespace } => {
                            commands_to_send.push(Box::new(GetResourceScale {
                                cluster_key: cluster.cluster_key,
                                api_resource: api_resource.clone(),
                                namespace,
                                resource_name: name,
                            }));
                        }
                        ResourceAction::ViewLogs {
                            name,
                            namespace,
                            container,
                        } => {
                            effects.log_to_open = Some(PodLogTarget {
                                cluster_key: cluster.cluster_key,
                                name,
                                namespace,
                                container,
                            });
                        }
                        action @ (ResourceAction::Shell { .. }
                        | ResourceAction::PodDebugShell { .. }
                        | ResourceAction::NodeShell { .. }) => {
                            effects.shell_request = action.shell_request(&cluster.name);
                        }
                        ResourceAction::SaveData { .. } => {
                            unreachable!("resource table actions cannot save inspector data")
                        }
                        ResourceAction::NavigateDetails {
                            api_resource,
                            name,
                            namespace,
                            uid,
                        } => {
                            effects.detail_to_open = Some(ResourceDetailTarget {
                                api_resource,
                                name,
                                namespace,
                                uid,
                            });
                        }
                    }
                }

                if let Some(selection_action) = resource_selection_action.take() {
                    match selection_action {
                        ResourceSelectionAction::Clear => {
                            cluster.resource_selections.remove(api_resource);
                        }
                        ResourceSelectionAction::Delete => {
                            let selected_uids = cluster
                                .resource_selections
                                .get(api_resource)
                                .cloned()
                                .unwrap_or_default();
                            let targets = all_resources
                                .iter()
                                .filter(|resource| selected_uids.contains(&resource.uid))
                                .map(|resource| BulkDeleteTarget {
                                    uid: resource.uid.clone(),
                                    name: resource.name.clone(),
                                    namespace: resource.namespace.clone(),
                                })
                                .collect::<Vec<_>>();
                            if !targets.is_empty() {
                                cluster.pending_bulk_delete =
                                    Some(PendingBulkDelete::new(api_resource.clone(), targets));
                            }
                        }
                    }
                }
            });
        });

    effects.apply(ui_state, &ctx, commands_to_send, shell_requests);
}

mod helm;
mod resource_data;
mod resource_table;
mod toolbar;

use helm::*;
use resource_data::*;
use resource_table::*;
use toolbar::*;

#[cfg(test)]
mod tests;
