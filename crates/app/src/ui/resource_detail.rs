use super::global_blade::{
    GlobalBladeContent, GlobalBladeEffect, GlobalBladeEffectContext, GlobalBladeNavigation,
    GlobalBladeRenderContext, GlobalBladeRenderResult,
};
use super::resource_actions::show_resource_action_items;
use super::resource_owner;
use super::state::{
    PendingDelete, PendingDeploymentRestart, PendingForceDelete, ResourceAction,
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

fn navigate_resource_detail_in_navigator(
    ui_state: &mut UiState,
    navigation: &mut GlobalBladeNavigation<'_>,
    cluster_key: i32,
    api_resource: crate::api_resource::ApiResource,
    name: String,
    namespace: Option<String>,
    uid: String,
) {
    let Some(cluster) = ui_state.clusters.get_mut(&cluster_key) else {
        return;
    };
    cluster.next_detail_generation += 1;
    let history_entry_id = cluster.next_detail_generation;
    let pod_metrics_api_available = cluster.pod_metrics_api_available;
    let node_metrics_api_available = cluster.node_metrics_api_available;
    if cluster.resource_detail_panel.is_none() {
        return;
    }
    navigation.push(Box::new(ResourceDetailHistoryEntry {
        history_entry_id,
        cluster_key,
        api_resource: api_resource.clone(),
        namespace: namespace.clone(),
        resource_name: name.clone(),
        resource_uid: uid.clone(),
        detail: None,
        events: Vec::new(),
        detail_error: None,
        events_error: None,
        managed_resources: Vec::new(),
        managed_resources_error: None,
        pod_usage: None,
        pod_usage_history: Vec::new(),
        pod_usage_missing: false,
        pod_metrics_api_unavailable: !pod_metrics_api_available,
        pod_usage_error: None,
        node_usage: None,
        node_usage_history: Vec::new(),
        node_metrics_api_unavailable: !node_metrics_api_available,
        node_usage_error: None,
        data_editor: None,
        pending_action: None,
    }));
    navigation
        .commands_to_send()
        .push(Box::new(crate::worker::StartResourceDetailWatch {
            cluster_key: cluster.cluster_key,
            history_entry_id,
            api_resource,
            namespace,
            resource_name: name,
            resource_uid: uid,
            pod_metrics_api_available,
            node_metrics_api_available,
        }));
}

fn show_resource_detail_header(
    ui: &mut egui::Ui,
    entry: &ResourceDetailHistoryEntry,
    is_foreground: bool,
    supports_scale: bool,
    debug_image_presets: &[DebugImagePreset],
) -> BladeResult {
    let mut result = BladeResult::default();
    let log_containers = entry
        .detail
        .as_ref()
        .and_then(|detail| match &detail.payload {
            ResourceDetailPayload::Pod(pod) => Some(pod.log_containers.clone()),
            _ => None,
        })
        .unwrap_or_default();
    let resource = MinimalResource {
        uid: entry.resource_uid.clone(),
        name: entry.resource_name.clone(),
        namespace: entry.namespace.clone(),
        creation_timestamp: None,
        controller_owner: None,
        labels: Default::default(),
        annotations: Default::default(),
        cells: Default::default(),
        log_containers,
    }
    .with_lifecycle_metadata(
        entry
            .detail
            .as_ref()
            .is_some_and(|detail| detail.is_deleting),
        entry
            .detail
            .as_ref()
            .map(|detail| detail.finalizers.clone())
            .unwrap_or_default(),
    );
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let more_label = if is_foreground {
            format!("More actions for {}", resource.name)
        } else {
            format!("More actions for {} in background blade", resource.name)
        };
        MoreButton::new(more_label).show(ui, |menu| {
            show_resource_action_items(
                menu,
                &entry.api_resource,
                &resource,
                &resource.log_containers,
                debug_image_presets,
                supports_scale,
                &mut result.action,
            );
        });
        ui.add_space(spacing::MD);
        let kind_width = ui
            .painter()
            .layout_no_wrap(
                entry.api_resource.kind.clone(),
                typography::body(),
                indigo::_600,
            )
            .size()
            .x
            + spacing::SM * 2.0;
        let available_name_width = (ui.available_width() - kind_width - spacing::MD).max(0.0);
        let natural_name_width = ui
            .painter()
            .layout_no_wrap(
                entry.resource_name.clone(),
                typography::page_title(),
                gray::_900,
            )
            .size()
            .x;
        let name = egui::RichText::new(&entry.resource_name)
            .font(typography::page_title())
            .color(gray::_900);
        if natural_name_width <= available_name_width {
            ui.label(name);
        } else {
            ui.add_sized(
                egui::vec2(available_name_width, 24.0),
                egui::Label::new(name).truncate().halign(egui::Align::RIGHT),
            );
        }
        ui.add_space(spacing::MD);
        egui::Frame::new()
            .fill(indigo::_50)
            .corner_radius(radius::control())
            .inner_margin(egui::Margin::symmetric(
                spacing::SM as i8,
                spacing::XS as i8,
            ))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(&entry.api_resource.kind)
                        .font(typography::body())
                        .color(indigo::_600),
                );
            });
    });
    result
}

