use super::*;
use crate::resource_table::{
    CONTAINERS_COLUMN, CPU_COLUMN, MEMORY_COLUMN, READY_COLUMN, REPLICAS_UNREADY_CELL,
    RESTARTS_COLUMN, ROLES_COLUMN, VERSION_COLUMN,
};
use crate::ui::preference_table::{
    ACTIONS_COLUMN_ID, AGE_COLUMN_ID, ColumnControls, PreferenceTable, actions_column, age_column,
    fill_first_column,
};
use crate::ui::resource_table_cache::{Grouping, PreparedGroup};
use components::design::size;
use components::tooltip::{TooltipExt, elided_label};
use components::{GroupHeader, MenuLook, ProblemTicks, TableGroup};
use components::{SortDirection, SortState};
use egui::{Rect, pos2};
use resource_data::CLUSTER_GROUP;

/// The columns a window too narrow for them leaves out (S08): the details
/// beside a row's name, status and age.
fn hidden_when_compact(column_id: &str) -> bool {
    matches!(
        column_id,
        CONTAINERS_COLUMN
            | CPU_COLUMN
            | MEMORY_COLUMN
            | NODE_COLUMN
            | ROLES_COLUMN
            | VERSION_COLUMN
    ) || column_id == "owner"
        || column_id.starts_with("metadata-")
}

pub(super) struct ResourceTableConfiguration {
    definition: crate::resource_table::ResourceTableDefinition,
    pub(super) metadata_columns: Vec<super::super::table_preferences::CustomMetadataColumn>,
    /// The visible columns, their widths and the sort.
    pub(super) layout: PreferenceTable,
    /// How the rows group under group headers.
    pub(super) grouping: Grouping,
}

impl ResourceTableConfiguration {
    /// The table of the visible columns.
    pub(super) fn table(&self, api_resource: &crate::api_resource::ApiResource) -> Table {
        self.layout.table(format!(
            "resource-table-{}-{}-{}",
            api_resource.group, api_resource.version, api_resource.name
        ))
    }
}

pub(super) fn resource_table_configuration(
    available_width: f32,
    api_resource: &crate::api_resource::ApiResource,
    custom_columns: &[CustomResourceColumn],
    show_namespace_column: bool,
    compact: bool,
    table_preferences: &mut PersistedResourceTablePreferences,
) -> ResourceTableConfiguration {
    let definition = table_definition(api_resource, custom_columns);
    let table_key = ResourceTableKey::workspace(api_resource);
    let metadata_columns = table_preferences.custom_columns(&table_key);
    let mut column_definitions = vec![TableColumnDefinition::sortable("name", "Name", 160.0)];
    if show_namespace_column {
        column_definitions.push(TableColumnDefinition::sortable(
            "namespace",
            "Namespace",
            size::TABLE_NAMESPACE_COLUMN,
        ));
    }
    column_definitions.push(
        TableColumnDefinition::sortable("owner", "Owner", size::TABLE_OWNER_COLUMN)
            .hidden_by_default(),
    );
    column_definitions.extend(
        definition
            .columns
            .iter()
            .map(|column| TableColumnDefinition {
                numeric: column.numeric,
                ..TableColumnDefinition::sortable(&column.id, &column.label, column.initial_width)
            }),
    );
    column_definitions.extend(
        metadata_columns
            .iter()
            .map(|column| TableColumnDefinition::sortable(&column.id(), &column.label, 160.0)),
    );
    column_definitions.extend([age_column(), actions_column()]);
    // A narrow window leaves out columns, which the name column then fills.
    let mut filled = column_definitions.clone();
    if compact {
        for column in &mut filled {
            column.default_hidden |= hidden_when_compact(&column.id);
        }
    }
    fill_first_column(&mut filled, available_width, size::TABLE_CHECKBOX_COLUMN);
    column_definitions[0].default_width = filled[0].default_width;
    let grouping = if api_resource.is_node() {
        Grouping::Cluster
    } else if api_resource.namespaced && table_preferences.groups_by_namespace(&table_key) {
        Grouping::Namespace
    } else {
        Grouping::None
    };

    let mut layout = PreferenceTable::new(table_preferences, table_key, column_definitions);
    if compact {
        layout.compact(hidden_when_compact, |id| {
            (id == READY_COLUMN).then_some(size::TABLE_READY_COLUMN_COMPACT)
        });
    }
    ResourceTableConfiguration {
        definition,
        metadata_columns,
        layout,
        grouping,
    }
}

