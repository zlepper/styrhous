use super::resource_actions::show_resource_action_items;
use super::resource_owner;
use super::state::{
    BulkDeleteTarget, ClusterConnectionState, ClusterLoadState, PendingBulkDelete, PendingDelete,
    PendingDeploymentRestart, PendingForceDelete, ResourceAction, ResourceSearchState, UiState,
};
use super::table_preferences::{
    MetadataColumnSource, PersistedResourceTablePreferences, ResourceTableKey,
    TableColumnDefinition,
};
use super::widgets::{
    show_resource_cell, workspace_empty_state, workspace_error_state, workspace_loading_state,
    workspace_search_error_state,
};
use crate::minimal_namespace::MinimalNamespace;
use crate::minimal_resource::MinimalResource;
use crate::pod_metrics::{format_cpu, format_memory};
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
use components::fuzzy::{matches_fuzzy, normalize_for_search};
use components::{
    ButtonSize, MoreButton, SelectionAction, TableRowBuilder, TailwindButton, TailwindCombobox,
    TailwindSearchInput, TailwindTable, WorkspacePage,
};
use egui_extras::{Size, StripBuilder};
use std::cell::RefCell;
use std::collections::{BTreeSet, HashSet};

const RESOURCE_SEARCH_WIDTH: f32 = 210.0;
const TOOLBAR_RIGHT_INSET: f32 = spacing::XL;
const TOOLBAR_HEIGHT: f32 = 52.0;
const TOOLBAR_CONTENT_HEIGHT: f32 = 36.0;
const TOOLBAR_VERTICAL_PADDING: f32 = (TOOLBAR_HEIGHT - TOOLBAR_CONTENT_HEIGHT) / 2.0;
const RESOURCE_TABLE_SELECTION_WIDTH: f32 = 48.0;

