use super::*;
use crate::ui::preference_table::{ColumnControls, PreferenceTable};

/// Whether the release list is live: a namespace is chosen and the release
/// watch of every chosen namespace has synchronised.
pub(super) fn helm_list_is_live(cluster: &super::super::state::ClusterState) -> bool {
    let scope = cluster.namespace_scope();
    !scope.is_empty()
        && scope.iter().all(|namespace| {
            cluster
                .helm_release_cache
                .get(namespace)
                .is_some_and(|watch| watch.is_synced)
        })
}

pub(super) fn show_helm_releases_workspace(
    ui: &mut egui::Ui,
    cluster: &mut super::super::state::ClusterState,
    namespace_selection: &mut Option<NamespaceSelection>,
    table_preferences: &mut PersistedResourceTablePreferences,
    column_settings_to_open: &mut Option<
        super::super::resource_table_settings::ResourceTableSettingsTarget,
    >,
    namespace_selector_settings: &crate::ui::namespace_selector::NamespaceSelectorSettings,
) -> Option<(String, String)> {
    let theme = ui.tokens();
    let api_resource = crate::api_resource::ApiResource::helm_releases();
    let mut releases = cluster
        .namespace_scope()
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
    let filtered = filter_resources(&resources, &search);
    let live = helm_list_is_live(cluster);
    show_toolbar(
        ui,
        cluster,
        Some(&api_resource),
        ListSummary {
            total: resources.len(),
            visible: filtered.resources.len(),
            live,
        },
        &mut search,
        namespace_selection,
        namespace_selector_settings,
    );
    let filtered = filter_resources(&resources, &search);
    cluster.resource_searches.insert(api_resource, search);
    match cluster.awaited_namespace_list() {
        NamespaceListWait::Ready => {}
        NamespaceListWait::Loading => {
            workspace_loading_state(
                ui,
                "Loading namespaces",
                "Helm releases are listed per namespace once the namespaces are known.",
            );
            return None;
        }
        NamespaceListWait::Failed(error) => {
            workspace_unavailable_state(ui, "Namespaces unavailable", error);
            return None;
        }
    }
    let selected_namespaces = cluster.namespace_scope();
    if selected_namespaces.is_empty() {
        workspace_empty_state(
            ui,
            Icon::Folder,
            "Choose a namespace",
            "Select one or more namespaces to inspect Helm releases.",
        );
        return None;
    }
    let selected_watches = selected_namespaces
        .iter()
        .filter_map(|namespace| cluster.helm_release_cache.get(namespace))
        .collect::<Vec<_>>();
    if selected_watches.len() != selected_namespaces.len()
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
            .color(theme.text.secondary),
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
            Icon::Cube,
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
        TableColumnDefinition::sortable("revision", "Revision", 82.0).numeric(),
        TableColumnDefinition::sortable("version", "Version", 100.0),
        TableColumnDefinition::sortable("app-version", "App Version", 110.0),
        TableColumnDefinition::sortable("status", "Status", 105.0),
        TableColumnDefinition::sortable("updated", "Updated", 180.0),
    ];
    let layout = PreferenceTable::new(
        table_preferences,
        ResourceTableKey::workspace(&ApiResource::helm_releases()),
        columns,
    );
    let sorted_releases = layout.sorted(releases, compare_helm_release_column);
    let table = layout.table("helm-release-table").fill_available_height();
    let pending = RefCell::new(None);
    // The first click of a frame wins.
    let inspect = |release: &HelmRelease| {
        pending
            .borrow_mut()
            .get_or_insert_with(|| (release.name.clone(), release.namespace.clone()));
    };
    layout.show(
        ui,
        table,
        &sorted_releases,
        ColumnControls {
            preferences: table_preferences,
            metadata_key_suggestions: None,
            column_settings: column_settings_to_open,
        },
        |ui, release, column_id| match column_id {
            "name" => {
                let response = TableRowBuilder::clickable_name(
                    ui,
                    &release.name,
                    format!("Inspect Helm release {}", release.name),
                );
                if response.clicked() {
                    inspect(release);
                }
            }
            "namespace" => TableRowBuilder::text(ui, &release.namespace, false),
            "chart" => TableRowBuilder::text(ui, &release.chart, false),
            "revision" => TableRowBuilder::number(ui, &release.revision.to_string()),
            "version" => TableRowBuilder::text(ui, &release.chart_version, false),
            "app-version" => TableRowBuilder::text(ui, &release.app_version, false),
            "status" => {
                TableRowBuilder::problem_status(
                    ui,
                    &release.status,
                    crate::ui::helm_releases::helm_status_tone(&release.status),
                );
            }
            "updated" => TableRowBuilder::text(ui, &release.last_deployed, false),
            _ => unreachable!("Helm release table columns are defined locally"),
        },
        |response, release, _| {
            if response.clicked() {
                inspect(release);
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