#[allow(clippy::too_many_arguments)]
fn show_resource_detail_blade(
    ui: &mut egui::Ui,
    resource_navigation: &ResourceNavigation,
    api_resource: &crate::api_resource::ApiResource,
    _namespace: &Option<String>,
    _resource_name: &str,
    resource_uid: &str,
    detail: &Option<ResourceDetail>,
    events: &[ResourceEvent],
    detail_error: Option<&str>,
    events_error: Option<&str>,
    managed_resources: &[ManagedResource],
    managed_resources_error: Option<&str>,
    pod_usage: Option<&PodUsage>,
    pod_usage_history: &[PodUsage],
    pod_usage_missing: bool,
    pod_metrics_api_unavailable: bool,
    pod_usage_error: Option<&str>,
    node_usage: Option<&NodeUsage>,
    node_usage_history: &[NodeUsage],
    node_metrics_api_unavailable: bool,
    node_usage_error: Option<&str>,
    data_editor: Option<&mut super::state::ResourceDataEditorState>,
    table_preferences: Option<&mut super::table_preferences::PersistedResourceTablePreferences>,
    column_settings: Option<
        &mut Option<super::resource_table_settings::ResourceTableSettingsTarget>,
    >,
) -> BladeResult {
    let mut result = BladeResult::default();
    ui.set_max_width(ui.available_width() - 9.0);
    if let Some(error) = detail_error {
        error_card(ui, "Unable to load resource details", error);
    } else if let Some(detail) = detail {
        show_detail(
            ui,
            detail,
            PodUsageDisplay {
                usage: pod_usage,
                history: pod_usage_history,
                missing: pod_usage_missing,
                metrics_api_unavailable: pod_metrics_api_unavailable,
                error: pod_usage_error,
            },
            NodeUsageDisplay {
                usage: node_usage,
                history: node_usage_history,
                metrics_api_unavailable: node_metrics_api_unavailable,
                error: node_usage_error,
            },
            &mut result.action,
        );
        ui.add_space(16.0);
        show_resource_data(ui, detail, data_editor, &mut result.action);
        metadata_maps(ui, detail);
    } else {
        ui.spinner();
        ui.label(egui::RichText::new("Loading resource details…").color(gray::_500));
    }
    ui.add_space(20.0);
    show_managed_resources_for(
        ui,
        api_resource,
        resource_uid,
        managed_resources,
        managed_resources_error,
        &mut result.action,
        table_preferences,
        column_settings,
    );
    ui.add_space(16.0);
    show_events(ui, events, events_error);
    ui.add_space(16.0);
    if let Some(detail) = detail {
        show_additional_sections(ui, detail, resource_navigation, &mut result.action);
        ui.add_space(16.0);
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn show_managed_resources_for(
    ui: &mut egui::Ui,
    api_resource: &crate::api_resource::ApiResource,
    resource_uid: &str,
    managed_resources: &[ManagedResource],
    managed_resources_error: Option<&str>,
    pending_action: &mut Option<ResourceAction>,
    mut table_preferences: Option<&mut super::table_preferences::PersistedResourceTablePreferences>,
    mut column_settings: Option<
        &mut Option<super::resource_table_settings::ResourceTableSettingsTarget>,
    >,
) {
    let table_kinds = managed_resource_table_kinds(api_resource);
    if table_kinds.is_empty() {
        return;
    }

    for (index, (title, kind)) in table_kinds.iter().enumerate() {
        if index > 0 {
            ui.add_space(16.0);
        }
        let rows = managed_resource_rows(managed_resources, kind);
        section_header(ui, title, Some(format!("{} resources", rows.len())));
        show_managed_resource_table(
            ui,
            api_resource,
            resource_uid,
            kind,
            &rows,
            api_resource.kind == "Node",
            pending_action,
            table_preferences.as_deref_mut(),
            column_settings.as_deref_mut(),
        );
        if rows.is_empty() {
            ui.add_space(8.0);
            ui.label(egui::RichText::new(format!("No {title} found.")).color(gray::_500));
        }
    }

    if let Some(error) = managed_resources_error {
        ui.add_space(8.0);
        error_card(ui, "Unable to load all managed resources", error);
    }
}

#[allow(clippy::too_many_arguments)]
fn show_managed_resource_table(
    ui: &mut egui::Ui,
    detail_api_resource: &crate::api_resource::ApiResource,
    resource_uid: &str,
    kind: &str,
    rows: &[ManagedResourceRow],
    show_namespace_column: bool,
    pending_action: &mut Option<ResourceAction>,
    table_preferences: Option<&mut super::table_preferences::PersistedResourceTablePreferences>,
    column_settings: Option<
        &mut Option<super::resource_table_settings::ResourceTableSettingsTarget>,
    >,
) {
    let definition = managed_resource_table_definition(kind, show_namespace_column);
    if let (Some(table_preferences), Some(column_settings)) = (table_preferences, column_settings) {
        let mut column_definitions = vec![TableColumnDefinition {
            id: "name".into(),
            label: "Name".into(),
            default_width: 160.0,
            sortable: true,
        }];
        if show_namespace_column {
            column_definitions.push(TableColumnDefinition {
                id: "namespace".into(),
                label: "Namespace".into(),
                default_width: 150.0,
                sortable: true,
            });
        }
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
        column_definitions.push(TableColumnDefinition {
            id: "age".into(),
            label: "Age".into(),
            default_width: 77.0,
            sortable: true,
        });
        let fixed_width = column_definitions
            .iter()
            .skip(1)
            .map(|column| column.default_width)
            .sum::<f32>();
        column_definitions[0].default_width =
            (ui.available_width() - fixed_width - 16.0).max(160.0);
        let table_resource = managed_resource_api_resource(kind);
        let table_key = ResourceTableKey::detail(detail_api_resource, &table_resource);
        let visible_columns = table_preferences.resolved_columns(&table_key, &column_definitions);
        let sort_state = table_preferences
            .sort(&table_key, &column_definitions)
            .map(|(column_id, direction)| components::SortState::new(column_id, direction));
        let mut sorted_rows = rows.to_vec();
        if let Some(sort) = &sort_state {
            sorted_rows.sort_by(|left, right| {
                compare_managed_resource_column(left, right, &sort.column_id, sort.direction)
            });
        }
        let mut table =
            TailwindTable::new(format!("managed-resource-table-{resource_uid}-{kind}")).roomy();
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
        let table_preferences = RefCell::new(table_preferences);
        let pending_action = RefCell::new(pending_action);
        table.show_configurable_with_row_response(
            ui,
            &sorted_rows,
            sort_state.as_ref(),
            |header, id, _label, sortable| {
                MoreButton::show_context_menu(header, |menu| {
                    super::resource_table_settings::show_configurable_table_header(
                        menu,
                        sortable,
                        id,
                        &table_key,
                        &column_definitions,
                        &table_preferences,
                        column_settings,
                    );
                });
            },
            |id, width| {
                table_preferences
                    .borrow_mut()
                    .set_width(&table_key, &column_definitions, id, width)
            },
            |ui, row, column_index| match visible_columns[column_index].definition.id.as_str() {
                "name" => {
                    if TableRowBuilder::clickable_text(
                        ui,
                        &row.name,
                        gray::_900,
                        format!("Open details for {}", row.name),
                    )
                    .clicked()
                        && pending_action.borrow().is_none()
                    {
                        **pending_action.borrow_mut() = Some(ResourceAction::NavigateDetails {
                            api_resource: row.api_resource.clone(),
                            name: row.name.clone(),
                            namespace: row.namespace.clone(),
                            uid: row.uid.clone(),
                        });
                    }
                }
                "namespace" => {
                    TableRowBuilder::text(ui, row.namespace.as_deref().unwrap_or("-"), false)
                }
                "age" => TableRowBuilder::text(ui, &format_age(row.creation_timestamp), false),
                id => show_resource_cell(ui, row.cells.get(id)),
            },
            |response, row, column_index| {
                if response.clicked() && pending_action.borrow().is_none() {
                    **pending_action.borrow_mut() = Some(ResourceAction::NavigateDetails {
                        api_resource: row.api_resource.clone(),
                        name: row.name.clone(),
                        namespace: row.namespace.clone(),
                        uid: row.uid.clone(),
                    });
                }
                let _ = column_index;
            },
        );
        return;
    }
    let mut table = TailwindTable::new(format!("managed-resource-table-{resource_uid}-{kind}",))
        .roomy()
        .column("name", "Name", |column| column.fill_remaining());
    if show_namespace_column {
        table = table.column("namespace", "Namespace", |column| {
            column.initial_width(150.0)
        });
    }
    for column in &definition.columns {
        table = table.column(column.id.clone(), column.label.clone(), |table_column| {
            table_column.initial_width(column.initial_width)
        });
    }
    table = table.column("age", "Age", |column| column.initial_width(77.0));
    table.show_with_row_response(
        ui,
        rows,
        |ui, row, column_index| {
            let type_specific_start = 1 + usize::from(show_namespace_column);
            match column_index {
                0 => {
                    if TableRowBuilder::clickable_text(
                        ui,
                        &row.name,
                        gray::_900,
                        format!("Open details for {}", row.name),
                    )
                    .clicked()
                        && pending_action.is_none()
                    {
                        *pending_action = Some(ResourceAction::NavigateDetails {
                            api_resource: row.api_resource.clone(),
                            name: row.name.clone(),
                            namespace: row.namespace.clone(),
                            uid: row.uid.clone(),
                        });
                    }
                }
                1 if show_namespace_column => {
                    TableRowBuilder::text(ui, row.namespace.as_deref().unwrap_or("-"), false)
                }
                index
                    if index >= type_specific_start
                        && index < type_specific_start + definition.columns.len() =>
                {
                    let column = &definition.columns[index - type_specific_start];
                    show_resource_cell(ui, row.cells.get(&column.id));
                }
                _ => TableRowBuilder::text(ui, &format_age(row.creation_timestamp), false),
            }
        },
        |_, _, _| {},
    );
}

#[derive(Clone)]
struct ManagedResourceRow {
    api_resource: crate::api_resource::ApiResource,
    name: String,
    namespace: Option<String>,
    uid: String,
    creation_timestamp: Option<time::OffsetDateTime>,
    cells: BTreeMap<String, crate::resource_table::CellValue>,
}

fn managed_resource_rows(resources: &[ManagedResource], kind: &str) -> Vec<ManagedResourceRow> {
    let mut rows = resources
        .iter()
        .filter(|resource| resource.api_resource.kind == kind)
        .map(ManagedResourceRow::from)
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| left.name.cmp(&right.name));
    rows
}

fn compare_managed_resource_column(
    left: &ManagedResourceRow,
    right: &ManagedResourceRow,
    column_id: &str,
    direction: components::SortDirection,
) -> std::cmp::Ordering {
    let value = |row: &ManagedResourceRow| match column_id {
        "name" => SortValue::Text(row.name.clone()),
        "namespace" => SortValue::Text(row.namespace.clone().unwrap_or_default()),
        "age" => row
            .creation_timestamp
            .map(|time| SortValue::Number(time.unix_timestamp()))
            .unwrap_or(SortValue::Empty),
        id => row
            .cells
            .get(id)
            .map(cell_sort_value)
            .unwrap_or(SortValue::Empty),
    };
    let left_value = value(left);
    let right_value = value(right);
    let ordering = compare_sort_values(left_value, right_value, direction);
    ordering
        .then_with(|| left.name.cmp(&right.name))
        .then_with(|| left.uid.cmp(&right.uid))
}

impl From<&ManagedResource> for ManagedResourceRow {
    fn from(resource: &ManagedResource) -> Self {
        Self {
            api_resource: resource.api_resource.clone(),
            name: resource.name.clone(),
            namespace: resource.namespace.clone(),
            uid: resource.uid.clone(),
            creation_timestamp: resource.creation_timestamp,
            cells: resource.cells.clone(),
        }
    }
}

fn managed_resource_table_definition(
    kind: &str,
    omit_contextual_node_column: bool,
) -> ResourceTableDefinition {
    let mut definition = table_definition(&managed_resource_api_resource(kind), &[]);
    // Live Metrics API values belong to the Pods workspace. Managed-resource tables are compact
    // relationship views and do not receive the namespace metric cache.
    definition
        .columns
        .retain(|column| column.id != CPU_COLUMN && column.id != MEMORY_COLUMN);
    if kind == "Pod" {
        for column in &mut definition.columns {
            column.initial_width = match column.id.as_str() {
                READY_COLUMN => 90.0,
                CONTAINERS_COLUMN => 150.0,
                STATUS_COLUMN => 128.0,
                RESTARTS_COLUMN => 120.0,
                NODE_COLUMN => 180.0,
                _ => column.initial_width,
            };
        }
    }
    if kind == "Pod" {
        // The inspector panel is substantially narrower than the workspace.
        // Container indicators are useful in the primary list, but in this
        // context they crowd out the Pod name while status and restart counts
        // remain directly actionable.
        definition
            .columns
            .retain(|column| column.id != CONTAINERS_COLUMN);
        if omit_contextual_node_column {
            // All listed Pods are scheduled to the inspected Node, so repeating
            // its name consumes scarce inspector width without adding context.
            definition.columns.retain(|column| column.id != NODE_COLUMN);
        }
    }
    definition
}

fn managed_resource_api_resource(kind: &str) -> crate::api_resource::ApiResource {
    let (group, name) = match kind {
        "ReplicaSet" => ("apps", "replicasets"),
        "Job" => ("batch", "jobs"),
        "Pod" => ("core", "pods"),
        _ => unreachable!("managed resource table kind must be supported"),
    };
    crate::api_resource::ApiResource {
        group: group.to_owned(),
        version: "v1".to_owned(),
        kind: kind.to_owned(),
        name: name.to_owned(),
        namespaced: true,
    }
}

fn managed_resource_table_kinds(
    api_resource: &crate::api_resource::ApiResource,
) -> &'static [(&'static str, &'static str)] {
    match (api_resource.group.as_str(), api_resource.kind.as_str()) {
        ("apps", "Deployment") => &[("ReplicaSets", "ReplicaSet"), ("Pods", "Pod")],
        ("batch", "CronJob") => &[("Jobs", "Job"), ("Pods", "Pod")],
        ("apps", "ReplicaSet")
        | ("apps", "StatefulSet")
        | ("apps", "DaemonSet")
        | ("core", "ReplicationController")
        | ("batch", "Job") => &[("Pods", "Pod")],
        ("core", "Node") => &[("Pods", "Pod")],
        _ => &[],
    }
}

