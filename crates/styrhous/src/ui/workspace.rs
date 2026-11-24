use super::resource_actions::{log_stream_targets, show_resource_action_items};
use super::resource_owner;
use super::resource_table_cache::{Grouping, PreparedResourceTable, PreparedResourceTableRow};
use super::state::{
    BulkDeleteTarget, ClusterConnectionState, ClusterLoadState, NamespaceListWait,
    PendingBulkDelete, PendingCronJobRun, PendingDelete, PendingDeploymentRestart,
    PendingForceDelete, PendingNodeDrain, ResourceAction, ResourceSearchState, SelectedNamespaces,
    UiState,
};
use super::table_preferences::{
    MetadataColumnSource, PersistedResourceTablePreferences, ResourceTableKey,
    TableColumnDefinition,
};
use super::widgets::{
    age_cell, namespace_cell, show_resource_cell, workspace_empty_state, workspace_error_state,
    workspace_filtered_empty_state, workspace_loading_state, workspace_search_error_state,
    workspace_unavailable_state,
};
use crate::api_resource::ApiResource;
use crate::helm_release::HelmRelease;
use crate::minimal_namespace::MinimalNamespace;
use crate::minimal_resource::MinimalResource;
use crate::resource_catalog::ResourceNavigation;
use crate::resource_handlers::table_definition;
use crate::resource_table::{
    CellValue, CustomResourceColumn, NODE_COLUMN, SortValue, cell_sort_value,
};
use crate::terminal_launcher::{DebugImagePreset, ShellRequest, TerminalLaunchSettings};
use crate::ui::global_blade::resource_detail_entry_ids;
use crate::ui::namespace_selector::NamespaceSelectorSettings;
use crate::worker::WorkerCommandBox;
use components::design::theme::ThemeExt;
use components::design::{space, type_scale};
use components::fuzzy::{fuzzy_match_score, normalize_for_search};
use components::icons::Icon;
use components::{
    BulkSelectionBar, Button, ButtonSize, CheckState, MoreButton, SearchStatus, SelectionAction,
    Table, TableRowBuilder, TopBar, WorkspacePage,
};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

/// Skeleton rows shown while a resource list first loads (S08).
const LOADING_ROWS: usize = 3;
/// Around the bulk selection bar, between the toolbar and the table header (C08).
const BULK_BAR_GAP: f32 = space::S12;

struct FilteredResources {
    resources: Vec<MinimalResource>,
}

#[derive(Clone, Copy)]
struct ResourceActionAvailability {
    enabled: bool,
    supports_scale: bool,
}

struct ResourceTableOptions<'a> {
    resource_cache: &'a HashMap<super::state::ResourceWatchKey, super::state::ResourceWatchState>,
    metrics: ResourceMetrics<'a>,
    resource_navigation: &'a ResourceNavigation,
    actions: ResourceActionAvailability,
    debug_image_presets: &'a [DebugImagePreset],
    /// The uid of the resource whose inspector blade is open.
    inspected_uid: Option<&'a str>,
    /// The groups of the table the user collapsed.
    collapsed_groups: &'a HashSet<String>,
    /// The problem row (its uid) the keys last stepped to.
    problem_cursor: Option<&'a str>,
    /// A row (its uid) to scroll into view.
    scroll_to: Option<&'a str>,
    /// Unsorted, the rows come in name order (no search ranks them).
    name_ordered: bool,
}

struct ResourceTableControls<'a> {
    selection: &'a mut HashSet<String>,
    table_preferences: &'a mut PersistedResourceTablePreferences,
    column_settings_to_open:
        &'a mut Option<super::resource_table_settings::ResourceTableSettingsTarget>,
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
    ViewLogs,
}