struct FilteredResources {
    resources: Vec<MinimalResource>,
    regex_error: Option<String>,
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

pub(super) fn show(
    ui: &mut egui::Ui,
    ui_state: &mut UiState,
    commands_to_send: &mut Vec<WorkerCommandBox>,
    shell_requests: &mut Vec<ShellRequest>,
    debug_image_presets: &[DebugImagePreset],
    table_preferences: &mut PersistedResourceTablePreferences,
) {
    let ctx = ui.ctx().clone();
    let mut namespace_selection = None;
    let mut retry_requested = false;
    let mut detail_to_open = None;
    let mut log_to_open = None;
    let mut yaml_to_open = None;
    let mut shell_to_open = None;
    let mut resource_selection_action = None;
    let mut column_settings_to_open = None;
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
                        retry_requested = workspace_error_state(ui, "Unable to connect", error);
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
                        retry_requested =
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
                let all_resources = decorate_pod_usage_rows(
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
                    &mut namespace_selection,
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
                    retry_requested = workspace_error_state(ui, "Unable to load resources", &error);
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
                } else if let Some(action) = show_resource_table(
                    ui,
                    api_resource,
                    &filtered_resources.resources,
                    ResourceTableOptions {
                        custom_columns: cluster
                            .custom_resource_columns
                            .get(api_resource)
                            .map(Vec::as_slice)
                            .unwrap_or_default(),
                        metadata_suggestion_resources: &all_resources,
                        resource_navigation: &cluster.resource_navigation,
                        hidden_resource_count: all_resources.len()
                            - filtered_resources.resources.len(),
                        show_namespace_column: api_resource.namespaced
                            && cluster.selected_namespaces.len() > 1,
                        actions: ResourceActionAvailability {
                            enabled: resource_actions_enabled,
                            supports_scale: cluster.scalable_api_resources.contains(api_resource),
                        },
                        debug_image_presets,
                    },
                    cluster
                        .resource_selections
                        .entry(api_resource.clone())
                        .or_default(),
                    table_preferences,
                    &mut column_settings_to_open,
                ) {
                    match action {
                        ResourceAction::OpenDetails {
                            name,
                            namespace,
                            uid,
                        } => {
                            detail_to_open = Some((api_resource.clone(), name, namespace, uid));
                        }
                        ResourceAction::EditYaml { name, namespace } => {
                            yaml_to_open =
                                Some((cluster.cluster_key, api_resource.clone(), namespace, name));
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
                            log_to_open = Some((cluster.cluster_key, name, namespace, container));
                        }
                        action @ (ResourceAction::Shell { .. }
                        | ResourceAction::PodDebugShell { .. }
                        | ResourceAction::NodeShell { .. }) => {
                            shell_to_open = action.shell_request(&cluster.name);
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
                            detail_to_open = Some((api_resource, name, namespace, uid));
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

    if let (Some(cluster_key), Some((api_resource, name, namespace, uid))) =
        (ui_state.selected_cluster, detail_to_open)
    {
        ui_state.open_resource_detail(
            cluster_key,
            api_resource,
            name,
            namespace,
            uid,
            commands_to_send,
        );
    }
    if let Some(target) = column_settings_to_open {
        ui_state.replace_global_blade(Box::new(target), commands_to_send);
    }
    if let Some((cluster_key, name, namespace, container)) = log_to_open {
        ui_state.open_pod_log_window(cluster_key, name, namespace, container, commands_to_send);
    }
    if let Some((cluster_key, api_resource, namespace, resource_name)) = yaml_to_open {
        ui_state.open_yaml_editor(
            &ctx,
            cluster_key,
            api_resource,
            namespace,
            resource_name,
            commands_to_send,
        );
    }
    shell_requests.extend(shell_to_open);
    if let (Some(cluster_key), Some(selection)) = (ui_state.selected_cluster, namespace_selection) {
        match selection {
            NamespaceSelection::Replace(namespace) => {
                ui_state.replace_selected_namespaces(cluster_key, [namespace], commands_to_send);
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
            if let Some(resource_selection) = cluster.resource_selections.get_mut(&api_resource) {
                resource_selection.retain(|uid| visible_uids.contains(uid));
            }
        }
    }
    if retry_requested && let Some(cluster_key) = ui_state.selected_cluster {
        ui_state.retry_selected_load(cluster_key, commands_to_send);
    }
}

fn selected_watch_error(
    cluster: &super::state::ClusterState,
    api_resource: &crate::api_resource::ApiResource,
) -> Option<String> {
    resource_watch_namespaces(cluster, api_resource)
        .into_iter()
        .find_map(|namespace| {
            cluster
                .resource_cache
                .get(&(api_resource.clone(), namespace))
                .and_then(|watch| watch.error.clone())
        })
}

fn selected_watches_are_loading(
    cluster: &super::state::ClusterState,
    api_resource: &crate::api_resource::ApiResource,
) -> bool {
    resource_watch_namespaces(cluster, api_resource)
        .into_iter()
        .any(|namespace| {
            cluster
                .resource_cache
                .get(&(api_resource.clone(), namespace))
                .is_none_or(|watch| !watch.is_synced)
        })
}

fn selected_resources(
    cluster: &super::state::ClusterState,
    api_resource: Option<&crate::api_resource::ApiResource>,
) -> Vec<MinimalResource> {
    let Some(api_resource) = api_resource else {
        return Vec::new();
    };
    let mut resources = Vec::new();
    for namespace in resource_watch_namespaces(cluster, api_resource) {
        if let Some(state) = cluster
            .resource_cache
            .get(&(api_resource.clone(), namespace))
        {
            resources.extend(state.resources.values().cloned());
        }
    }
    resources.sort_by_key(|resource| resource.name.to_lowercase());
    resources
}

fn decorate_pod_usage_rows(
    cluster: &super::state::ClusterState,
    api_resource: Option<&crate::api_resource::ApiResource>,
    mut resources: Vec<MinimalResource>,
) -> Vec<MinimalResource> {
    let is_pod =
        api_resource.is_some_and(|resource| resource.group == "core" && resource.kind == "Pod");
    if !is_pod {
        return resources;
    }
    for resource in &mut resources {
        let Some(namespace) = resource.namespace.as_deref() else {
            continue;
        };
        let metrics = cluster.pod_metrics.get(namespace);
        if !cluster.pod_metrics_api_available
            || metrics.is_some_and(|metrics| metrics.error.is_some())
        {
            resource
                .cells
                .insert(CPU_COLUMN.into(), CellValue::Text("Unavailable".into()));
            resource
                .cells
                .insert(MEMORY_COLUMN.into(), CellValue::Text("Unavailable".into()));
        } else if let Some(usage) = metrics.and_then(|metrics| metrics.usages.get(&resource.name)) {
            resource.cells.insert(
                CPU_COLUMN.into(),
                CellValue::Usage {
                    label: format_cpu(usage.cpu_nanocores),
                    value: usage.cpu_nanocores,
                },
            );
            resource.cells.insert(
                MEMORY_COLUMN.into(),
                CellValue::Usage {
                    label: format_memory(usage.memory_bytes),
                    value: usage.memory_bytes,
                },
            );
        }
    }
    resources
}

fn resource_watch_namespaces(
    cluster: &super::state::ClusterState,
    api_resource: &crate::api_resource::ApiResource,
) -> Vec<Option<String>> {
    if api_resource.namespaced {
        cluster
            .selected_namespaces
            .iter()
            .cloned()
            .map(Some)
            .collect()
    } else {
        vec![None]
    }
}

fn show_toolbar(
    ui: &mut egui::Ui,
    cluster: &super::state::ClusterState,
    selected_api_resource: Option<&crate::api_resource::ApiResource>,
    all_resources: &[MinimalResource],
    resource_search: &mut ResourceSearchState,
    namespace_selection: &mut Option<NamespaceSelection>,
    selection_controls: ResourceSelectionControls<'_>,
) -> FilteredResources {
    let selected_text = match cluster.selected_namespaces.len() {
        0 => "Select namespaces".to_owned(),
        1 => cluster
            .selected_namespaces
            .iter()
            .next()
            .cloned()
            .unwrap_or_default(),
        count => format!("{count} namespaces"),
    };
    let namespaces: Vec<&MinimalNamespace> = cluster.namespaces.values().collect();
    let all_namespaces_selected = !namespaces.is_empty()
        && namespaces
            .iter()
            .all(|namespace| cluster.selected_namespaces.contains(&namespace.name));
    let selected_status = if !selected_api_resource.is_some_and(|resource| resource.namespaced) {
        selected_api_resource.map(|api_resource| {
            cluster
                .active_watchers
                .contains(&(api_resource.clone(), None))
        })
    } else if cluster.selected_namespaces.len() == 1 {
        selected_api_resource.map(|api_resource| {
            let namespace = cluster
                .selected_namespaces
                .iter()
                .next()
                .expect("selection length was checked");
            cluster
                .active_watchers
                .contains(&(api_resource.clone(), Some(namespace.clone())))
        })
    } else {
        None
    };

    let mut filtered_resources = filter_resources(all_resources, resource_search);

    ui.add_space(TOOLBAR_VERTICAL_PADDING);
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), TOOLBAR_CONTENT_HEIGHT),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            StripBuilder::new(ui)
                .size(Size::exact(360.0))
                .size(Size::remainder())
                .size(Size::exact(
                    selected_api_resource.map_or(0.0, |_| RESOURCE_SEARCH_WIDTH),
                ))
                .size(Size::exact(TOOLBAR_RIGHT_INSET))
                .clip(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .horizontal(|mut strip| {
                    strip.cell(|ui| {
                        ui.add_space(37.0);
                        if selected_api_resource.is_some_and(|resource| !resource.namespaced) {
                            ui.label(
                                egui::RichText::new("Scope")
                                    .font(typography::body())
                                    .color(gray::_700),
                            );
                            ui.add_space(7.0);
                            ui.label(
                                egui::RichText::new("Cluster-wide")
                                    .font(typography::body())
                                    .color(gray::_700),
                            );
                        } else {
                            ui.label(
                                egui::RichText::new("Namespace")
                                    .font(typography::body())
                                    .color(gray::_700),
                            );
                            ui.add_space(7.0);
                            let namespace_response = TailwindCombobox::new("namespace-selector")
                                .placeholder("Search namespaces...")
                                .search_accessibility_label("Search Namespace")
                                .selected_text(selected_text)
                                .selected_status(selected_status)
                                .width(230.0)
                                .compact()
                                .select_all(all_namespaces_selected)
                                .filter_by(|ns: &&MinimalNamespace| ns.get_name_to_display())
                                .show_items(ui, &namespaces, |cb, ns| {
                                    let status = selected_api_resource.map(|api_resource| {
                                        cluster.active_watchers.contains(&(
                                            api_resource.clone(),
                                            Some(ns.name.clone()),
                                        ))
                                    });
                                    if let Some(action) = cb
                                        .item_with_status(
                                            ns.get_name_to_display(),
                                            cluster.selected_namespaces.contains(&ns.name),
                                            status,
                                        )
                                        .selection_action()
                                    {
                                        *namespace_selection = Some(match action {
                                            SelectionAction::Replace => {
                                                NamespaceSelection::Replace(ns.name.clone())
                                            }
                                            SelectionAction::Toggle => {
                                                NamespaceSelection::Toggle(ns.name.clone())
                                            }
                                        });
                                    }
                                });
                            if namespace_response.select_all_clicked {
                                *namespace_selection = Some(if all_namespaces_selected {
                                    NamespaceSelection::ClearAll
                                } else {
                                    NamespaceSelection::SelectAll
                                });
                            }
                            namespace_response.response.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::WidgetType::ComboBox,
                                    ui.is_enabled(),
                                    "Namespace",
                                )
                            });
                        }
                    });
                    strip.cell(|ui| {
                        if selected_api_resource.is_some() {
                            ui.add_space(15.0);
                            ui.separator();
                            ui.add_space(18.0);
                            if selection_controls.selected_count == 0 {
                                ui.label(
                                    egui::RichText::new(resource_count_label(
                                        all_resources.len(),
                                        filtered_resources.resources.len(),
                                        !resource_search.query.is_empty(),
                                    ))
                                    .font(typography::section_heading())
                                    .color(gray::_500),
                                );
                            } else {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{} selected",
                                        selection_controls.selected_count
                                    ))
                                    .font(typography::section_heading())
                                    .color(gray::_700),
                                );
                                ui.add_space(spacing::MD);
                                if TailwindButton::secondary("Clear selection")
                                    .size(ButtonSize::Xs)
                                    .show(ui)
                                    .clicked()
                                {
                                    *selection_controls.action =
                                        Some(ResourceSelectionAction::Clear);
                                }
                                let delete =
                                    TailwindButton::danger("Delete selected").size(ButtonSize::Xs);
                                if ui
                                    .add_enabled_ui(selection_controls.actions_enabled, |ui| {
                                        delete.show(ui)
                                    })
                                    .inner
                                    .clicked()
                                {
                                    *selection_controls.action =
                                        Some(ResourceSelectionAction::Delete);
                                }
                            }
                        }
                    });
                    strip.cell(|ui| {
                        if selected_api_resource.is_some() {
                            ui.allocate_ui_with_layout(
                                egui::vec2(RESOURCE_SEARCH_WIDTH, 36.0),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| show_resource_search(ui, resource_search),
                            );
                            filtered_resources = filter_resources(all_resources, resource_search);
                        }
                    });
                    strip.empty();
                });
        },
    );

    filtered_resources
}