fn show_detail(
    ui: &mut egui::Ui,
    detail: &ResourceDetail,
    usage: PodUsageDisplay<'_>,
    node_usage: NodeUsageDisplay<'_>,
    pending_action: &mut Option<ResourceAction>,
) {
    show_generic_summary(ui, detail);
    ui.add_space(13.0);
    if let ResourceDetailPayload::Pod(pod) = &detail.payload {
        show_pod_summary(ui, detail, pod, pending_action);
        ui.add_space(13.0);
        show_pod_detail(ui, pod, usage);
    } else if let ResourceDetailPayload::Node(node) = &detail.payload {
        show_node_detail(ui, node, node_usage);
    } else if let ResourceDetailPayload::Diagnostic(diagnostic) = &detail.payload {
        show_diagnostic_detail(ui, diagnostic);
    }
}

fn show_node_detail(ui: &mut egui::Ui, node: &NodeDetail, usage: NodeUsageDisplay<'_>) {
    let pod_cidrs = if node.pod_cidrs.is_empty() {
        "-".to_owned()
    } else {
        node.pod_cidrs.join(", ")
    };
    let taints = if node.taints.is_empty() {
        "None".to_owned()
    } else {
        node.taints.join(", ")
    };
    InspectorDetails::show_titled_properties(
        ui,
        "Spec",
        &[DetailRow::new([
            DetailCell::status(
                "Scheduling",
                if node.unschedulable {
                    "Scheduling disabled"
                } else {
                    "Schedulable"
                },
                if node.unschedulable {
                    DetailTone::Warning
                } else {
                    DetailTone::Success
                },
            ),
            DetailCell::new("Provider ID", node.provider_id.as_deref().unwrap_or("-")).copyable(),
            DetailCell::new("Pod CIDRs", pod_cidrs.as_str()).copyable(),
            DetailCell::new("Taints", taints.as_str()).copyable(),
        ])],
    );
    ui.add_space(13.0);
    show_node_usage(ui, usage, node.allocatable);
}

fn show_generic_summary(ui: &mut egui::Ui, detail: &ResourceDetail) {
    detail_summary_card(ui, |ui| {
        InspectorDetails::show_properties(
            ui,
            &[
                DetailRow::new([
                    DetailCell::new("Kind", detail.api_resource.kind.as_str()).copyable(),
                    DetailCell::new("Name", detail.name.as_str()).copyable(),
                    detail.namespace.as_deref().map_or_else(
                        || DetailCell::new("Namespace", "Cluster-wide"),
                        |namespace| DetailCell::new("Namespace", namespace).copyable(),
                    ),
                ]),
                DetailRow::new([
                    DetailCell::new("UID", detail.uid.as_str()).copyable(),
                    DetailCell::new("Resource version", detail.resource_version.as_str())
                        .copyable(),
                    DetailCell::new("Age", format_age(detail.creation_timestamp)),
                ]),
            ],
        );
    });
}

fn show_diagnostic_detail(ui: &mut egui::Ui, diagnostic: &DiagnosticDetail) {
    for (index, section) in diagnostic.sections.iter().enumerate() {
        InspectorDetails::show_titled_properties(
            ui,
            section.title.as_str(),
            &[DetailRow::new(section.fields.iter().map(|field| {
                DetailCell::new(field.label.as_str(), field.value.as_str()).copyable()
            }))],
        );
        if index + 1 < diagnostic.sections.len() {
            ui.add_space(CARD_GAP);
        }
    }
}

fn show_resource_data(
    ui: &mut egui::Ui,
    detail: &ResourceDetail,
    editor: Option<&mut super::state::ResourceDataEditorState>,
    pending_action: &mut Option<ResourceAction>,
) {
    let Some(editor) = editor else {
        return;
    };
    match &detail.payload {
        ResourceDetailPayload::ConfigMap(config_map) => {
            show_config_map_data(ui, config_map, editor, pending_action)
        }
        ResourceDetailPayload::Secret(secret) => {
            show_secret_data(ui, secret, editor, pending_action)
        }
        ResourceDetailPayload::Generic
        | ResourceDetailPayload::Diagnostic(_)
        | ResourceDetailPayload::Pod(_)
        | ResourceDetailPayload::Node(_) => {}
    }
}

fn show_config_map_data(
    ui: &mut egui::Ui,
    config_map: &ConfigMapDetail,
    editor: &mut super::state::ResourceDataEditorState,
    pending_action: &mut Option<ResourceAction>,
) {
    section_header(
        ui,
        "Data",
        Some(format!(
            "{} entries · {}",
            config_map.data.len(),
            if config_map.immutable {
                "Immutable"
            } else {
                "Mutable"
            }
        )),
    );
    if config_map.data.is_empty() {
        detail_message_card(ui, |ui| {
            ui.label(egui::RichText::new("No text data entries.").color(gray::_500));
        });
    }
    for key in config_map.data.keys() {
        let value = editor
            .draft_values
            .get(key)
            .expect("typed data detail and editor keys remain in sync")
            .clone();
        data_entry(
            ui,
            key,
            None,
            |ui| {
                if TailwindButton::secondary(format!("Copy {key}"))
                    .size(ButtonSize::Sm)
                    .show(ui)
                    .clicked()
                {
                    ui.ctx().copy_text(value);
                }
            },
            |ui| data_value_editor(ui, key, editor, config_map.immutable),
        );
    }
    data_save_controls(ui, editor, config_map.immutable, pending_action);
    ui.add_space(16.0);
}

fn show_secret_data(
    ui: &mut egui::Ui,
    secret: &SecretDetail,
    editor: &mut super::state::ResourceDataEditorState,
    pending_action: &mut Option<ResourceAction>,
) {
    section_header(
        ui,
        "Data",
        Some(format!(
            "{} entries · {} · {}",
            secret.data.len(),
            secret.type_,
            if secret.immutable {
                "Immutable"
            } else {
                "Mutable"
            }
        )),
    );
    if secret.data.is_empty() {
        detail_message_card(ui, |ui| {
            ui.label(egui::RichText::new("No data entries.").color(gray::_500));
        });
    }
    for (key, value) in &secret.data {
        let revealed = editor.revealed_secret_keys.contains(key);
        let mut visibility_toggled = false;
        let copy_value = (revealed && value.text.is_some())
            .then(|| editor.draft_values.get(key).cloned())
            .flatten();
        data_entry(
            ui,
            key,
            Some(value.byte_len),
            |ui| {
                if let Some(copy_value) = copy_value.as_ref()
                    && TailwindButton::secondary(format!("Copy {key}"))
                        .size(ButtonSize::Sm)
                        .show(ui)
                        .clicked()
                {
                    ui.ctx().copy_text(copy_value.clone());
                }
                if value.text.is_some()
                    && TailwindButton::secondary(if revealed { "Hide" } else { "Reveal" })
                        .size(ButtonSize::Sm)
                        .show(ui)
                        .clicked()
                {
                    visibility_toggled = true;
                }
            },
            |ui| match value.text.as_ref() {
                Some(_) if revealed => data_value_editor(ui, key, editor, secret.immutable),
                Some(_) => secret_value_mask(ui),
                None => unavailable_secret_value(ui),
            },
        );
        if visibility_toggled {
            if revealed {
                editor.revealed_secret_keys.remove(key);
            } else {
                editor.revealed_secret_keys.insert(key.clone());
            }
        }
    }
    data_save_controls(ui, editor, secret.immutable, pending_action);
    ui.add_space(16.0);
}

fn detail_summary_card(ui: &mut egui::Ui, add_content: impl FnOnce(&mut egui::Ui)) {
    WorkspaceCard::new().padding(18).show(ui, add_content);
}

fn detail_item_card(
    ui: &mut egui::Ui,
    add_header: impl FnOnce(&mut egui::Ui),
    add_content: impl FnOnce(&mut egui::Ui),
) {
    WorkspaceCard::new()
        .padding(CARD_CONTENT_PADDING)
        .show(ui, |ui| {
            add_header(ui);
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);
            add_content(ui);
        });
}

fn detail_message_card(ui: &mut egui::Ui, add_content: impl FnOnce(&mut egui::Ui)) {
    WorkspaceCard::new()
        .padding(CARD_CONTENT_PADDING)
        .show(ui, add_content);
}