/// The em dash shown, and the tooltip explaining it, for a metadata column
/// whose key the resource does not have.
fn missing_metadata_reason(
    column: &super::super::table_preferences::CustomMetadataColumn,
) -> String {
    match column.source {
        MetadataColumnSource::Label => format!("No label {}", column.key),
        MetadataColumnSource::Annotation => format!("No annotation {}", column.key),
    }
}

/// What the resource table did this frame.
#[derive(Default)]
pub(super) struct ResourceTableOutcome {
    pub(super) action: Option<ResourceAction>,
    /// The group whose header chip was clicked: collapse or expand it.
    pub(super) toggled_group: Option<String>,
    /// The problem row whose tick was clicked (its uid): scroll to it.
    pub(super) clicked_problem: Option<String>,
}

/// The index of the row `uid` among the data rows the table shows (those of
/// collapsed groups do not), as [`Table::inspected_row`] counts them.
pub(super) fn displayed_row_index(
    prepared: &PreparedResourceTable,
    collapsed: &HashSet<String>,
    uid: &str,
) -> Option<usize> {
    let mut index = 0;
    for group in shown_groups(prepared, collapsed) {
        let rows = &prepared.rows[group.rows.clone()];
        if let Some(offset) = rows.iter().position(|row| row_uid(row) == Some(uid)) {
            return Some(index + offset);
        }
        index += rows.len();
    }
    None
}

/// The groups whose rows show: those not collapsed.
fn shown_groups<'a>(
    prepared: &'a PreparedResourceTable,
    collapsed: &'a HashSet<String>,
) -> impl Iterator<Item = &'a PreparedGroup> {
    prepared.groups.iter().filter(|group| {
        group
            .name
            .as_ref()
            .is_none_or(|name| !collapsed.contains(name))
    })
}

fn row_uid(row: &PreparedResourceTableRow) -> Option<&str> {
    match row {
        PreparedResourceTableRow::Resource(identity) => Some(&identity.uid),
        PreparedResourceTableRow::HiddenBySearch(_) => None,
    }
}

/// The problem rows in the order the table shows them: `(uid, group)`, with
/// those of collapsed groups left out unless `all`.
pub(super) fn problem_rows<'a>(
    prepared: &'a PreparedResourceTable,
    collapsed: &'a HashSet<String>,
    all: bool,
) -> Vec<(&'a str, Option<&'a str>)> {
    prepared
        .groups
        .iter()
        .filter(|group| {
            all || group
                .name
                .as_ref()
                .is_none_or(|name| !collapsed.contains(name))
        })
        .flat_map(|group| {
            prepared.rows[group.rows.clone()]
                .iter()
                .filter_map(|row| match row {
                    PreparedResourceTableRow::Resource(identity) if identity.problem.is_some() => {
                        Some((identity.uid.as_str(), group.name.as_deref()))
                    }
                    _ => None,
                })
        })
        .collect()
}

/// Make the problem row after (or before) the one last stepped to the one
/// to scroll to, expanding its group when it is collapsed.
pub(super) fn step_to_problem(
    prepared: &PreparedResourceTable,
    search: &mut ResourceSearchState,
    step: ProblemStep,
) {
    let rows = problem_rows(prepared, &search.collapsed_groups, true)
        .into_iter()
        .map(|(uid, group)| (uid.to_owned(), group.map(str::to_owned)))
        .collect::<Vec<_>>();
    let Some(last) = rows.len().checked_sub(1) else {
        return;
    };
    let current = search
        .problem_cursor
        .as_deref()
        .and_then(|cursor| rows.iter().position(|(uid, _)| uid == cursor));
    let index = match (current, step) {
        (None, ProblemStep::Next) => 0,
        (None, ProblemStep::Previous) => last,
        (Some(current), ProblemStep::Next) if current == last => 0,
        (Some(current), ProblemStep::Next) => current + 1,
        (Some(0), ProblemStep::Previous) => last,
        (Some(current), ProblemStep::Previous) => current - 1,
    };
    let (uid, group) = &rows[index];
    if let Some(group) = group {
        search.collapsed_groups.remove(group);
    }
    search.problem_cursor = Some(uid.clone());
    search.scroll_to = Some(uid.clone());
}