#[derive(Default)]
struct WorkspaceEffects {
    namespace_selection: Option<NamespaceSelection>,
    retry_requested: bool,
    detail_to_open: Option<ResourceDetailTarget>,
    log_sources_to_open: Option<LogSourcesTarget>,
    yaml_to_open: Option<YamlEditorTarget>,
    shell_request: Option<ShellRequest>,
    column_settings_to_open: Option<super::resource_table_settings::ResourceTableSettingsTarget>,
    helm_detail_to_open: Option<HelmReleaseDetailTarget>,
    scale_to_request: Option<ScaleTarget>,
    /// (namespace, name) of a Deployment whose rollback dialog to open.
    deployment_rollback_to_request: Option<(String, String)>,
    /// The terminal settings, opened from a debug image menu.
    terminal_settings_to_open: Option<TerminalLaunchSettings>,
    /// A Node of the table to cordon or uncordon.
    node_cordon: Option<(i32, String, bool)>,
}

struct ResourceDetailTarget {
    api_resource: ApiResource,
    name: String,
    namespace: Option<String>,
    uid: String,
}

struct ScaleTarget {
    api_resource: ApiResource,
    name: String,
    namespace: Option<String>,
}

struct LogSourcesTarget {
    cluster_key: i32,
    targets: Vec<crate::worker::PodLogStreamTarget>,
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
        if let Some(target) = self.log_sources_to_open {
            ui_state.request_pod_log_window(target.cluster_key, target.targets, commands_to_send);
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
        if let Some(settings) = self.terminal_settings_to_open {
            ui_state.open_terminal_settings(&settings, commands_to_send);
        }
        if let (Some(cluster), Some((namespace, name))) = (
            ui_state
                .selected_cluster
                .and_then(|cluster_key| ui_state.clusters.get_mut(&cluster_key)),
            self.deployment_rollback_to_request,
        ) {
            let cluster_key = cluster.cluster_key;
            cluster.request_deployment_rollback(cluster_key, namespace, name, commands_to_send);
        }
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
        if let Some((cluster_key, node_name, cordon)) = self.node_cordon
            && let Some(command) = ui_state.request_node_cordon(cluster_key, node_name, cordon)
        {
            commands_to_send.push(Box::new(command));
        }
        // Last, so the lookup records the view as it is after this frame.
        if let (Some(cluster_key), Some(target)) =
            (ui_state.selected_cluster, self.scale_to_request)
        {
            let inspector_ids = resource_detail_entry_ids(ui_state.global_blades.navigator());
            let command = ui_state.request_resource_scale(
                inspector_ids,
                cluster_key,
                target.api_resource,
                target.namespace,
                target.name,
            );
            commands_to_send.push(Box::new(command));
        }
    }
}