fn data_entry(
    ui: &mut egui::Ui,
    key: &str,
    byte_len: Option<usize>,
    add_action: impl FnOnce(&mut egui::Ui),
    add_value: impl FnOnce(&mut egui::Ui),
) {
    detail_item_card(
        ui,
        |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(key)
                        .monospace()
                        .strong()
                        .color(gray::_800),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    add_action(ui);
                    if let Some(byte_len) = byte_len {
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(format!("{byte_len} bytes"))
                                .font(typography::metadata())
                                .color(gray::_500),
                        );
                    }
                });
            });
        },
        add_value,
    );
    ui.add_space(8.0);
}

fn data_value_editor(
    ui: &mut egui::Ui,
    key: &str,
    editor: &mut super::state::ResourceDataEditorState,
    immutable: bool,
) {
    let value = editor
        .draft_values
        .get_mut(key)
        .expect("typed data detail and editor keys remain in sync");
    let response = TailwindTextArea::new(value)
        .id_salt(("resource-data-value", key))
        .monospace()
        .desired_rows(3)
        .enabled(!immutable && !editor.saving)
        .show(ui);
    if response.hovered() && immutable {
        response.on_hover_text("This resource's data is immutable.");
    }
}

fn secret_value_mask(ui: &mut egui::Ui) {
    egui::Frame::new()
        .fill(gray::_50)
        .stroke(egui::Stroke::new(1.0, gray::_200))
        .corner_radius(radius::control())
        .inner_margin(egui::Margin::symmetric(
            (spacing::SM + spacing::XS) as i8,
            spacing::SM as i8,
        ))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("••••••••••••")
                    .monospace()
                    .color(gray::_700),
            );
        });
}

fn unavailable_secret_value(ui: &mut egui::Ui) {
    egui::Frame::new()
        .fill(gray::_50)
        .stroke(egui::Stroke::new(1.0, gray::_200))
        .corner_radius(radius::control())
        .inner_margin(egui::Margin::symmetric(
            (spacing::SM + spacing::XS) as i8,
            spacing::SM as i8,
        ))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("Binary data")
                    .strong()
                    .color(gray::_700),
            );
            ui.label(
                egui::RichText::new("This value cannot be edited in the inspector.")
                    .font(typography::metadata())
                    .color(gray::_600),
            );
        });
}

fn data_save_controls(
    ui: &mut egui::Ui,
    editor: &mut super::state::ResourceDataEditorState,
    immutable: bool,
    pending_action: &mut Option<ResourceAction>,
) {
    if let Some(error) = &editor.save_error {
        ui.colored_label(status::DANGER, error);
        ui.add_space(spacing::SM);
    }
    if immutable {
        ui.label(egui::RichText::new("Data is immutable and cannot be edited.").color(gray::_500));
        return;
    }
    let (expected_values, updated_values) = editor.changed_values();
    let save_clicked = ui
        .horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_enabled_ui(!editor.saving && !updated_values.is_empty(), |ui| {
                    TailwindButton::primary(if editor.saving {
                        "Saving…"
                    } else {
                        "Save data"
                    })
                    .size(ButtonSize::Sm)
                    .show(ui)
                })
                .inner
            })
            .inner
        })
        .inner
        .clicked();
    if save_clicked && pending_action.is_none() {
        editor.saving = true;
        editor.save_error = None;
        *pending_action = Some(ResourceAction::SaveData {
            expected_values,
            updated_values,
        });
    }
}

fn show_data_conflict_dialog(
    ctx: &egui::Context,
    editor: Option<&mut super::state::ResourceDataEditorState>,
) {
    let Some(editor) = editor else {
        return;
    };
    if editor.pending_external_values.is_none() {
        return;
    }
    let mut use_external = false;
    let mut keep_local = false;
    egui::Window::new("Data changed on cluster")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_min_width(360.0);
            ui.label("This resource changed on the cluster while you have unsaved data edits.");
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui
                    .button("Use cluster version")
                    .with_pointing_hand()
                    .clicked()
                {
                    use_external = true;
                }
                if ui.button("Keep my edits").with_pointing_hand().clicked() {
                    keep_local = true;
                }
            });
        });
    if use_external {
        editor.use_external_values();
    } else if keep_local {
        editor.keep_local_edits();
    }
}

fn show_pod_summary(
    ui: &mut egui::Ui,
    detail: &ResourceDetail,
    pod: &PodDetail,
    pending_action: &mut Option<ResourceAction>,
) {
    let ready = format!(
        "{}/{}",
        pod.containers
            .iter()
            .filter(|container| container.ready)
            .count(),
        pod.containers.len()
    );
    detail_summary_card(ui, |ui| {
        let node_action = ui.id().with("pod-summary-node");
        let node_cell = pod.node_name.as_deref().map_or_else(
            || DetailCell::unavailable("Node"),
            |node_name| DetailCell::link("Node", node_name, node_action),
        );
        let response = InspectorDetails::show_properties(
            ui,
            &[
                DetailRow::new([
                    DetailCell::new(
                        "Namespace",
                        detail.namespace.as_deref().unwrap_or("Cluster-wide"),
                    ),
                    DetailCell::status("Status", pod.phase.as_str(), pod_phase_tone(&pod.phase)),
                    node_cell,
                ]),
                DetailRow::new([
                    DetailCell::new("Pod IP", pod.pod_ip.as_deref().unwrap_or("-")).copyable(),
                    DetailCell::new("Host IP", pod.host_ip.as_deref().unwrap_or("-")).copyable(),
                    DetailCell::new("QoS class", pod.qos_class.as_deref().unwrap_or("-")),
                ]),
                DetailRow::new([
                    DetailCell::new("Ready", ready),
                    DetailCell::new("Age", format_age(detail.creation_timestamp)),
                ]),
            ],
        );
        if response.activated.contains(&node_action)
            && let Some(node_name) = pod.node_name.as_deref()
            && pending_action.is_none()
        {
            *pending_action = Some(ResourceAction::NavigateDetails {
                api_resource: crate::resource_handlers::node::api_resource(),
                name: node_name.to_owned(),
                namespace: None,
                uid: node_name.to_owned(),
            });
        }
    });
}

fn show_pod_detail(ui: &mut egui::Ui, pod: &PodDetail, usage: PodUsageDisplay<'_>) {
    let pod_requests =
        total_resource_thresholds(&pod.containers, |container| container.resource_requests);
    let pod_limits =
        total_resource_thresholds(&pod.containers, |container| container.resource_limits);
    show_pod_usage(ui, usage, pod_requests, pod_limits);
    ui.add_space(CARD_GAP);
    section_header(ui, "Containers", None);
    for container in &pod.containers {
        detail_item_card(
            ui,
            |ui| {
                ui.label(
                    egui::RichText::new(format!("⌃   {}", container.name))
                        .strong()
                        .color(gray::_800),
                );
            },
            |ui| {
                InspectorDetails::show_properties(
                    ui,
                    &[DetailRow::new([
                        DetailCell::new("Image", container.image.as_str()).copyable(),
                        DetailCell::new("State", container.state.as_str()),
                    ])],
                );
                show_container_usage(
                    ui,
                    &container.name,
                    usage
                        .usage
                        .and_then(|usage| usage.containers.get(&container.name)),
                    usage.history,
                    container.resource_requests,
                    container.resource_limits,
                    usage.missing,
                    usage.metrics_api_unavailable,
                    usage.error,
                );
                ui.add_space(6.0);
                InspectorDetails::show_properties(
                    ui,
                    &[DetailRow::new([
                        DetailCell::new("Ready", if container.ready { "Yes" } else { "No" }),
                        DetailCell::new("Restarts", container.restart_count.to_string()),
                    ])],
                );
                if !container.command.is_empty() {
                    chip_row(ui, "Command", &container.command);
                }
                if !container.args.is_empty() {
                    chip_row(ui, "Args", &container.args);
                }
                if !container.ports.is_empty() {
                    chip_row(ui, "Ports", &container.ports);
                }
                if !container.environment_variables.is_empty() {
                    ui.add_space(10.0);
                    environment_variables(ui, &container.environment_variables);
                }
                if let Some(reason) = &container.reason {
                    InspectorDetails::show_properties(
                        ui,
                        &[DetailRow::new([
                            DetailCell::new("Reason", reason.as_str()).copyable()
                        ])],
                    );
                }
                if let Some(message) = &container.message {
                    InspectorDetails::show_properties(
                        ui,
                        &[DetailRow::new([DetailCell::new(
                            "Message",
                            message.as_str(),
                        )
                        .copyable()])],
                    );
                }
            },
        );
        ui.add_space(CARD_GAP);
    }
    if !pod.volumes.is_empty() {
        section_header(ui, "Volumes", None);
        for (index, volume) in pod.volumes.iter().enumerate() {
            detail_item_card(
                ui,
                |ui| {
                    ui.label(
                        egui::RichText::new(format!("⌄   {}", volume.name))
                            .strong()
                            .color(gray::_800),
                    );
                },
                |ui| volume_detail_row(ui, volume),
            );
            if index + 1 < pod.volumes.len() {
                ui.add_space(CARD_GAP);
            }
        }
    }
}