fn show_resource_search(ui: &mut egui::Ui, resource_search: &mut ResourceSearchState) {
    let invalid = regex_error(resource_search).is_some();
    let focus_search = ui
        .ctx()
        .input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::F));
    TailwindSearchInput::new(&mut resource_search.query, &mut resource_search.regex_mode)
        .hint_text("Search resources...")
        .id_salt("resource-search-input")
        .accessibility_label("Search resources")
        .invalid(invalid)
        .focus(focus_search)
        .show(ui);
}

fn filter_resources(
    all_resources: &[MinimalResource],
    resource_search: &ResourceSearchState,
) -> FilteredResources {
    if resource_search.query.is_empty() {
        return FilteredResources {
            resources: all_resources.to_vec(),
            regex_error: None,
        };
    }

    if resource_search.regex_mode {
        let regex = match regex::RegexBuilder::new(&resource_search.query)
            .case_insensitive(true)
            .build()
        {
            Ok(regex) => regex,
            Err(error) => {
                return FilteredResources {
                    resources: Vec::new(),
                    regex_error: Some(error.to_string()),
                };
            }
        };
        return FilteredResources {
            resources: all_resources
                .iter()
                .filter(|resource| {
                    let normalized_name: String = normalize_for_search(&resource.name).collect();
                    regex.is_match(&normalized_name)
                })
                .cloned()
                .collect(),
            regex_error: None,
        };
    }

    let query: Vec<char> = normalize_for_search(&resource_search.query).collect();
    FilteredResources {
        resources: all_resources
            .iter()
            .filter(|resource| matches_fuzzy(&resource.name, &query))
            .cloned()
            .collect(),
        regex_error: None,
    }
}