pub(super) fn show(
    ui: &mut egui::Ui,
    ui_state: &mut UiState,
    commands_to_send: &mut Vec<WorkerCommandBox>,
    shell_requests: &mut Vec<ShellRequest>,
    terminal_launch_settings: &TerminalLaunchSettings,
    table_preferences: &mut PersistedResourceTablePreferences,
    namespace_selector_settings: &NamespaceSelectorSettings,
) {
    let debug_image_presets = terminal_launch_settings.debug_image_presets.as_slice();
    let ctx = ui.ctx().clone();
    let mut effects = WorkspaceEffects::default();
    let mut resource_selection_action = None;
    // The resource whose inspector blade (opened from the table) is showing.
    let inspected = ui_state
        .global_blades
        .navigator()
        .and_then(|navigator| {
            navigator
                .entries()
                .find_map(|content| content.resource_detail())
        })
        .map(|entry| {
            (
                entry.cluster_key,
                entry.api_resource.clone(),
                entry.resource_uid.clone(),
            )
        });
    egui::CentralPanel::default()
        .frame(WorkspacePage::canvas_frame(&ui.tokens()))
        .show(ui, |ui| {
            WorkspacePage::show(ui, |ui| {
                if super::cluster_landing::is_shown(ui_state) {
                    super::cluster_landing::show(ui, ui_state, commands_to_send);
                    return;
                }
                let Some(cluster) = ui_state
                    .selected_cluster
                    .and_then(|key| ui_state.clusters.get_mut(&key))
                else {
                    return;
                };

                match &cluster.connection {
                    ClusterConnectionState::Failed(_) => {
                        effects.retry_requested = show_connection_progress(ui, cluster);
                        return;
                    }
                    ClusterConnectionState::Connecting | ClusterConnectionState::Connected
                        if !cluster.reachable() =>
                    {
                        show_connection_progress(ui, cluster);
                        return;
                    }
                    ClusterConnectionState::Disconnected => return,
                    ClusterConnectionState::Connecting | ClusterConnectionState::Connected => {}
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
                        namespace_selector_settings,
                    )
                    .map(|(name, namespace)| HelmReleaseDetailTarget { name, namespace });
                    return;
                }
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
                let compact = TopBar::new(ui.ctx().content_rect().width()).is_narrow();
                let (table_configuration, summary_counts) =
                    if let Some(api_resource) = &selected_api_resource {
                        let configuration = resource_table_configuration(
                            ui.available_width(),
                            api_resource,
                            cluster
                                .custom_resource_columns
                                .get(api_resource)
                                .map(Vec::as_slice)
                                .unwrap_or_default(),
                            api_resource.namespaced
                                && (cluster.watches_every_namespace(api_resource)
                                    || cluster.namespace_scope().len() > 1),
                            compact,
                            table_preferences,
                        );
                        prepare_cluster_table(
                            cluster,
                            api_resource,
                            &resource_search,
                            &configuration,
                        );
                        let prepared = cluster.resource_table_cache.prepared();
                        let counts = (
                            prepared.resource_count,
                            prepared.shown_count,
                            prepared.problem_count,
                            prepared
                                .groups
                                .iter()
                                .filter(|group| group.name.is_some())
                                .count(),
                        );
                        (Some(configuration), counts)
                    } else {
                        (None, (0, 0, 0, 0))
                    };
                let (resource_count, shown_count, problem_count, group_count) =
                    summary_counts;
                let toolbar_effects = show_toolbar(
                    ui,
                    cluster,
                    selected_api_resource.as_ref(),
                    ListSummary {
                        total: resource_count,
                        shown: shown_count,
                        namespaces: if selected_api_resource
                            .as_ref()
                            .is_some_and(|api_resource| api_resource.namespaced)
                        {
                            group_count
                        } else {
                            0
                        },
                        live: selected_api_resource.as_ref().is_some_and(|api_resource| {
                            // Nothing is watched until a namespace is chosen.
                            !(api_resource.namespaced
                                && cluster.selected_namespaces.is_nothing_selected())
                                && !selected_watches_are_loading(cluster, api_resource)
                                && selected_watch_error(cluster, api_resource).is_none()
                        }),
                    },
                    Some(ListFilters {
                        grouped_by_namespace: selected_api_resource
                            .as_ref()
                            .filter(|api_resource| api_resource.namespaced)
                            .map(|_| {
                                table_configuration
                                    .as_ref()
                                    .is_some_and(|configuration| {
                                        configuration.grouping == Grouping::Namespace
                                    })
                            }),
                        problem_count,
                        problems_only: resource_search.problems_only,
                    }),
                    &mut resource_search,
                    &mut effects.namespace_selection,
                    namespace_selector_settings,
                );
                if let Some(api_resource) = &selected_api_resource {
                    if toolbar_effects.toggle_grouping {
                        let key = ResourceTableKey::workspace(api_resource);
                        let grouped = table_preferences.groups_by_namespace(&key);
                        table_preferences.set_groups_by_namespace(&key, !grouped);
                        resource_search.problem_cursor = None;
                        // The table is configured before the toolbar: lay it out again.
                        ui.ctx().request_repaint();
                    }
                    if toolbar_effects.toggle_problems_only {
                        resource_search.problems_only = !resource_search.problems_only;
                        resource_search.problem_cursor = None;
                    }
                    cluster
                        .resource_searches
                        .insert(api_resource.clone(), resource_search);
                }

                let Some(api_resource) = &selected_api_resource else {
                    workspace_empty_state(
                        ui,
                        Icon::QueueList,
                        "Select a resource",
                        "Choose an API resource from the navigator to inspect it.",
                    );
                    return;
                };
                let table_configuration = table_configuration
                    .as_ref()
                    .expect("selected resources have a table configuration");
                let namespace_selection_empty =
                    api_resource.namespaced && cluster.selected_namespaces.is_nothing_selected();
                let watch_error = selected_watch_error(cluster, api_resource);
                let namespace_list_error = match awaited_namespace_list(cluster, api_resource) {
                    NamespaceListWait::Failed(error) => Some(error.to_owned()),
                    NamespaceListWait::Ready | NamespaceListWait::Loading => None,
                };
                let watches_are_loading = selected_watches_are_loading(cluster, api_resource);
                let resource_search = cluster
                    .resource_searches
                    .get(api_resource)
                    .expect("selected resource search was just stored")
                    .clone();
                prepare_cluster_table(cluster, api_resource, &resource_search, table_configuration);
                if let Some(step) = toolbar_effects.problem_step {
                    let resources = &mut cluster.resources;
                    if let Some(search) = resources.resource_searches.get_mut(api_resource) {
                        step_to_problem(resources.resource_table_cache.prepared(), search, step);
                    }
                }
                let resource_search = cluster
                    .resource_searches
                    .get(api_resource)
                    .expect("selected resource search was just stored")
                    .clone();
                let inspected_uid = inspected
                    .as_ref()
                    .filter(|(cluster_key, resource, _)| {
                        *cluster_key == cluster.cluster_key && resource == api_resource
                    })
                    .map(|(_, _, uid)| uid.clone());
                let metrics_shown = cluster.metrics_shown_as_available();
                let resources = &mut cluster.resources;
                let prepared = resources.resource_table_cache.prepared();
                let metrics = ResourceMetrics::new(
                    metrics_shown,
                    &resources.pod_metrics,
                    &resources.node_metrics,
                    &resources.node_pod_counts,
                );
                let mut clear_filter = false;
                let mut show_every_row = false;
                let mut table_shown = false;
                // The bar stays above every state while rows are selected, so the
                // selection can always be cleared or acted on.
                if selected_resource_count > 0 {
                    let selection = resources.resource_selections.get(api_resource);
                    let visible_selected = prepared
                        .rows
                        .iter()
                        .filter(|row| {
                            matches!(row, PreparedResourceTableRow::Resource(identity)
                                if selection.is_some_and(|selection| selection.contains(&identity.uid)))
                        })
                        .count();
                    show_bulk_selection_bar(
                        ui,
                        BulkSelection {
                            selected: selected_resource_count,
                            hidden: selected_resource_count.saturating_sub(visible_selected),
                            visible: prepared.visible_resource_count,
                            actions_enabled: resource_actions_enabled,
                            interleaved_logs: api_resource.is_pod(),
                        },
                        &mut resource_selection_action,
                    );
                }
                if namespace_selection_empty {
                    workspace_empty_state(
                        ui,
                        Icon::Folder,
                        "Choose a namespace",
                        "Select one or more namespaces to start watching resources.",
                    );
                } else if let Some(error) = &namespace_list_error {
                    // The namespace watcher keeps retrying by itself, so there is nothing to retry.
                    workspace_unavailable_state(ui, "Namespaces unavailable", error);
                } else if let Some(error) = watch_error {
                    effects.retry_requested =
                        workspace_error_state(ui, "Unable to load resources", &error);
                } else if watches_are_loading {
                    // The first list response: the table's header over skeleton rows (S08).
                    table_configuration
                        .table(api_resource)
                        .selectable()
                        .show_loading(ui, LOADING_ROWS, "Loading resources");
                } else if prepared.resource_count == 0 {
                    workspace_empty_state(
                        ui,
                        Icon::Cube,
                        "No resources found",
                        "This resource type has no items in the selected namespace scope.",
                    );
                } else if let Some(error) = &prepared.regex_error {
                    workspace_search_error_state(ui, error);
                } else if prepared.visible_resource_count == 0 {
                    if workspace_filtered_empty_state(
                        ui,
                        "No matching resources",
                        "Try a different search term.",
                    ) {
                        clear_filter = true;
                    }
                } else if prepared.shown_count == 0 {
                    if workspace_filtered_empty_state(
                        ui,
                        "No problems",
                        "Every matching resource is doing well.",
                    ) {
                        show_every_row = true;
                    }
                } else {
                    table_shown = true;
                    let outcome = show_resource_table(
                        ui,
                        api_resource,
                        prepared,
                        table_configuration,
                        ResourceTableOptions {
                            resource_cache: &resources.resource_cache,
                            metrics,
                            resource_navigation: &resources.resource_navigation,
                            actions: ResourceActionAvailability {
                                enabled: resource_actions_enabled,
                                supports_scale: resources
                                    .displayed_api_resources
                                    .is_scalable(api_resource),
                            },
                            debug_image_presets,
                            inspected_uid: inspected_uid.as_deref(),
                            collapsed_groups: &resource_search.collapsed_groups,
                            problem_cursor: resource_search.problem_cursor.as_deref(),
                            scroll_to: resource_search.scroll_to.as_deref(),
                            name_ordered: resource_search.query.is_empty(),
                        },
                        ResourceTableControls {
                            selection: resources
                                .resource_selections
                                .entry(api_resource.clone())
                                .or_default(),
                            table_preferences,
                            column_settings_to_open: &mut effects.column_settings_to_open,
                        },
                    );
                    if let Some(search) = resources.resource_searches.get_mut(api_resource) {
                        search.scroll_to = None;
                        if let Some(group) = outcome.toggled_group
                            && !search.collapsed_groups.remove(&group)
                        {
                            search.collapsed_groups.insert(group);
                        }
                        if let Some(uid) = outcome.clicked_problem {
                            search.problem_cursor = Some(uid.clone());
                            search.scroll_to = Some(uid);
                            ui.ctx().request_repaint();
                        }
                    }
                    if let Some(action) = outcome.action {
                    match action {
                        ResourceAction::OpenDetails {
                            name,
                            namespace,
                            uid,
                        } => {
                            if let Some(search) = resources.resource_searches.get_mut(api_resource) {
                                search.problem_cursor = None;
                            }
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
                            resources.pending_delete =
                                Some(PendingDelete::new(api_resource.clone(), name, namespace));
                        }
                        ResourceAction::RequestForceDelete {
                            name,
                            uid,
                            namespace,
                            finalizers,
                        } => {
                            resources.pending_force_delete = Some(PendingForceDelete::new(
                                api_resource.clone(),
                                name,
                                uid,
                                namespace,
                                finalizers,
                            ));
                        }
                        ResourceAction::RequestDeploymentRestart { name, namespace } => {
                            resources.pending_deployment_restart = Some(PendingDeploymentRestart {
                                resource_name: name,
                                namespace,
                            });
                        }
                        ResourceAction::RequestDeploymentRollback { name, namespace } => {
                            effects.deployment_rollback_to_request = Some((namespace, name));
                        }
                        ResourceAction::RequestCronJobRun { name, namespace } => {
                            resources.pending_cron_job_run = Some(PendingCronJobRun {
                                resource_name: name,
                                namespace,
                            });
                        }
                        ResourceAction::RequestScale { name, namespace } => {
                            effects.scale_to_request = Some(ScaleTarget {
                                api_resource: api_resource.clone(),
                                name,
                                namespace,
                            });
                        }
                        ResourceAction::RequestNodeCordon { name, cordon } => {
                            effects.node_cordon = Some((cluster.cluster_key, name, cordon));
                        }
                        ResourceAction::RequestNodeDrain { name } => {
                            resources.pending_node_drain =
                                Some(PendingNodeDrain { node_name: name });
                        }
                        ResourceAction::ViewLogs { targets } => {
                            effects.log_sources_to_open = Some(LogSourcesTarget {
                                cluster_key: cluster.cluster_key,
                                targets,
                            });
                        }
                        action @ (ResourceAction::Shell { .. }
                        | ResourceAction::PodDebugShell { .. }
                        | ResourceAction::NodeShell { .. }) => {
                            effects.shell_request = action.shell_request(&cluster.name);
                        }
                        ResourceAction::ConfigureDebugImages => {
                            effects.terminal_settings_to_open =
                                Some(terminal_launch_settings.clone());
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
                }

                if let Some(selection_action) = resource_selection_action.take() {
                    match selection_action {
                        ResourceSelectionAction::Clear => {
                            resources.resource_selections.remove(api_resource);
                        }
                        ResourceSelectionAction::Delete => {
                            let selected_uids = resources
                                .resource_selections
                                .get(api_resource)
                                .cloned()
                                .unwrap_or_default();
                            let targets = prepared
                                .watch_keys
                                .iter()
                                .filter_map(|watch_key| resources.resource_cache.get(watch_key))
                                .flat_map(|watch| watch.resources.values())
                                .filter(|resource| selected_uids.contains(&resource.uid))
                                .map(|resource| BulkDeleteTarget {
                                    uid: resource.uid.clone(),
                                    name: resource.name.clone(),
                                    namespace: resource.namespace.clone(),
                                })
                                .collect::<Vec<_>>();
                            if !targets.is_empty() {
                                resources.pending_bulk_delete =
                                    Some(PendingBulkDelete::new(api_resource.clone(), targets));
                            }
                        }
                        ResourceSelectionAction::ViewLogs => {
                            let selected_uids = resources
                                .resource_selections
                                .get(api_resource)
                                .cloned()
                                .unwrap_or_default();
                            let targets = prepared
                                .watch_keys
                                .iter()
                                .filter_map(|watch_key| resources.resource_cache.get(watch_key))
                                .flat_map(|watch| watch.resources.values())
                                .filter(|resource| selected_uids.contains(&resource.uid))
                                .flat_map(|resource| {
                                    log_stream_targets(resource, &resource.log_containers)
                                })
                                .collect::<Vec<_>>();
                            if !targets.is_empty() {
                                effects.log_sources_to_open = Some(LogSourcesTarget {
                                    cluster_key: cluster.cluster_key,
                                    targets,
                                });
                            }
                        }
                    }
                }
                // Ticking the first row or clearing the last one adds or removes the
                // bulk bar above the table: lay the frame out again rather than
                // showing the table where the pointer no longer is.
                let selected_after = resources
                    .resource_selections
                    .get(api_resource)
                    .map_or(0, HashSet::len);
                if (selected_resource_count > 0) != (selected_after > 0) {
                    ui.ctx().request_discard("the bulk selection bar appeared or went");
                }
                if let Some(search) = cluster.resource_searches.get_mut(api_resource) {
                    if clear_filter {
                        search.query.clear();
                    }
                    if clear_filter || show_every_row {
                        search.problems_only = false;
                    }
                    // A scroll that found no table to scroll must not linger.
                    if !table_shown {
                        search.scroll_to = None;
                    }
                }
            });
        });

    effects.apply(ui_state, &ctx, commands_to_send, shell_requests);
}

mod connection_progress;
mod helm;
mod resource_data;
mod resource_table;
mod toolbar;

use connection_progress::show_connection_progress;
use helm::*;
pub(super) use resource_data::listed_count;
use resource_data::*;
use resource_table::*;
use toolbar::*;

#[cfg(test)]
mod tests;