fn show_pod_usage(
    ui: &mut egui::Ui,
    usage: PodUsageDisplay<'_>,
    requests: PodResourceThresholds,
    limits: PodResourceThresholds,
) {
    section_header(ui, "Resource usage", None);
    WorkspaceCard::new().show(ui, |ui| {
        if usage.metrics_api_unavailable {
            show_metrics_api_unavailable(ui, requests, limits);
            return;
        }
        let cpu_references = usage_references(
            requests.cpu_nanocores,
            limits.cpu_nanocores,
            ["Request", "Limit"],
        );
        let memory_references = usage_references(
            requests.memory_bytes,
            limits.memory_bytes,
            ["Request", "Limit"],
        );
        let displayed_usage = displayed_usage_values(
            usage
                .usage
                .map(|usage| (usage.cpu_nanocores, usage.memory_bytes)),
            usage.error,
        );
        show_usage_value_grid(ui, displayed_usage);
        if !usage.history.is_empty()
            || has_usage_references(&cpu_references)
            || has_usage_references(&memory_references)
        {
            if usage.usage.is_none() {
                usage_chart_pair_labels(ui);
            }
            ui.add_space(8.0);
            ui.columns(2, |columns| {
                usage_chart(
                    &mut columns[0],
                    "Pod CPU usage chart",
                    usage
                        .history
                        .iter()
                        .map(|sample| (sample.timestamp, sample.cpu_nanocores))
                        .collect(),
                    format_cpu,
                    if usage.error.is_some() {
                        gray::_400
                    } else {
                        indigo::_600
                    },
                    usage.error.is_some(),
                    &cpu_references,
                );
                usage_chart(
                    &mut columns[1],
                    "Pod memory usage chart",
                    usage
                        .history
                        .iter()
                        .map(|sample| (sample.timestamp, sample.memory_bytes))
                        .collect(),
                    format_memory,
                    if usage.error.is_some() {
                        gray::_400
                    } else {
                        status::SUCCESS
                    },
                    usage.error.is_some(),
                    &memory_references,
                );
            });
        }
    });
}

