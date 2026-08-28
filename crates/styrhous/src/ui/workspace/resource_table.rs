use super::*;

pub(super) fn show_resource_table(
    ui: &mut egui::Ui,
    api_resource: &crate::api_resource::ApiResource,
    resources: &[MinimalResource],
    options: ResourceTableOptions<'_>,
    selection: &mut HashSet<String>,
    table_preferences: &mut PersistedResourceTablePreferences,
    column_settings_to_open: &mut Option<
        super::super::resource_table_settings::ResourceTableSettingsTarget,
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
            compare_resource_column_with_relevance(
                left,
                right,
                &sort.column_id,
                sort.direction,
                &metadata_columns,
                options.fuzzy_scores,
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
                        super::super::resource_table_settings::target_with_metadata_key_suggestions(
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

pub(super) fn show_resource_actions(
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

#[cfg(test)]
pub(super) fn compare_resource_column(
    left: &MinimalResource,
    right: &MinimalResource,
    column_id: &str,
    direction: components::SortDirection,
    metadata_columns: &[super::super::table_preferences::CustomMetadataColumn],
) -> std::cmp::Ordering {
    compare_resource_column_with_relevance(
        left,
        right,
        column_id,
        direction,
        metadata_columns,
        None,
    )
}

pub(super) fn compare_resource_column_with_relevance(
    left: &MinimalResource,
    right: &MinimalResource,
    column_id: &str,
    direction: components::SortDirection,
    metadata_columns: &[super::super::table_preferences::CustomMetadataColumn],
    fuzzy_scores: Option<&std::collections::HashMap<String, components::fuzzy::FuzzyMatchScore>>,
) -> std::cmp::Ordering {
    compare_resource_column_values(left, right, column_id, direction, metadata_columns)
        .then_with(|| {
            let left_score = fuzzy_scores.and_then(|scores| scores.get(&left.uid));
            let right_score = fuzzy_scores.and_then(|scores| scores.get(&right.uid));
            match (left_score, right_score) {
                (Some(left_score), Some(right_score)) => right_score.cmp(left_score),
                _ => std::cmp::Ordering::Equal,
            }
        })
        .then_with(|| left.name.cmp(&right.name))
        .then_with(|| left.uid.cmp(&right.uid))
}

fn compare_resource_column_values(
    left: &MinimalResource,
    right: &MinimalResource,
    column_id: &str,
    direction: components::SortDirection,
    metadata_columns: &[super::super::table_preferences::CustomMetadataColumn],
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
    compare_sort_values(left_value, right_value, direction)
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

pub(super) fn show_metadata_cell(ui: &mut egui::Ui, value: &str) {
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

pub(super) fn metadata_key_suggestions(
    resources: &[MinimalResource],
) -> super::super::resource_table_settings::MetadataKeySuggestions {
    super::super::resource_table_settings::MetadataKeySuggestions {
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
