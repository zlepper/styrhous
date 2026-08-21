use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn show_managed_resources_for(
    ui: &mut egui::Ui,
    api_resource: &crate::api_resource::ApiResource,
    resource_uid: &str,
    managed_resources: &[ManagedResource],
    managed_resources_error: Option<&str>,
    pending_action: &mut Option<ResourceAction>,
    mut table_preferences: Option<
        &mut super::super::table_preferences::PersistedResourceTablePreferences,
    >,
    mut column_settings: Option<
        &mut Option<super::super::resource_table_settings::ResourceTableSettingsTarget>,
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
pub(super) fn show_managed_resource_table(
    ui: &mut egui::Ui,
    detail_api_resource: &crate::api_resource::ApiResource,
    resource_uid: &str,
    kind: &str,
    rows: &[ManagedResourceRow],
    show_namespace_column: bool,
    pending_action: &mut Option<ResourceAction>,
    table_preferences: Option<
        &mut super::super::table_preferences::PersistedResourceTablePreferences,
    >,
    column_settings: Option<
        &mut Option<super::super::resource_table_settings::ResourceTableSettingsTarget>,
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
                    super::super::resource_table_settings::show_configurable_table_header(
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
pub(super) struct ManagedResourceRow {
    api_resource: crate::api_resource::ApiResource,
    name: String,
    namespace: Option<String>,
    uid: String,
    creation_timestamp: Option<time::OffsetDateTime>,
    cells: BTreeMap<String, crate::resource_table::CellValue>,
}

pub(super) fn managed_resource_rows(
    resources: &[ManagedResource],
    kind: &str,
) -> Vec<ManagedResourceRow> {
    let mut rows = resources
        .iter()
        .filter(|resource| resource.api_resource.kind == kind)
        .map(ManagedResourceRow::from)
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| left.name.cmp(&right.name));
    rows
}

pub(super) fn compare_managed_resource_column(
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

pub(super) fn managed_resource_table_definition(
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

pub(super) fn managed_resource_api_resource(kind: &str) -> crate::api_resource::ApiResource {
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

pub(super) fn managed_resource_table_kinds(
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