fn regex_error(resource_search: &ResourceSearchState) -> Option<String> {
    if resource_search.regex_mode && !resource_search.query.is_empty() {
        regex::RegexBuilder::new(&resource_search.query)
            .case_insensitive(true)
            .build()
            .err()
            .map(|error| format!("Invalid regular expression: {error}"))
    } else {
        None
    }
}

fn resource_count_label(
    total_count: usize,
    visible_count: usize,
    search_is_active: bool,
) -> String {
    if search_is_active {
        format!("{visible_count} of {total_count} items")
    } else {
        format!("{total_count} items")
    }
}

fn show_resource_table(
    ui: &mut egui::Ui,
    api_resource: &crate::api_resource::ApiResource,
    resources: &[MinimalResource],
    options: ResourceTableOptions<'_>,
    selection: &mut HashSet<String>,
    table_preferences: &mut PersistedResourceTablePreferences,
    column_settings_to_open: &mut Option<
        super::resource_table_settings::ResourceTableSettingsTarget,
    >,
) -> Option<ResourceAction> {
    let pending_action = RefCell::new(None);
    let definition = table_definition(api_resource, options.custom_columns);
    let table_key = ResourceTableKey::workspace(api_resource);
    let metadata_columns = table_preferences.custom_columns(&table_key);
    let mut column_definitions = vec![TableColumnDefinition {
        id: "name".into(),
        label: "Name".into(),
        default_width: 160.0,
        sortable: true,
    }];
    if options.show_namespace_column {
        column_definitions.push(TableColumnDefinition {
            id: "namespace".into(),
            label: "Namespace".into(),
            default_width: 180.0,
            sortable: true,
        });
    }
    column_definitions.push(TableColumnDefinition {
        id: "owner".into(),
        label: "Owner".into(),
        default_width: 160.0,
        sortable: true,
    });
    column_definitions.extend(
        definition
            .columns
            .iter()
            .map(|column| TableColumnDefinition {
                id: column.id.clone(),
                label: column.label.clone(),
                default_width: column.initial_width,
                sortable: true,
            }),
    );
    column_definitions.extend(metadata_columns.iter().map(|column| TableColumnDefinition {
        id: column.id(),
        label: column.label.clone(),
        default_width: 160.0,
        sortable: true,
    }));
    column_definitions.extend([
        TableColumnDefinition {
            id: "age".into(),
            label: "Age".into(),
            default_width: 77.0,
            sortable: true,
        },
        TableColumnDefinition {
            id: "actions".into(),
            label: "Actions".into(),
            default_width: 104.0,
            sortable: false,
        },
    ]);
    let fixed_width = RESOURCE_TABLE_SELECTION_WIDTH
        + column_definitions
            .iter()
            .skip(1)
            .map(|column| column.default_width)
            .sum::<f32>();
    column_definitions[0].default_width = (ui.available_width() - fixed_width - 16.0).max(160.0);
    let visible_columns = table_preferences.resolved_columns(&table_key, &column_definitions);
    let sort_state = table_preferences
        .sort(&table_key, &column_definitions)
        .map(|(column_id, direction)| components::SortState::new(column_id, direction));
    let mut resource_rows = resources.iter().collect::<Vec<_>>();
    if let Some(sort) = &sort_state {
        resource_rows.sort_by(|left, right| {
            compare_resource_column(
                left,
                right,
                &sort.column_id,
                sort.direction,
                &metadata_columns,
            )
        });
    }
    let mut rows = resource_rows
        .into_iter()
        .map(ResourceTableRow::Resource)
        .collect::<Vec<_>>();
    if options.hidden_resource_count > 0 {
        rows.push(ResourceTableRow::HiddenBySearch(
            options.hidden_resource_count,
        ));
    }
    let node_column_index = visible_columns
        .iter()
        .position(|column| column.definition.id == NODE_COLUMN);
    let mut table = TailwindTable::new(format!(
        "resource-table-{}-{}-{}",
        api_resource.group, api_resource.version, api_resource.name
    ));
    for column in &visible_columns {
        table = table.column(
            column.definition.id.clone(),
            column.definition.label.clone(),
            |builder| {
                let builder = builder.initial_width(column.width);
                if column.definition.sortable {
                    builder.sortable()
                } else {
                    builder
                }
            },
        );
    }
    table = table.selectable().fill_available_height();

    let table_preferences = RefCell::new(table_preferences);
    table.show_selectable_configurable_with_row_response(
        ui,
        &rows,
        selection,
        |row| match row {
            ResourceTableRow::Resource(resource) => Some(resource.uid.clone()),
            ResourceTableRow::HiddenBySearch(_) => None,
        },
        sort_state.as_ref(),
        |header, id, _label, sortable| {
            MoreButton::show_context_menu(header, |menu| {
                if sortable {
                    if menu.action("Sort ascending").clicked() {
                        table_preferences.borrow_mut().set_sort(
                            &table_key,
                            &column_definitions,
                            id,
                            components::SortDirection::Ascending,
                        );
                    }
                    if menu.action("Sort descending").clicked() {
                        table_preferences.borrow_mut().set_sort(
                            &table_key,
                            &column_definitions,
                            id,
                            components::SortDirection::Descending,
                        );
                    }
                    menu.separator();
                }
                if menu.action("Configure columns").clicked() {
                    *column_settings_to_open = Some(
                        super::resource_table_settings::target_with_metadata_key_suggestions(
                            &mut table_preferences.borrow_mut(),
                            table_key.clone(),
                            &column_definitions,
                            metadata_key_suggestions(options.metadata_suggestion_resources),
                        ),
                    );
                }
            });
        },
        |id, width| {
            table_preferences
                .borrow_mut()
                .set_width(&table_key, &column_definitions, id, width)
        },
        |ui, row, column_index| {
            let column_id = &visible_columns[column_index].definition.id;
            match row {
                ResourceTableRow::Resource(resource) => match column_id.as_str() {
                    "name" if options.actions.enabled => {
                        let response = TableRowBuilder::clickable_text(
                            ui,
                            &resource.name,
                            gray::_900,
                            format!("Open details for {}", resource.name),
                        );
                        if response.clicked() && pending_action.borrow().is_none() {
                            *pending_action.borrow_mut() = Some(ResourceAction::OpenDetails {
                                name: resource.name.clone(),
                                namespace: resource.namespace.clone(),
                                uid: resource.uid.clone(),
                            });
                        }
                        MoreButton::show_context_menu(&response, |menu| {
                            show_resource_action_items(
                                menu,
                                api_resource,
                                resource,
                                &resource.log_containers,
                                options.debug_image_presets,
                                options.actions.supports_scale,
                                &mut pending_action.borrow_mut(),
                            );
                        });
                    }
                    "name" => TableRowBuilder::text(ui, &resource.name, true),
                    "namespace" => {
                        TableRowBuilder::text(
                            ui,
                            resource.namespace.as_deref().unwrap_or("-"),
                            false,
                        );
                    }
                    "owner" => {
                        let Some(owner) = &resource.controller_owner else {
                            TableRowBuilder::text(ui, "-", false);
                            return;
                        };
                        let label = owner.label();
                        if let Some(action) = resource_owner::navigation_action(
                            options.resource_navigation,
                            owner,
                            resource.namespace.as_deref(),
                        ) {
                            if options.actions.enabled {
                                let response = TableRowBuilder::clickable_text(
                                    ui,
                                    &label,
                                    components::colors::indigo::_600,
                                    format!("Open details for {label}"),
                                );
                                response.clone().on_hover_text(&label);
                                if response.clicked() {
                                    resource_owner::queue_navigation_action(
                                        &mut pending_action.borrow_mut(),
                                        action,
                                    );
                                }
                            } else {
                                TableRowBuilder::text(ui, &label, false);
                            }
                        } else {
                            ui.label(
                                egui::RichText::new(label)
                                    .font(typography::body())
                                    .color(components::colors::gray::_500),
                            )
                            .on_hover_text(resource_owner::unavailable_tooltip(owner));
                        }
                    }
                    id if metadata_columns.iter().any(|column| column.id() == id) => {
                        let column = metadata_columns
                            .iter()
                            .find(|column| column.id() == id)
                            .expect("metadata column was checked");
                        show_metadata_cell(
                            ui,
                            resource_metadata_value(resource, column.source, &column.key)
                                .unwrap_or("-"),
                        );
                    }
                    id if definition.columns.iter().any(|column| column.id == id) => {
                        let column = definition
                            .columns
                            .iter()
                            .find(|column| column.id == id)
                            .expect("resource column was checked");
                        if column.id == NODE_COLUMN
                            && api_resource.kind == "Pod"
                            && let Some(CellValue::Text(node_name)) = resource.cells.get(&column.id)
                        {
                            if options.actions.enabled && node_name != "-" {
                                let response = TableRowBuilder::clickable_text(
                                    ui,
                                    node_name,
                                    components::colors::indigo::_600,
                                    format!("Open details for Node {node_name}"),
                                );
                                if response.clicked() && pending_action.borrow().is_none() {
                                    *pending_action.borrow_mut() =
                                        Some(ResourceAction::NavigateDetails {
                                            api_resource:
                                                crate::resource_handlers::node::api_resource(),
                                            name: node_name.clone(),
                                            namespace: None,
                                            uid: node_name.clone(),
                                        });
                                }
                                MoreButton::show_context_menu(&response, |menu| {
                                    show_resource_action_items(
                                        menu,
                                        api_resource,
                                        resource,
                                        &resource.log_containers,
                                        options.debug_image_presets,
                                        options.actions.supports_scale,
                                        &mut pending_action.borrow_mut(),
                                    );
                                });
                            } else {
                                TableRowBuilder::text(ui, node_name, false);
                            }
                        } else {
                            show_resource_cell(ui, resource.cells.get(&column.id));
                        }
                    }
                    "age" => TableRowBuilder::text(ui, &resource.age(), false),
                    "actions" if options.actions.enabled => {
                        show_resource_actions(
                            ui,
                            api_resource,
                            resource,
                            options.actions.supports_scale,
                            options.debug_image_presets,
                            &mut pending_action.borrow_mut(),
                        );
                    }
                    _ => {}
                },
                ResourceTableRow::HiddenBySearch(hidden_count) if column_index == 0 => {
                    let label = if *hidden_count == 1 {
                        "1 resource hidden by search".to_owned()
                    } else {
                        format!("{hidden_count} resources hidden by search")
                    };
                    TableRowBuilder::text(ui, &label, false);
                }
                _ => {}
            }
        },
        |row_response, row, column_index| {
            if let ResourceTableRow::Resource(resource) = row {
                let column_id = &visible_columns[column_index].definition.id;
                if options.actions.enabled
                    && column_id != "actions"
                    && row_response.clicked()
                    && pending_action.borrow().is_none()
                {
                    *pending_action.borrow_mut() = Some(ResourceAction::OpenDetails {
                        name: resource.name.clone(),
                        namespace: resource.namespace.clone(),
                        uid: resource.uid.clone(),
                    });
                }
                if Some(column_index) == node_column_index
                    && let Some(CellValue::Text(node_name)) = resource.cells.get(NODE_COLUMN)
                    && node_name == "-"
                {
                    row_response
                        .clone()
                        .on_hover_text("Kubernetes has not assigned this Pod to a Node.");
                }
                if options.actions.enabled {
                    MoreButton::show_context_menu(row_response, |menu| {
                        show_resource_action_items(
                            menu,
                            api_resource,
                            resource,
                            &resource.log_containers,
                            options.debug_image_presets,
                            options.actions.supports_scale,
                            &mut pending_action.borrow_mut(),
                        );
                    });
                }
            }
        },
    );
    pending_action.into_inner()
}

