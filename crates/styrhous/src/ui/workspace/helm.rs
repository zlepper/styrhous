use super::*;

pub(super) fn show_helm_releases_workspace(
    ui: &mut egui::Ui,
    cluster: &mut super::super::state::ClusterState,
    namespace_selection: &mut Option<NamespaceSelection>,
    table_preferences: &mut PersistedResourceTablePreferences,
    column_settings_to_open: &mut Option<
        super::super::resource_table_settings::ResourceTableSettingsTarget,
    >,
) -> Option<(String, String)> {
    let api_resource = crate::api_resource::ApiResource::helm_releases();
    let mut releases = cluster
        .selected_namespaces
        .iter()
        .filter_map(|namespace| cluster.helm_release_cache.get(namespace))
        .flat_map(|watch| watch.releases.iter().cloned())
        .collect::<Vec<_>>();
    releases.sort_by_key(|release| {
        (
            release.namespace.clone(),
            release.name.clone(),
            std::cmp::Reverse(release.revision),
        )
    });
    releases.dedup_by(|left, right| left.namespace == right.namespace && left.name == right.name);
    let resources = releases
        .iter()
        .map(|release| MinimalResource {
            uid: release.id(),
            name: release.name.clone(),
            namespace: Some(release.namespace.clone()),
            creation_timestamp: None,
            controller_owner: None,
            labels: std::collections::BTreeMap::from([("chart".into(), release.chart.clone())]),
            annotations: Default::default(),
            cells: Default::default(),
            log_containers: Vec::new(),
        })
        .collect::<Vec<_>>();
    let mut search = cluster
        .resource_searches
        .get(&api_resource)
        .cloned()
        .unwrap_or_default();
    let mut ignored_selection_action = None;
    let filtered = show_toolbar(
        ui,
        cluster,
        Some(&api_resource),
        &resources,
        &mut search,
        namespace_selection,
        ResourceSelectionControls {
            selected_count: 0,
            actions_enabled: false,
            action: &mut ignored_selection_action,
        },
    );
    cluster.resource_searches.insert(api_resource, search);
    ui.add_space(TOOLBAR_VERTICAL_PADDING);
    if cluster.selected_namespaces.is_empty() {
        workspace_empty_state(
            ui,
            "Choose a namespace",
            "Select one or more namespaces to inspect Helm releases.",
        );
        return None;
    }
    let selected_watches = cluster
        .selected_namespaces
        .iter()
        .filter_map(|namespace| cluster.helm_release_cache.get(namespace))
        .collect::<Vec<_>>();
    if selected_watches.len() != cluster.selected_namespaces.len()
        || selected_watches.iter().any(|watch| !watch.is_synced)
    {
        workspace_loading_state(
            ui,
            "Loading Helm releases",
            "Waiting for Helm release records to synchronize.",
        );
        return None;
    }
    let backend_errors = selected_watches
        .iter()
        .flat_map(|watch| watch.backend_errors.iter())
        .map(|(backend, error)| format!("{backend}: {error}"))
        .collect::<Vec<_>>();
    if !backend_errors.is_empty() {
        ui.label(
            egui::RichText::new(format!(
                "Some Helm storage backends could not be read: {}",
                backend_errors.join(" · ")
            ))
            .color(gray::_700),
        );
    }
    let selected_ids = filtered
        .resources
        .iter()
        .map(|resource| resource.uid.as_str())
        .collect::<HashSet<_>>();
    releases.retain(|release| selected_ids.contains(release.id().as_str()));
    if releases.is_empty() {
        workspace_empty_state(
            ui,
            "No Helm releases found",
            "No Helm release records were found in the selected namespace scope.",
        );
        return None;
    }
    show_helm_release_table(ui, &releases, table_preferences, column_settings_to_open)
}

