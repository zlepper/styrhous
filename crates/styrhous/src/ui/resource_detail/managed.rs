use super::*;
use crate::ui::preference_table::{
    AGE_COLUMN_ID, ColumnControls, PreferenceTable, age_column, fill_first_column,
};
use crate::ui::resource_table_settings::ResourceTableSettingsTarget;
use crate::ui::table_preferences::PersistedResourceTablePreferences;

#[allow(clippy::too_many_arguments)]
pub(super) fn show_managed_resource_table(
    ui: &mut egui::Ui,
    detail_api_resource: &crate::api_resource::ApiResource,
    resource_uid: &str,
    kind: &str,
    rows: &[ManagedResourceRow],
    show_namespace_column: bool,
    pending_action: &mut Option<ResourceAction>,
    table_preferences: &mut PersistedResourceTablePreferences,
    column_settings: &mut Option<ResourceTableSettingsTarget>,
) {
    let theme = ui.tokens();
    let definition = managed_resource_table_definition(kind, show_namespace_column);
    let mut column_definitions = vec![TableColumnDefinition::sortable("name", "Name", 160.0)];
    if show_namespace_column {
        column_definitions.push(TableColumnDefinition::sortable(
            "namespace",
            "Namespace",
            150.0,
        ));
    }
    column_definitions.extend(
        definition
            .columns
            .iter()
            .map(|column| TableColumnDefinition {
                numeric: column.numeric,
                ..TableColumnDefinition::sortable(&column.id, &column.label, column.initial_width)
            }),
    );
    column_definitions.push(age_column());
    fill_first_column(&mut column_definitions, ui.available_width(), 0.0);
    let layout = PreferenceTable::new(
        table_preferences,
        ResourceTableKey::detail(detail_api_resource, &managed_resource_api_resource(kind)),
        column_definitions,
    );
    let sorted_rows = layout.sorted(rows, compare_managed_resource_column);
    let table = layout.table(format!("managed-resource-table-{resource_uid}-{kind}"));
    // The first click of a frame wins.
    let pending_action = RefCell::new(pending_action);
    let open = |row: &ManagedResourceRow| {
        pending_action
            .borrow_mut()
            .get_or_insert_with(|| row.details_action());
    };
    layout.show(
        ui,
        table,
        &sorted_rows,
        ColumnControls {
            preferences: table_preferences,
            metadata_key_suggestions: None,
            column_settings,
        },
        |ui, row, column_id| match column_id {
            "name" => {
                let response = TableRowBuilder::clickable_text(
                    ui,
                    &row.name,
                    theme.text.primary,
                    format!("Open details for {}", row.name),
                );
                if response.clicked() {
                    open(row);
                }
            }
            "namespace" => namespace_cell(ui, row.namespace.as_deref()),
            AGE_COLUMN_ID => age_cell(ui, row.creation_timestamp),
            id => show_resource_cell(ui, id, row.cells.get(id)),
        },
        |response, row, _| {
            if response.clicked() {
                open(row);
            }
        },
    );
}

pub(super) struct ManagedResourceRow {
    api_resource: crate::api_resource::ApiResource,
    name: String,
    namespace: Option<String>,
    uid: String,
    creation_timestamp: Option<time::OffsetDateTime>,
    cells: BTreeMap<String, crate::resource_table::CellValue>,
}

impl ManagedResourceRow {
    /// Open this resource's inspector.
    fn details_action(&self) -> ResourceAction {
        ResourceAction::NavigateDetails {
            api_resource: self.api_resource.clone(),
            name: self.name.clone(),
            namespace: self.namespace.clone(),
            uid: self.uid.clone(),
        }
    }
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
        AGE_COLUMN_ID => row
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