fn show_resource_actions(
    ui: &mut egui::Ui,
    api_resource: &crate::api_resource::ApiResource,
    resource: &MinimalResource,
    supports_scale: bool,
    debug_image_presets: &[DebugImagePreset],
    pending_action: &mut Option<ResourceAction>,
) {
    let mut action_ui = ui.new_child(
        egui::UiBuilder::new()
            // The table cell's content cursor sits below the row centre after
            // its horizontal padding has been applied. Keep the square action
            // control visually centred with the row's text and status marker.
            // The horizontal inset makes Actions read as its own column rather
            // than an extension of the Age value.
            .max_rect(
                ui.max_rect()
                    .shrink2(egui::vec2(28.0, 0.0))
                    .translate(egui::vec2(0.0, -8.0)),
            )
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    MoreButton::new(format!("More actions for {}", resource.name)).show(&mut action_ui, |menu| {
        show_resource_action_items(
            menu,
            api_resource,
            resource,
            &resource.log_containers,
            debug_image_presets,
            supports_scale,
            pending_action,
        );
    });
}

fn compare_resource_column(
    left: &MinimalResource,
    right: &MinimalResource,
    column_id: &str,
    direction: components::SortDirection,
    metadata_columns: &[super::table_preferences::CustomMetadataColumn],
) -> std::cmp::Ordering {
    let value = |resource: &MinimalResource| match column_id {
        "name" => SortValue::Text(resource.name.clone()),
        "namespace" => SortValue::Text(resource.namespace.clone().unwrap_or_default()),
        "owner" => SortValue::Text(
            resource
                .controller_owner
                .as_ref()
                .map(|owner| owner.label())
                .unwrap_or_default(),
        ),
        "age" => resource
            .creation_timestamp
            .map(|time| SortValue::Number(time.unix_timestamp()))
            .unwrap_or(SortValue::Empty),
        id => metadata_columns
            .iter()
            .find(|column| column.id() == id)
            .and_then(|column| resource_metadata_value(resource, column.source, &column.key))
            .map(|value| SortValue::Text(value.to_owned()))
            .or_else(|| resource.cells.get(id).map(cell_sort_value))
            .unwrap_or(SortValue::Empty),
    };
    let left_value = value(left);
    let right_value = value(right);
    let ordering = compare_sort_values(left_value, right_value, direction);
    ordering
        .then_with(|| left.name.cmp(&right.name))
        .then_with(|| left.uid.cmp(&right.uid))
}

fn resource_metadata_value<'a>(
    resource: &'a MinimalResource,
    source: MetadataColumnSource,
    key: &str,
) -> Option<&'a str> {
    match source {
        MetadataColumnSource::Label => resource.labels.get(key),
        MetadataColumnSource::Annotation => resource.annotations.get(key),
    }
    .map(String::as_str)
}