fn show_node_usage(
    ui: &mut egui::Ui,
    usage: NodeUsageDisplay<'_>,
    allocatable: PodResourceThresholds,
) {
    section_header(ui, "Resource usage", None);
    WorkspaceCard::new().show(ui, |ui| {
        if usage.metrics_api_unavailable {
            show_node_metrics_api_unavailable(ui, allocatable);
            return;
        }
        let cpu_references = usage_references(allocatable.cpu_nanocores, None, ["Allocatable", ""]);
        let memory_references =
            usage_references(allocatable.memory_bytes, None, ["Allocatable", ""]);
        show_usage_value_grid(
            ui,
            displayed_usage_values(
                usage
                    .usage
                    .map(|usage| (usage.cpu_nanocores, usage.memory_bytes)),
                usage.error,
            ),
        );
        if !usage.history.is_empty()
            || has_usage_references(&cpu_references)
            || has_usage_references(&memory_references)
        {
            if usage.usage.is_none() {
                usage_chart_pair_labels(ui);
            }
            ui.add_space(8.0);
            ui.columns(2, |columns| {
                usage_chart(
                    &mut columns[0],
                    "Node CPU usage chart",
                    usage
                        .history
                        .iter()
                        .map(|sample| (sample.timestamp, sample.cpu_nanocores))
                        .collect(),
                    format_cpu,
                    if usage.error.is_some() {
                        gray::_400
                    } else {
                        indigo::_600
                    },
                    usage.error.is_some(),
                    &cpu_references,
                );
                usage_chart(
                    &mut columns[1],
                    "Node memory usage chart",
                    usage
                        .history
                        .iter()
                        .map(|sample| (sample.timestamp, sample.memory_bytes))
                        .collect(),
                    format_memory,
                    if usage.error.is_some() {
                        gray::_400
                    } else {
                        status::SUCCESS
                    },
                    usage.error.is_some(),
                    &memory_references,
                );
            });
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn show_container_usage(
    ui: &mut egui::Ui,
    name: &str,
    usage: Option<&ContainerUsage>,
    history: &[PodUsage],
    requests: PodResourceThresholds,
    limits: PodResourceThresholds,
    _missing: bool,
    metrics_api_unavailable: bool,
    error: Option<&str>,
) {
    ui.add_space(10.0);
    if metrics_api_unavailable {
        show_metrics_api_unavailable(ui, requests, limits);
        return;
    }
    let cpu_references = usage_references(
        requests.cpu_nanocores,
        limits.cpu_nanocores,
        ["Request", "Limit"],
    );
    let memory_references = usage_references(
        requests.memory_bytes,
        limits.memory_bytes,
        ["Request", "Limit"],
    );
    let displayed_usage = displayed_usage_values(
        usage.map(|usage| (usage.cpu_nanocores, usage.memory_bytes)),
        error,
    );
    show_usage_value_grid(ui, displayed_usage);
    if history
        .iter()
        .any(|sample| sample.containers.contains_key(name))
        || has_usage_references(&cpu_references)
        || has_usage_references(&memory_references)
    {
        if usage.is_none() {
            usage_chart_pair_labels(ui);
        }
        ui.add_space(6.0);
        ui.columns(2, |columns| {
            usage_chart(
                &mut columns[0],
                &format!("{name} CPU usage chart"),
                history
                    .iter()
                    .filter_map(|sample| {
                        sample
                            .containers
                            .get(name)
                            .map(|usage| (sample.timestamp, usage.cpu_nanocores))
                    })
                    .collect(),
                format_cpu,
                if error.is_some() {
                    gray::_400
                } else {
                    indigo::_600
                },
                error.is_some(),
                &cpu_references,
            );
            usage_chart(
                &mut columns[1],
                &format!("{name} memory usage chart"),
                history
                    .iter()
                    .filter_map(|sample| {
                        sample
                            .containers
                            .get(name)
                            .map(|usage| (sample.timestamp, usage.memory_bytes))
                    })
                    .collect(),
                format_memory,
                if error.is_some() {
                    gray::_400
                } else {
                    status::SUCCESS
                },
                error.is_some(),
                &memory_references,
            );
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn usage_chart(
    ui: &mut egui::Ui,
    accessibility_label: &str,
    samples: Vec<(time::OffsetDateTime, i64)>,
    format: impl Fn(i64) -> String,
    color: egui::Color32,
    metrics_unavailable: bool,
    references: &UsageReferences,
) {
    let max_value = samples
        .iter()
        .map(|(_, value)| *value)
        .chain(references.iter().flatten().map(|reference| reference.value))
        .max()
        .unwrap_or(1)
        .max(1);
    let max = max_value as f32;
    let reference_summary = references
        .iter()
        .flatten()
        .map(|reference| format!("{} {}", reference.label, format(reference.value)))
        .collect::<Vec<_>>()
        .join(", ");
    let chart_summary = format!(
        "{accessibility_label}; {}; {} history; scale from 0 to {}; {}",
        if metrics_unavailable {
            "metrics unavailable; displayed history may be stale"
        } else if samples.len() < 2 {
            "collecting samples"
        } else {
            "usage history available"
        },
        format_history_window(),
        format(max_value),
        if reference_summary.is_empty() {
            "no usage reference configured"
        } else {
            &reference_summary
        }
    );
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), USAGE_CHART_HEIGHT),
        egui::Sense::hover(),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Image, true, chart_summary.clone())
    });
    let status_message = if metrics_unavailable {
        "Unavailable"
    } else {
        "Collecting…"
    };
    if samples.len() < 2 && !has_usage_references(references) {
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            status_message,
            typography::metadata(),
            gray::_500,
        );
        return;
    }
    let start = time::OffsetDateTime::now_utc() - POD_USAGE_HISTORY_WINDOW;
    let plot = egui::Rect::from_min_max(
        egui::pos2(
            rect.left() + USAGE_CHART_LEFT_INSET,
            rect.top() + USAGE_CHART_TOP_INSET,
        ),
        egui::pos2(
            rect.right() - USAGE_CHART_RIGHT_INSET,
            rect.bottom() - USAGE_CHART_BOTTOM_INSET,
        ),
    );
    draw_chart_axes(ui.painter(), plot, &format, max);
    for reference in references.iter().flatten() {
        let y = plot.bottom() - plot.height() * (reference.value as f32 / max);
        dashed_reference_line(
            ui.painter(),
            plot.left(),
            plot.right(),
            y,
            egui::Stroke::new(
                1.0,
                reference
                    .color
                    .gamma_multiply(USAGE_CHART_REFERENCE_OPACITY),
            ),
        );
    }
    let points = samples
        .iter()
        .map(|(timestamp, sample)| {
            let fraction = ((*timestamp - start).whole_seconds() as f32
                / POD_USAGE_HISTORY_WINDOW.whole_seconds() as f32)
                .clamp(0.0, 1.0);
            egui::pos2(
                egui::lerp(plot.left()..=plot.right(), fraction),
                plot.bottom() - plot.height() * (*sample as f32 / max),
            )
        })
        .collect::<Vec<_>>();
    if points.len() >= 2 {
        ui.painter().add(egui::Shape::mesh(usage_area_mesh(
            &points,
            plot.bottom(),
            color.gamma_multiply(USAGE_CHART_AREA_OPACITY),
        )));
        ui.painter().add(egui::Shape::line(
            points.clone(),
            egui::Stroke::new(1.7, color),
        ));
    } else {
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            status_message,
            typography::metadata(),
            gray::_500,
        );
    }
    if let Some(pointer) = response
        .hover_pos()
        .filter(|pointer| plot.contains(*pointer))
        && let Some((timestamp, sample)) = points
            .iter()
            .zip(&samples)
            .min_by(|(left, _), (right, _)| {
                (pointer.x - left.x)
                    .abs()
                    .total_cmp(&(pointer.x - right.x).abs())
            })
            .map(|(_, sample)| *sample)
    {
        let mut tooltip = format!(
            "{}\n{}",
            format(sample),
            timestamp
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default()
        );
        for reference in references.iter().flatten() {
            tooltip.push_str(&format!(
                "\n{}: {}",
                reference.label,
                format(reference.value)
            ));
        }
        response.on_hover_text(tooltip);
    }
}

fn usage_area_mesh(points: &[egui::Pos2], baseline: f32, color: egui::Color32) -> egui::Mesh {
    let mut mesh = egui::Mesh::default();
    let segment_count = points.len().saturating_sub(1);
    mesh.reserve_vertices(segment_count * 4);
    mesh.reserve_triangles(segment_count * 2);
    for segment in points.windows(2) {
        let base_index = mesh.vertices.len() as u32;
        let [start, end] = segment else {
            unreachable!("windows of two always contain two points")
        };
        mesh.colored_vertex(*start, color);
        mesh.colored_vertex(egui::pos2(start.x, baseline), color);
        mesh.colored_vertex(*end, color);
        mesh.colored_vertex(egui::pos2(end.x, baseline), color);
        mesh.add_triangle(base_index, base_index + 1, base_index + 2);
        mesh.add_triangle(base_index + 2, base_index + 1, base_index + 3);
    }
    mesh
}

#[derive(Clone, Copy)]
struct UsageReference {
    label: &'static str,
    value: i64,
    color: egui::Color32,
}

type UsageReferences = [Option<UsageReference>; 2];

fn usage_references(
    request: Option<i64>,
    limit: Option<i64>,
    labels: [&'static str; 2],
) -> UsageReferences {
    [
        request.map(|value| UsageReference {
            label: labels[0],
            value,
            color: status::WARNING,
        }),
        limit.map(|value| UsageReference {
            label: labels[1],
            value,
            color: status::CRITICAL,
        }),
    ]
}

fn usage_chart_pair_labels(ui: &mut egui::Ui) {
    ui.columns(2, |columns| {
        columns[0].label(egui::RichText::new("CPU").color(gray::_500));
        columns[1].label(egui::RichText::new("Memory").color(gray::_500));
    });
}

fn has_usage_references(references: &UsageReferences) -> bool {
    references.iter().any(Option::is_some)
}

fn format_history_window() -> String {
    let seconds = POD_USAGE_HISTORY_WINDOW.whole_seconds();
    if seconds % 60 == 0 {
        format!("{}-minute", seconds / 60)
    } else {
        format!("{seconds}-second")
    }
}

fn dashed_reference_line(
    painter: &egui::Painter,
    left: f32,
    right: f32,
    y: f32,
    stroke: egui::Stroke,
) {
    draw_dashed_horizontal_line(painter, left, right, y, stroke, 3.0, 3.0);
}

fn draw_chart_axes(
    painter: &egui::Painter,
    plot: egui::Rect,
    format: &impl Fn(i64) -> String,
    max: f32,
) {
    let tick_color = gray::_300;
    dashed_grid_line(painter, plot.left(), plot.right(), plot.center().y);
    for fraction in [0.0, 1.0] {
        let y = egui::lerp(plot.bottom()..=plot.top(), fraction);
        painter.line_segment(
            [egui::pos2(plot.left() - 3.0, y), egui::pos2(plot.left(), y)],
            egui::Stroke::new(1.0, tick_color),
        );
        painter.line_segment(
            [
                egui::pos2(plot.right(), y),
                egui::pos2(plot.right() + 3.0, y),
            ],
            egui::Stroke::new(1.0, tick_color),
        );
        let value = (max * fraction).round() as i64;
        painter.text(
            egui::pos2(plot.left() - 6.0, y),
            egui::Align2::RIGHT_CENTER,
            if value == 0 {
                "0".to_owned()
            } else {
                format(value)
            },
            typography::chart_axis(),
            gray::_500,
        );
    }
    let time_labels = history_axis_labels();
    for (fraction, label, align) in [
        (0.0, time_labels[0].as_str(), egui::Align2::LEFT_TOP),
        (1.0, time_labels[1].as_str(), egui::Align2::RIGHT_TOP),
    ] {
        let x = egui::lerp(plot.left()..=plot.right(), fraction);
        painter.line_segment(
            [
                egui::pos2(x, plot.bottom()),
                egui::pos2(x, plot.bottom() + 3.0),
            ],
            egui::Stroke::new(1.0, tick_color),
        );
        painter.text(
            egui::pos2(x, plot.bottom() + 4.0),
            align,
            label,
            typography::chart_axis(),
            gray::_500,
        );
    }
}

fn dashed_grid_line(painter: &egui::Painter, left: f32, right: f32, y: f32) {
    draw_dashed_horizontal_line(
        painter,
        left,
        right,
        y,
        egui::Stroke::new(1.0, gray::_200),
        2.0,
        2.0,
    );
}

fn draw_dashed_horizontal_line(
    painter: &egui::Painter,
    left: f32,
    right: f32,
    y: f32,
    stroke: egui::Stroke,
    dash_length: f32,
    gap_length: f32,
) {
    let mut start = left;
    while start < right {
        let end = (start + dash_length).min(right);
        painter.line_segment([egui::pos2(start, y), egui::pos2(end, y)], stroke);
        start += dash_length + gap_length;
    }
}

fn history_axis_labels() -> [String; 2] {
    let seconds = POD_USAGE_HISTORY_WINDOW.whole_seconds();
    let unit = if seconds % 60 == 0 { "m" } else { "s" };
    let amount = if unit == "m" { seconds / 60 } else { seconds };
    [format!("{amount}{unit} ago"), "now".to_owned()]
}

fn total_resource_thresholds(
    containers: &[PodContainerDetail],
    thresholds: impl Fn(&PodContainerDetail) -> PodResourceThresholds,
) -> PodResourceThresholds {
    PodResourceThresholds {
        cpu_nanocores: sum_resource_quantities(
            containers
                .iter()
                .map(|container| thresholds(container).cpu_nanocores),
        ),
        memory_bytes: sum_resource_quantities(
            containers
                .iter()
                .map(|container| thresholds(container).memory_bytes),
        ),
    }
}

fn sum_resource_quantities(values: impl Iterator<Item = Option<i64>>) -> Option<i64> {
    let mut found = false;
    let mut total = 0_i64;
    for value in values.flatten() {
        found = true;
        total = total.checked_add(value)?;
    }
    found.then_some(total)
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

fn metadata_maps(ui: &mut egui::Ui, detail: &ResourceDetail) {
    disclosure_card(
        ui,
        "labels-and-annotations-open",
        "Labels & annotations",
        false,
        |ui| {
            ui.label(egui::RichText::new("Labels").strong().color(gray::_800));
            InspectorDetails::show_properties(
                ui,
                &[DetailRow::new(detail.labels.iter().map(|(key, value)| {
                    DetailCell::new(key.as_str(), value.as_str())
                        .copyable_as(format!("{key}={value}"))
                }))],
            );
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("Annotations")
                    .strong()
                    .color(gray::_800),
            );
            InspectorDetails::show_properties(
                ui,
                &[DetailRow::new(detail.annotations.iter().map(
                    |(key, value)| {
                        DetailCell::new(key.as_str(), value.as_str())
                            .copyable_as(format!("{key}={value}"))
                    },
                ))],
            );
        },
    );
}

fn show_events(ui: &mut egui::Ui, events: &[ResourceEvent], error: Option<&str>) {
    WorkspaceCard::new().padding(0).show(ui, |ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), CARD_HEADER_HEIGHT),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.add_space(CARD_HEADER_PADDING);
                ui.label(
                    egui::RichText::new("Events")
                        .strong()
                        .font(typography::section_heading())
                        .color(gray::_800),
                );
            },
        );
        ui.separator();
        egui::Frame::new()
            .inner_margin(egui::Margin::same(CARD_CONTENT_PADDING))
            .show(ui, |ui| {
                if let Some(error) = error {
                    error_card(ui, "Unable to load events", error);
                } else if events.is_empty() {
                    ui.label(egui::RichText::new("No events recorded.").color(gray::_500));
                } else {
                    let rows = events
                        .iter()
                        .map(|event| {
                            DetailTableRow::new([
                                DetailTableCell::new(DetailValue::Status {
                                    text: event.type_.as_str().into(),
                                    tone: event_tone(&event.type_),
                                }),
                                DetailTableCell::new(DetailValue::Text(
                                    event.reason.as_str().into(),
                                ))
                                .copyable(),
                                DetailTableCell::new(DetailValue::Text(
                                    event.message.as_str().into(),
                                ))
                                .copyable(),
                                DetailTableCell::new(DetailValue::Text(
                                    event.source.as_deref().unwrap_or("Kubernetes").into(),
                                ))
                                .copyable(),
                                DetailTableCell::new(DetailValue::Text(
                                    format!("{} ago", format_age(event.last_timestamp)).into(),
                                )),
                            ])
                        })
                        .collect::<Vec<_>>();
                    InspectorDetails::show_table(
                        ui,
                        &[
                            DetailColumn::new("Type"),
                            DetailColumn::new("Reason"),
                            DetailColumn::new("Message").weight(2.0),
                            DetailColumn::new("Source"),
                            DetailColumn::new("Time"),
                        ],
                        &rows,
                    );
                }
            });
    });
}

