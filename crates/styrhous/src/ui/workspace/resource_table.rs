use super::*;
use crate::ui::preference_table::{
    ACTIONS_COLUMN_ID, AGE_COLUMN_ID, ColumnControls, PreferenceTable, actions_column, age_column,
    fill_first_column,
};
use components::design::size;
use components::tooltip::{TooltipExt, elided_label};

pub(super) struct ResourceTableConfiguration {
    definition: crate::resource_table::ResourceTableDefinition,
    pub(super) metadata_columns: Vec<super::super::table_preferences::CustomMetadataColumn>,
    /// The visible columns, their widths and the sort.
    pub(super) layout: PreferenceTable,
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
            180.0,
        ));
    }
    column_definitions.push(TableColumnDefinition::sortable("owner", "Owner", 160.0));
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
    fill_first_column(
        &mut column_definitions,
        available_width,
        size::TABLE_CHECKBOX_COLUMN,
    );

    ResourceTableConfiguration {
        definition,
        metadata_columns,
        layout: PreferenceTable::new(table_preferences, table_key, column_definitions),
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

pub(super) fn show_resource_table(
    ui: &mut egui::Ui,
    api_resource: &crate::api_resource::ApiResource,
    prepared: &PreparedResourceTable,
    configuration: &ResourceTableConfiguration,
    options: ResourceTableOptions<'_>,
    controls: ResourceTableControls<'_>,
) -> Option<ResourceAction> {
    let theme = ui.tokens();
    let pending_action = RefCell::new(None);
    let ResourceTableConfiguration {
        definition,
        metadata_columns,
        layout,
    } = configuration;
    let inspected_row = options.inspected_uid.and_then(|uid| {
        prepared.rows.iter().position(|row| {
            matches!(row, PreparedResourceTableRow::Resource(identity) if identity.uid == uid)
        })
    });
    let table = configuration
        .table(api_resource)
        .selectable()
        .fill_available_height()
        .inspected_row(inspected_row);

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
    layout.show_selectable(
        ui,
        table,
        &prepared.rows,
        controls.selection,
        |row| match row {
            PreparedResourceTableRow::Resource(identity) => Some(&identity.uid),
            PreparedResourceTableRow::HiddenBySearch(_) => None,
        },
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
                    MoreButton::show_context_menu(&response, |menu| {
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
                                theme.accent.text,
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
                            MoreButton::show_context_menu(&response, |menu| {
                                actions_menu(menu, resource);
                            });
                        } else {
                            TableRowBuilder::text(ui, node_name, false);
                        }
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
            let Some(resource) =
                resolve_prepared_resource(options.resource_cache, prepared, identity)
            else {
                return;
            };
            if column_id != ACTIONS_COLUMN_ID && row_response.clicked() {
                queue(ResourceAction::open_details(resource));
            }
            MoreButton::show_context_menu(row_response, |menu| {
                actions_menu(menu, resource);
            });
        },
    );
    pending_action.into_inner()
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