/// What a group header says on its end: how much of the group shows, or for
/// the Nodes their roles.
fn group_window(
    group: &PreparedGroup,
    prepared: &PreparedResourceTable,
    options: &ResourceTableOptions<'_>,
) -> Option<String> {
    let shown = group.rows.len();
    if group.name.as_deref() == Some(CLUSTER_GROUP) {
        return Some(node_roles_summary(
            prepared.rows[group.rows.clone()]
                .iter()
                .filter_map(|row| match row {
                    PreparedResourceTableRow::Resource(identity) => {
                        resolve_prepared_resource(options.resource_cache, prepared, identity)
                    }
                    PreparedResourceTableRow::HiddenBySearch(_) => None,
                }),
        ));
    }
    if shown < group.total {
        Some(format!(
            "{} of {} shown",
            components::search::format_count(shown),
            components::search::format_count(group.total)
        ))
    } else if prepared
        .groups
        .iter()
        .filter(|group| group.name.is_some())
        .count()
        == 1
    {
        Some(format!(
            "all {} shown",
            components::search::format_count(shown)
        ))
    } else {
        None
    }
}

/// "control plane · 2 workers": the roles of the Nodes shown.
fn node_roles_summary<'a>(nodes: impl Iterator<Item = &'a MinimalResource>) -> String {
    let (mut control_planes, mut workers) = (0, 0);
    for node in nodes {
        let is_control_plane = matches!(
            node.cells.get(ROLES_COLUMN),
            Some(CellValue::List(roles))
                if roles.iter().any(|role| role == "control-plane" || role == "master")
        );
        if is_control_plane {
            control_planes += 1;
        } else {
            workers += 1;
        }
    }
    let control_plane = match control_planes {
        0 => None,
        1 => Some("control plane".to_owned()),
        count => Some(format!("{count} control planes")),
    };
    let workers = (workers > 0).then(|| components::counted(workers, "worker", "workers"));
    [control_plane, workers]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ")
}