pub(super) fn show_helm_release_table(
    ui: &mut egui::Ui,
    releases: &[HelmRelease],
    table_preferences: &mut PersistedResourceTablePreferences,
    column_settings_to_open: &mut Option<
        super::super::resource_table_settings::ResourceTableSettingsTarget,
    >,
) -> Option<(String, String)> {
    let columns = vec![
        TableColumnDefinition::sortable("name", "Name", 170.0),
        TableColumnDefinition::sortable("namespace", "Namespace", 135.0),
        TableColumnDefinition::sortable("chart", "Chart", 155.0),
        TableColumnDefinition::sortable("revision", "Revision", 82.0),
        TableColumnDefinition::sortable("version", "Version", 100.0),
        TableColumnDefinition::sortable("app-version", "App Version", 110.0),
        TableColumnDefinition::sortable("status", "Status", 105.0),
        TableColumnDefinition::sortable("updated", "Updated", 180.0),
    ];
    let table_key = ResourceTableKey::workspace(&ApiResource::helm_releases());
    let visible_columns = table_preferences.resolved_columns(&table_key, &columns);
    let sort_state = table_preferences
        .sort(&table_key, &columns)
        .map(|(column_id, direction)| components::SortState::new(column_id, direction));
    let mut sorted_releases = releases.to_vec();
    if let Some(sort) = &sort_state {
        sorted_releases.sort_by(|left, right| {
            compare_helm_release_column(left, right, &sort.column_id, sort.direction)
        });
    }
    let mut table = TailwindTable::new("helm-release-table");
    for column in &visible_columns {
        table = table.column(
            column.definition.id.clone(),
            column.definition.label.clone(),
            |builder| builder.initial_width(column.width).sortable(),
        );
    }
    let pending = RefCell::new(None);
    let table_preferences = RefCell::new(table_preferences);
    table
        .fill_available_height()
        .show_configurable_with_row_response(
            ui,
            &sorted_releases,
            sort_state.as_ref(),
            |header, id, _label, sortable| {
                MoreButton::show_context_menu(header, |menu| {
                    super::super::resource_table_settings::show_configurable_table_header(
                        menu,
                        sortable,
                        id,
                        &table_key,
                        &columns,
                        &table_preferences,
                        column_settings_to_open,
                    );
                });
            },
            |id, width| {
                table_preferences
                    .borrow_mut()
                    .set_width(&table_key, &columns, id, width);
            },
            |ui, release, column| match visible_columns[column].definition.id.as_str() {
                "name" => {
                    TableRowBuilder::clickable_text(
                        ui,
                        &release.name,
                        gray::_900,
                        format!("Inspect Helm release {}", release.name),
                    );
                }
                "namespace" => TableRowBuilder::text(ui, &release.namespace, false),
                "chart" => TableRowBuilder::text(ui, &release.chart, false),
                "revision" => {
                    let revision = release.revision.to_string();
                    TableRowBuilder::text(ui, &revision, false);
                }
                "version" => TableRowBuilder::text(ui, &release.chart_version, false),
                "app-version" => TableRowBuilder::text(ui, &release.app_version, false),
                "status" => TableRowBuilder::text(ui, &release.status, false),
                "updated" => TableRowBuilder::text(ui, &release.last_deployed, false),
                _ => unreachable!("Helm release table columns are defined locally"),
            },
            |response, release, _column| {
                if response.clicked() && pending.borrow().is_none() {
                    *pending.borrow_mut() = Some((release.name.clone(), release.namespace.clone()));
                }
            },
        );
    pending.into_inner()
}

pub(super) fn compare_helm_release_column(
    left: &HelmRelease,
    right: &HelmRelease,
    column_id: &str,
    direction: components::SortDirection,
) -> std::cmp::Ordering {
    let ordering = match column_id {
        "name" => left.name.cmp(&right.name),
        "namespace" => left.namespace.cmp(&right.namespace),
        "chart" => left.chart.cmp(&right.chart),
        "revision" => left.revision.cmp(&right.revision),
        "version" => left.chart_version.cmp(&right.chart_version),
        "app-version" => left.app_version.cmp(&right.app_version),
        "status" => left.status.cmp(&right.status),
        "updated" => left.last_deployed.cmp(&right.last_deployed),
        _ => std::cmp::Ordering::Equal,
    };
    let ordering = match direction {
        components::SortDirection::Ascending => ordering,
        components::SortDirection::Descending => ordering.reverse(),
    };
    ordering
        .then_with(|| left.namespace.cmp(&right.namespace))
        .then_with(|| left.name.cmp(&right.name))
        .then_with(|| left.revision.cmp(&right.revision))
}