fn show_additional_sections(
    ui: &mut egui::Ui,
    detail: &ResourceDetail,
    resource_navigation: &ResourceNavigation,
    pending_action: &mut Option<ResourceAction>,
) {
    if let ResourceDetailPayload::Pod(pod) = &detail.payload {
        disclosure_card(ui, "conditions", "Conditions", false, |ui| {
            if pod.conditions.is_empty() {
                ui.label(egui::RichText::new("No conditions reported.").color(gray::_500));
            } else {
                let rows = pod
                    .conditions
                    .iter()
                    .map(|condition| {
                        DetailTableRow::new([
                            DetailTableCell::new(DetailValue::Text(
                                condition.type_.as_str().into(),
                            )),
                            DetailTableCell::new(DetailValue::Status {
                                text: condition.status.as_str().into(),
                                tone: condition_tone(&condition.status),
                            }),
                            DetailTableCell::new(DetailValue::Text(
                                condition.reason.as_deref().unwrap_or("-").into(),
                            ))
                            .copyable(),
                            DetailTableCell::new(DetailValue::Text(
                                condition.message.as_deref().unwrap_or("-").into(),
                            ))
                            .copyable(),
                        ])
                    })
                    .collect::<Vec<_>>();
                InspectorDetails::show_table(
                    ui,
                    &[
                        DetailColumn::new("Type"),
                        DetailColumn::new("Status"),
                        DetailColumn::new("Reason"),
                        DetailColumn::new("Message").weight(2.0),
                    ],
                    &rows,
                );
            }
        });
        ui.add_space(16.0);
    }
    disclosure_card(ui, "owner-references", "Owner references", false, |ui| {
        if detail.owners.is_empty() {
            ui.label(egui::RichText::new("No owner references.").color(gray::_500));
        } else {
            for owner in &detail.owners {
                let label = owner.label();
                let copy_value = format!("{}/{} {}", owner.api_version, owner.kind, owner.name);
                ui.horizontal(|ui| {
                    if let Some(action) = resource_owner::navigation_action(
                        resource_navigation,
                        owner,
                        detail.namespace.as_deref(),
                    ) {
                        let response = ui.add(
                            egui::Label::new(
                                egui::RichText::new(&label)
                                    .font(typography::metadata())
                                    .color(indigo::_600),
                            )
                            .sense(egui::Sense::click()),
                        );
                        response.clone().with_pointing_hand().widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::WidgetType::Button,
                                response.enabled(),
                                format!("Open details for {label}"),
                            )
                        });
                        if response.clicked() {
                            resource_owner::queue_navigation_action(pending_action, action);
                        }
                    } else {
                        ui.label(
                            egui::RichText::new(&label)
                                .font(typography::metadata())
                                .color(gray::_900),
                        )
                        .on_hover_text(resource_owner::unavailable_tooltip(owner));
                    }
                    if TailwindButton::secondary(format!("Copy owner {label}"))
                        .size(ButtonSize::Sm)
                        .show(ui)
                        .clicked()
                    {
                        ui.ctx().copy_text(copy_value);
                    }
                });
            }
        }
    });
    if let ResourceDetailPayload::Pod(pod) = &detail.payload {
        ui.add_space(16.0);
        disclosure_card(
            ui,
            "resource-configuration",
            "Resource configuration",
            true,
            |ui| {
                InspectorDetails::show_properties(
                    ui,
                    &[DetailRow::new([
                        DetailCell::new(
                            "Restart policy",
                            pod.restart_policy.as_deref().unwrap_or("-"),
                        ),
                        DetailCell::new(
                            "Service account",
                            pod.service_account_name.as_deref().unwrap_or("-"),
                        ),
                        DetailCell::new("DNS policy", pod.dns_policy.as_deref().unwrap_or("-")),
                    ])],
                );
            },
        );
    }
}

fn pod_phase_tone(phase: &str) -> DetailTone {
    match phase {
        "Running" | "Succeeded" => DetailTone::Success,
        "Pending" => DetailTone::Warning,
        "Failed" => DetailTone::Danger,
        _ => DetailTone::Neutral,
    }
}

fn event_tone(event_type: &str) -> DetailTone {
    match event_type {
        "Normal" => DetailTone::Success,
        "Warning" => DetailTone::Warning,
        _ => DetailTone::Neutral,
    }
}

fn condition_tone(condition_status: &str) -> DetailTone {
    match condition_status {
        "True" => DetailTone::Success,
        "False" => DetailTone::Neutral,
        "Unknown" => DetailTone::Warning,
        _ => DetailTone::Neutral,
    }
}

pub(super) fn disclosure_card(
    ui: &mut egui::Ui,
    id_source: &str,
    title: &str,
    default_open: bool,
    add_content: impl FnOnce(&mut egui::Ui),
) {
    WorkspaceCard::new().padding(0).show(ui, |ui| {
        let id = ui.id().with(id_source);
        let mut open = ui
            .data(|data| data.get_temp::<bool>(id))
            .unwrap_or(default_open);
        let (header, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), CARD_HEADER_HEIGHT),
            egui::Sense::click(),
        );
        let response = response.with_pointing_hand();
        if response.clicked() {
            open = !open;
            ui.data_mut(|data| data.insert_temp(id, open));
        }
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::CollapsingHeader,
                ui.is_enabled(),
                open,
                title,
            )
        });
        let painter = ui.painter();
        painter.text(
            header.left_center() + egui::vec2(CARD_HEADER_PADDING, 0.0),
            egui::Align2::LEFT_CENTER,
            title,
            typography::body(),
            gray::_800,
        );
        painter.text(
            header.right_center() - egui::vec2(CARD_HEADER_PADDING, 0.0),
            egui::Align2::RIGHT_CENTER,
            if open { "⌃" } else { "⌄" },
            typography::section_heading(),
            gray::_700,
        );
        if open {
            ui.separator();
            egui::Frame::new()
                .inner_margin(egui::Margin::same(CARD_CONTENT_PADDING))
                .show(ui, add_content);
        }
    });
}

fn section_header(ui: &mut egui::Ui, title: &str, detail: Option<String>) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(title)
                .strong()
                .font(typography::section_heading())
                .color(gray::_800),
        );
        if let Some(detail) = detail {
            ui.label(
                egui::RichText::new(detail)
                    .font(typography::body())
                    .color(gray::_600),
            );
        }
    });
    ui.add_space(6.0);
}

fn environment_variables(
    ui: &mut egui::Ui,
    variables: &[crate::resource_detail::PodEnvironmentVariableDetail],
) {
    ui.label(
        egui::RichText::new("Environment variables")
            .font(typography::metadata())
            .color(gray::_500),
    );
    ui.add_space(4.0);
    egui::Frame::new()
        .fill(gray::_100)
        .stroke(egui::Stroke::new(1.0, gray::_200))
        .corner_radius(radius::subtle())
        .inner_margin(egui::Margin::symmetric(
            spacing::SM as i8,
            (spacing::SM + 2.0) as i8,
        ))
        .show(ui, |ui| {
            environment_variable_header(ui);
            for variable in variables {
                ui.add_space(2.0);
                environment_variable_row(ui, variable);
            }
        });
}

fn environment_variable_header(ui: &mut egui::Ui) {
    ui.columns(3, |columns| {
        environment_variable_cell(&mut columns[0], "Key", true);
        environment_variable_cell(&mut columns[1], "Value", true);
        environment_variable_cell(&mut columns[2], "Source", true);
    });
}

fn environment_variable_row(
    ui: &mut egui::Ui,
    variable: &crate::resource_detail::PodEnvironmentVariableDetail,
) {
    ui.columns(3, |columns| {
        environment_variable_cell(&mut columns[0], &variable.name, false);
        environment_variable_value_cell(&mut columns[1], variable);
        environment_variable_source_cell(&mut columns[2], variable);
    });
}

fn environment_variable_cell(ui: &mut egui::Ui, value: &str, header: bool) {
    let text = egui::RichText::new(value)
        .monospace()
        .font(typography::monospace())
        .color(if header { gray::_600 } else { gray::_800 });
    let text = if header { text.strong() } else { text };
    egui::Frame::new()
        .fill(WHITE)
        .stroke(egui::Stroke::new(1.0, gray::_200))
        .corner_radius(radius::subtle())
        .inner_margin(egui::Margin::symmetric(
            (spacing::SM - 2.0) as i8,
            spacing::XS as i8,
        ))
        .show(ui, |ui| {
            ui.set_max_width(ui.available_width());
            ui.add(egui::Label::new(text).selectable(!header).wrap());
        });
}

fn environment_variable_value_cell(
    ui: &mut egui::Ui,
    variable: &crate::resource_detail::PodEnvironmentVariableDetail,
) {
    let secret = matches!(
        variable.source,
        crate::resource_detail::PodEnvironmentVariableSource::SecretKey { .. }
    );
    let revealed = secret && environment_variable_secret_revealed(ui, variable);
    let value = if secret && !revealed {
        "••••••"
    } else {
        variable.value.as_deref().unwrap_or("Unavailable")
    };
    egui::Frame::new()
        .fill(WHITE)
        .stroke(egui::Stroke::new(1.0, gray::_200))
        .corner_radius(radius::subtle())
        .inner_margin(egui::Margin::symmetric(
            (spacing::SM - 2.0) as i8,
            spacing::XS as i8,
        ))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.set_max_width(ui.available_width());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                if secret && variable.value.is_some() {
                    let action = if revealed { "Hide" } else { "Reveal" };
                    let response = components::icons::eye_button(ui, 14.0, gray::_600, action);
                    if response.on_hover_text(action).clicked() {
                        ui.data_mut(|data| {
                            data.insert_temp(
                                environment_variable_secret_id(ui, variable),
                                !revealed,
                            )
                        });
                    }
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(value)
                                .monospace()
                                .font(typography::monospace())
                                .color(gray::_800),
                        )
                        .selectable(true)
                        .wrap(),
                    );
                });
            });
        });
}