pub(super) fn show_resource_table(
    ui: &mut egui::Ui,
    api_resource: &crate::api_resource::ApiResource,
    prepared: &PreparedResourceTable,
    configuration: &ResourceTableConfiguration,
    options: ResourceTableOptions<'_>,
    controls: ResourceTableControls<'_>,
) -> ResourceTableOutcome {
    let theme = ui.tokens();
    let pending_action = RefCell::new(None);
    let ResourceTableConfiguration {
        definition,
        metadata_columns,
        layout,
        ..
    } = configuration;
    let collapsed = options.collapsed_groups;
    let inspected_row = options
        .inspected_uid
        .or(options.problem_cursor)
        .and_then(|uid| displayed_row_index(prepared, collapsed, uid));
    let scroll_row = options
        .scroll_to
        .and_then(|uid| displayed_row_index(prepared, collapsed, uid));
    let table = configuration
        .table(api_resource)
        .selectable()
        .fill_available_height()
        .inspected_row(inspected_row)
        .scroll_to_row(scroll_row);

    let actions_menu = |menu: &mut components::Menu<'_>, resource: &MinimalResource| {
        show_resource_action_items(
            menu,
            api_resource,
            resource,
            &resource.log_containers,
            options.debug_image_presets,
            options.actions.supports_scale,
            &mut pending_action.borrow_mut(),
        );
    };
    // The first click of a frame wins.
    let queue = |action: ResourceAction| {
        pending_action.borrow_mut().get_or_insert(action);
    };
    let singular = singular_of(&api_resource.kind, &api_resource.name);
    let windows = prepared
        .groups
        .iter()
        .map(|group| group_window(group, prepared, &options))
        .collect::<Vec<_>>();
    let groups = prepared
        .groups
        .iter()
        .zip(&windows)
        .map(|(group, window)| {
            let rows = &prepared.rows[group.rows.clone()];
            let Some(name) = &group.name else {
                return TableGroup::ungrouped(rows);
            };
            let mut header = GroupHeader::new(name, group.total)
                .unit(&singular, &api_resource.name)
                .failing(group.failing);
            if let Some(window) = window {
                header = header.window(window);
            }
            TableGroup::new(header, rows).expanded(!collapsed.contains(name))
        })
        .collect::<Vec<_>>();
    let track_area = ui.available_rect_before_wrap();
    let response = layout.show_grouped(
        ui,
        table,
        groups,
        controls.selection,
        |row| match row {
            PreparedResourceTableRow::Resource(identity) => Some(&identity.uid),
            PreparedResourceTableRow::HiddenBySearch(_) => None,
        },
        |row| match row {
            PreparedResourceTableRow::Resource(identity) => identity.problem,
            PreparedResourceTableRow::HiddenBySearch(_) => None,
        },
        // Unsorted rows come in name order, unless a search ranks them.
        options
            .name_ordered
            .then(|| SortState::new("name", SortDirection::Ascending)),
        ColumnControls {
            preferences: controls.table_preferences,
            metadata_key_suggestions: Some(&prepared.metadata_key_suggestions),
            column_settings: controls.column_settings_to_open,
        },
        |ui, row, column_id| {
            let resource = match row {
                PreparedResourceTableRow::Resource(identity) => {
                    resolve_prepared_resource(options.resource_cache, prepared, identity)
                }
                PreparedResourceTableRow::HiddenBySearch(hidden_count) => {
                    if layout.is_first_column(column_id) {
                        let label = if *hidden_count == 1 {
                            "1 resource hidden by search".to_owned()
                        } else {
                            format!("{hidden_count} resources hidden by search")
                        };
                        TableRowBuilder::text(ui, &label, false);
                    }
                    return;
                }
            };
            let Some(resource) = resource else {
                return;
            };
            match column_id {
                "name" if options.actions.enabled => {
                    let response = TableRowBuilder::clickable_name(
                        ui,
                        &resource.name,
                        format!("Open details for {}", resource.name),
                    );
                    if response.clicked() {
                        queue(ResourceAction::open_details(resource));
                    }
                    MoreButton::show_context_menu_with(&response, MenuLook::ROW, |menu| {
                        actions_menu(menu, resource);
                    });
                }
                "name" => TableRowBuilder::name(ui, &resource.name),
                "namespace" => namespace_cell(ui, resource.namespace.as_deref()),
                "owner" => {
                    let Some(owner) = &resource.controller_owner else {
                        TableRowBuilder::missing(ui, "No controlling owner");
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
                                theme.accent.text,
                                format!("Open details for {label}"),
                            )
                            .tooltip(&label);
                            if response.clicked() {
                                queue(action);
                            }
                        } else {
                            TableRowBuilder::text(ui, &label, false);
                        }
                    } else {
                        elided_label(ui, type_scale::UI, &label, theme.text.muted)
                            .tooltip(resource_owner::unavailable_tooltip(owner));
                    }
                }
                id if metadata_columns.iter().any(|column| column.id() == id) => {
                    let column = metadata_columns
                        .iter()
                        .find(|column| column.id() == id)
                        .expect("metadata column was checked");
                    match resource_metadata_value(resource, column.source, &column.key) {
                        Some(value) => show_metadata_cell(ui, value),
                        None => TableRowBuilder::missing(ui, &missing_metadata_reason(column)),
                    }
                }
                id if definition.columns.iter().any(|column| column.id == id) => {
                    if id == NODE_COLUMN
                        && api_resource.is_pod()
                        && let Some(CellValue::Text(node_name)) = resource.cells.get(id)
                    {
                        if node_name == "-" {
                            TableRowBuilder::missing(
                                ui,
                                "Kubernetes has not assigned this Pod to a Node.",
                            );
                        } else if options.actions.enabled {
                            let response = TableRowBuilder::clickable_text(
                                ui,
                                node_name,
                                theme.text.secondary,
                                format!("Open details for Node {node_name}"),
                            );
                            if response.clicked() {
                                queue(ResourceAction::NavigateDetails {
                                    api_resource: crate::resource_handlers::node::api_resource(),
                                    name: node_name.clone(),
                                    namespace: None,
                                    uid: node_name.clone(),
                                });
                            }
                            MoreButton::show_context_menu_with(&response, MenuLook::ROW, |menu| {
                                actions_menu(menu, resource);
                            });
                        } else {
                            TableRowBuilder::text(ui, node_name, false);
                        }
                    } else if id == RESTARTS_COLUMN
                        && let Some(CellValue::Number(restarts)) = resource.cells.get(id)
                    {
                        // A restart count carries the row's problem.
                        let tone = if *restarts > 0 {
                            match row {
                                PreparedResourceTableRow::Resource(identity) => identity.problem,
                                PreparedResourceTableRow::HiddenBySearch(_) => None,
                            }
                        } else {
                            None
                        };
                        TableRowBuilder::problem_number(ui, &restarts.to_string(), tone);
                    } else if id == READY_COLUMN
                        && let Some(CellValue::Text(ready)) = resource.cells.get(id)
                        && let Some(CellValue::Status { tone, .. }) =
                            resource.cells.get(REPLICAS_UNREADY_CELL)
                    {
                        // Fewer replicas ready than wanted: the count says so.
                        TableRowBuilder::problem_number(ui, ready, Some(*tone));
                    } else {
                        let resolved =
                            resolved_resource_cell(resource, id, options.metrics, api_resource);
                        show_resource_cell(
                            ui,
                            id,
                            resolved.as_ref().or_else(|| resource.cells.get(id)),
                        );
                    }
                }
                AGE_COLUMN_ID => age_cell(ui, resource.creation_timestamp),
                ACTIONS_COLUMN_ID if options.actions.enabled => {
                    MoreButton::new(format!("More actions for {}", resource.name))
                        .look(MenuLook::ROW)
                        .show(ui, |menu| actions_menu(menu, resource));
                }
                _ => {}
            }
        },
        |row_response, row, column_id| {
            let PreparedResourceTableRow::Resource(identity) = row else {
                return;
            };
            if !options.actions.enabled {
                return;
            }
            let opens_details = column_id != ACTIONS_COLUMN_ID && row_response.clicked();
            // Runs for every cell each frame: find the resource only when it is acted on.
            if !opens_details && !MoreButton::context_menu_active(row_response) {
                return;
            }
            let Some(resource) =
                resolve_prepared_resource(options.resource_cache, prepared, identity)
            else {
                return;
            };
            if opens_details {
                queue(ResourceAction::open_details(resource));
            }
            MoreButton::show_context_menu_with(row_response, MenuLook::ROW, |menu| {
                actions_menu(menu, resource);
            });
        },
    );
    let track = Rect::from_min_max(
        pos2(
            track_area.right() + size::PROBLEM_TRACK_GAP,
            track_area.top() + size::PROBLEM_TRACK_TOP_INSET,
        ),
        pos2(
            track_area.right() + size::PROBLEM_TRACK_GAP + size::PROBLEM_TRACK,
            track_area.bottom() - size::PROBLEM_TRACK_BOTTOM_INSET,
        ),
    );
    let ticks = ProblemTicks::new(&response.ticks)
        .id_salt(("resource-problem-ticks", &api_resource.name))
        .thumb(response.visible.0, response.visible.1)
        .show_in(ui, track);
    let clicked_problem = ticks.clicked.and_then(|tick| {
        problem_rows(prepared, collapsed, false)
            .get(tick)
            .map(|(uid, _)| (*uid).to_owned())
    });
    ResourceTableOutcome {
        action: pending_action.into_inner(),
        toggled_group: response
            .toggled
            .and_then(|group| prepared.groups.get(group)?.name.clone()),
        clicked_problem,
    }
}

pub(super) fn resource_column_sort_value(
    resource: &MinimalResource,
    column_id: &str,
    metadata_columns: &[super::super::table_preferences::CustomMetadataColumn],
    metrics: ResourceMetrics<'_>,
    api_resource: &crate::api_resource::ApiResource,
) -> SortValue {
    match column_id {
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
        id => {
            if let Some(value) = metadata_columns
                .iter()
                .find(|column| column.id() == id)
                .and_then(|column| resource_metadata_value(resource, column.source, &column.key))
            {
                return SortValue::Text(value.to_owned());
            }
            if let Some(cell) = resolved_resource_cell(resource, id, metrics, api_resource) {
                return cell_sort_value(&cell);
            }
            resource
                .cells
                .get(id)
                .map(cell_sort_value)
                .unwrap_or(SortValue::Empty)
        }
    }
}

pub(super) fn resource_metadata_value<'a>(
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
    let color = ui.tokens().text.muted;
    TableRowBuilder::text_in(ui, type_scale::UI, value, color);
}