fn show_metadata_cell(ui: &mut egui::Ui, value: &str) {
    ui.add(
        egui::Label::new(
            egui::RichText::new(value)
                .font(typography::body())
                .color(gray::_500),
        )
        .truncate(),
    )
    .on_hover_text(value);
}

fn metadata_key_suggestions(
    resources: &[MinimalResource],
) -> super::resource_table_settings::MetadataKeySuggestions {
    super::resource_table_settings::MetadataKeySuggestions {
        labels: resources
            .iter()
            .flat_map(|resource| resource.labels.keys().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        annotations: resources
            .iter()
            .flat_map(|resource| resource.annotations.keys().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::rc::Rc;

    fn resource(name: &str) -> MinimalResource {
        MinimalResource {
            uid: name.into(),
            name: name.into(),
            namespace: Some("default".into()),
            creation_timestamp: None,
            controller_owner: None,
            labels: Default::default(),
            annotations: Default::default(),
            cells: Default::default(),
            log_containers: Vec::new(),
        }
    }

    #[test]
    fn fuzzy_search_matches_normalized_resource_names() {
        let resources = vec![resource("Café-API"), resource("worker")];
        let filtered = filter_resources(
            &resources,
            &ResourceSearchState {
                query: "cfa".into(),
                regex_mode: false,
            },
        );

        assert_eq!(
            filtered
                .resources
                .iter()
                .map(|resource| resource.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Café-API"]
        );
        assert!(filtered.regex_error.is_none());
    }

    #[test]
    fn regex_search_matches_normalized_resource_names_case_insensitively() {
        let resources = vec![resource("Café-API"), resource("worker")];
        let filtered = filter_resources(
            &resources,
            &ResourceSearchState {
                query: "CAFE-.*".into(),
                regex_mode: true,
            },
        );

        assert_eq!(filtered.resources.len(), 1);
        assert_eq!(filtered.resources[0].name, "Café-API");
        assert!(filtered.regex_error.is_none());
    }

    #[test]
    fn invalid_regex_has_no_results_and_an_error() {
        let filtered = filter_resources(
            &[resource("pod")],
            &ResourceSearchState {
                query: "[".into(),
                regex_mode: true,
            },
        );

        assert!(filtered.resources.is_empty());
        assert!(
            filtered
                .regex_error
                .as_deref()
                .is_some_and(|error| error.starts_with("regex parse error:"))
        );
    }

    #[test]
    fn metadata_suggestions_are_sorted_and_cover_labels_and_annotations() {
        let mut first = resource("first");
        first.labels = BTreeMap::from([("app".into(), "api".into())]);
        first.annotations = BTreeMap::from([("example.com/team".into(), "platform".into())]);
        let mut second = resource("second");
        second.labels = BTreeMap::from([
            ("app".into(), "worker".into()),
            ("tier".into(), "backend".into()),
        ]);
        second.annotations = BTreeMap::from([("example.com/owner".into(), "ops".into())]);

        let suggestions = metadata_key_suggestions(&[first, second]);

        assert_eq!(suggestions.labels, ["app", "tier"]);
        assert_eq!(
            suggestions.annotations,
            ["example.com/owner", "example.com/team"]
        );
    }

    #[test]
    fn custom_metadata_columns_render_and_sort_by_metadata_values() {
        let mut api = resource("api");
        api.labels.insert("app".into(), "api".into());
        let worker = resource("worker");
        let column = super::super::table_preferences::CustomMetadataColumn {
            source: MetadataColumnSource::Label,
            key: "app".into(),
            label: "Application".into(),
        };

        assert_eq!(
            resource_metadata_value(&api, column.source, &column.key),
            Some("api")
        );
        assert_eq!(
            resource_metadata_value(&worker, column.source, &column.key),
            None
        );
        assert_eq!(
            compare_resource_column(
                &api,
                &worker,
                &column.id(),
                components::SortDirection::Ascending,
                std::slice::from_ref(&column),
            ),
            std::cmp::Ordering::Less
        );

        let annotation_column = super::super::table_preferences::CustomMetadataColumn {
            source: MetadataColumnSource::Annotation,
            key: "example.com/team".into(),
            label: "Team".into(),
        };
        api.annotations
            .insert("example.com/team".into(), "platform".into());
        assert_eq!(
            resource_metadata_value(&api, annotation_column.source, &annotation_column.key),
            Some("platform")
        );
    }

    #[test]
    fn resource_count_includes_the_total_while_searching() {
        assert_eq!(resource_count_label(8, 1, true), "1 of 8 items");
        assert_eq!(resource_count_label(8, 8, false), "8 items");
    }

    #[test]
    fn command_f_focuses_resource_search_input() {
        let search = Rc::new(RefCell::new(ResourceSearchState::default()));
        let search_for_ui = search.clone();
        let mut harness = Harness::builder().build_ui(move |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                show_resource_search(ui, &mut search_for_ui.borrow_mut());
            });
        });
        components::test_support::setup_egui(&mut harness);
        harness.run();
        harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::F);
        harness.run();
        harness.event(egui::Event::Text("worker".into()));
        harness.run();

        assert_eq!(search.borrow().query, "worker");
    }
}