fn environment_variable_source_cell(
    ui: &mut egui::Ui,
    variable: &crate::resource_detail::PodEnvironmentVariableDetail,
) {
    let source = environment_variable_source_label(variable);
    egui::Frame::new()
        .fill(WHITE)
        .stroke(egui::Stroke::new(1.0, gray::_200))
        .corner_radius(radius::subtle())
        .inner_margin(egui::Margin::symmetric(
            (spacing::SM - 2.0) as i8,
            spacing::XS as i8,
        ))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.set_max_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                let response = ui.add(
                    egui::Label::new(
                        egui::RichText::new(&source)
                            .font(typography::metadata())
                            .color(gray::_600),
                    )
                    .wrap()
                    .sense(egui::Sense::click()),
                );
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Button,
                        response.enabled(),
                        "Copy environment variable source",
                    )
                });
                if response.clicked() {
                    ui.ctx().copy_text(source);
                }
            });
        });
}

fn environment_variable_secret_revealed(
    ui: &egui::Ui,
    variable: &crate::resource_detail::PodEnvironmentVariableDetail,
) -> bool {
    ui.data(|data| {
        data.get_temp::<bool>(environment_variable_secret_id(ui, variable))
            .unwrap_or(false)
    })
}

fn environment_variable_secret_id(
    _ui: &egui::Ui,
    variable: &crate::resource_detail::PodEnvironmentVariableDetail,
) -> egui::Id {
    egui::Id::new((
        "environment-variable-secret",
        &variable.name,
        environment_variable_source_label(variable),
    ))
}

fn environment_variable_source_label(
    variable: &crate::resource_detail::PodEnvironmentVariableDetail,
) -> String {
    use crate::resource_detail::PodEnvironmentVariableSource;

    let resolved = if variable.value.is_some() {
        "resolved"
    } else {
        "unavailable"
    };
    match &variable.source {
        PodEnvironmentVariableSource::Literal => "Literal".to_owned(),
        PodEnvironmentVariableSource::ConfigMapKey {
            name,
            key,
            optional,
        } => {
            format!(
                "ConfigMap {name}/{key}{} · {resolved}",
                optional_label(*optional)
            )
        }
        PodEnvironmentVariableSource::SecretKey {
            name,
            key,
            optional,
        } => {
            format!(
                "Secret {name}/{key}{} · {resolved}",
                optional_label(*optional)
            )
        }
        PodEnvironmentVariableSource::Field { path } => format!("Field {path} · {resolved}"),
        PodEnvironmentVariableSource::ResourceField {
            resource,
            container_name,
        } => format!(
            "Resource field {resource}{} · {resolved}",
            container_name
                .as_deref()
                .map(|name| format!(" ({name})"))
                .unwrap_or_default()
        ),
        PodEnvironmentVariableSource::ConfigMapImport { name, optional, .. } => {
            format!(
                "ConfigMap import {name}{} · {resolved}",
                optional_label(*optional)
            )
        }
        PodEnvironmentVariableSource::SecretImport { name, optional, .. } => {
            format!(
                "Secret import {name}{} · {resolved}",
                optional_label(*optional)
            )
        }
        PodEnvironmentVariableSource::Unspecified => "Unspecified source".to_owned(),
    }
}

fn optional_label(optional: bool) -> &'static str {
    if optional { " (optional)" } else { "" }
}

fn show_usage_value_grid(ui: &mut egui::Ui, usage: Option<(i64, i64)>) {
    let cells = match usage {
        Some((cpu, memory)) => [
            DetailCell::new("CPU", format_cpu(cpu)),
            DetailCell::new("Memory", format_memory(memory)),
        ],
        None => [
            DetailCell::unavailable("CPU"),
            DetailCell::unavailable("Memory"),
        ],
    };
    InspectorDetails::show_properties(ui, &[DetailRow::new(cells)]);
}

fn displayed_usage_values(
    usage: Option<(i64, i64)>,
    metrics_error: Option<&str>,
) -> Option<(i64, i64)> {
    metrics_error.is_none().then_some(usage).flatten()
}

fn show_metrics_api_unavailable(
    ui: &mut egui::Ui,
    requests: PodResourceThresholds,
    limits: PodResourceThresholds,
) {
    ui.label(
        egui::RichText::new("Metrics API unavailable")
            .font(typography::body())
            .color(gray::_700),
    );
    ui.label(
        egui::RichText::new("Live CPU and memory usage requires the Kubernetes Metrics API.")
            .font(typography::metadata())
            .color(gray::_500),
    );
    ui.add_space(8.0);
    InspectorDetails::show_properties(
        ui,
        &[
            DetailRow::new([
                DetailCell::new(
                    "CPU request",
                    requests
                        .cpu_nanocores
                        .map(format_cpu)
                        .unwrap_or_else(|| "Not set".into()),
                ),
                DetailCell::new(
                    "CPU limit",
                    limits
                        .cpu_nanocores
                        .map(format_cpu)
                        .unwrap_or_else(|| "Not set".into()),
                ),
            ]),
            DetailRow::new([
                DetailCell::new(
                    "Memory request",
                    requests
                        .memory_bytes
                        .map(format_memory)
                        .unwrap_or_else(|| "Not set".into()),
                ),
                DetailCell::new(
                    "Memory limit",
                    limits
                        .memory_bytes
                        .map(format_memory)
                        .unwrap_or_else(|| "Not set".into()),
                ),
            ]),
        ],
    );
}

fn show_node_metrics_api_unavailable(ui: &mut egui::Ui, allocatable: PodResourceThresholds) {
    ui.label(
        egui::RichText::new("Metrics API unavailable")
            .font(typography::body())
            .color(gray::_700),
    );
    ui.label(
        egui::RichText::new("Live CPU and memory usage requires the Kubernetes Metrics API.")
            .font(typography::metadata())
            .color(gray::_500),
    );
    ui.add_space(8.0);
    InspectorDetails::show_properties(
        ui,
        &[DetailRow::new([
            DetailCell::new(
                "CPU allocatable",
                allocatable
                    .cpu_nanocores
                    .map(format_cpu)
                    .unwrap_or_else(|| "Not reported".into()),
            ),
            DetailCell::new(
                "Memory allocatable",
                allocatable
                    .memory_bytes
                    .map(format_memory)
                    .unwrap_or_else(|| "Not reported".into()),
            ),
        ])],
    );
}

fn chip_row(ui: &mut egui::Ui, label: &str, values: &[String]) {
    ui.label(
        egui::RichText::new(label)
            .font(typography::metadata())
            .color(gray::_500),
    );
    ui.with_layout(
        egui::Layout::left_to_right(egui::Align::TOP).with_main_wrap(true),
        |ui| {
            for value in values {
                let chip_width = (value.chars().count() as f32 * 6.7 + 28.0).clamp(54.0, 320.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(chip_width, 0.0),
                    egui::Layout::top_down(egui::Align::LEFT),
                    |ui| {
                        egui::Frame::new()
                            .fill(gray::_100)
                            .stroke(egui::Stroke::new(1.0, gray::_200))
                            .corner_radius(radius::subtle())
                            .inner_margin(egui::Margin::symmetric((spacing::SM - 3.0) as i8, 0))
                            .show(ui, |ui| {
                                ui.set_max_width(chip_width - 10.0);
                                ui.horizontal_top(|ui| {
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(value)
                                                .monospace()
                                                .font(typography::monospace())
                                                .color(gray::_800),
                                        )
                                        .wrap(),
                                    );
                                    let (icon_rect, copy_action) = ui.allocate_exact_size(
                                        egui::vec2(14.0, 14.0),
                                        egui::Sense::click(),
                                    );
                                    copy_action.widget_info(|| {
                                        egui::WidgetInfo::labeled(
                                            egui::WidgetType::Button,
                                            copy_action.enabled(),
                                            format!("Copy {label}"),
                                        )
                                    });
                                    if copy_action.hovered() {
                                        components::icons::document_duplicate_icon()
                                            .fit_to_exact_size(egui::vec2(14.0, 14.0))
                                            .tint(gray::_700)
                                            .paint_at(ui, icon_rect);
                                    }
                                    if copy_action.clicked() {
                                        ui.ctx().copy_text(value.clone());
                                    }
                                });
                            });
                    },
                );
            }
        },
    );
}

fn volume_detail_row(ui: &mut egui::Ui, volume: &crate::resource_detail::PodVolumeDetail) {
    InspectorDetails::show_properties(
        ui,
        &[
            DetailRow::new([
                DetailCell::new("Type", volume.kind.as_str()),
                DetailCell::new("Source", volume.source.as_str()),
                DetailCell::new("Read-only", if volume.read_only { "true" } else { "false" }),
            ]),
            DetailRow::new([DetailCell::new(
                "Mount path",
                volume.mount_path.as_deref().unwrap_or("-"),
            )]),
        ],
    );
}

fn error_card(ui: &mut egui::Ui, title: &str, error: &str) {
    ui.label(egui::RichText::new(title).strong().color(status::DANGER));
    ui.label(
        egui::RichText::new(error)
            .font(typography::metadata())
            .color(gray::_600),
    );
}
