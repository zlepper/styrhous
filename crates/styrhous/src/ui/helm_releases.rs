use super::global_blade::{
    GlobalBladeContent, GlobalBladeEffect, GlobalBladeEffectContext, GlobalBladeNavigation,
    GlobalBladeRenderContext, GlobalBladeRenderResult,
};
use super::table_preferences::{
    PersistedResourceTablePreferences, ResourceTableKey, TableColumnDefinition,
};
use crate::api_resource::ApiResource;
use crate::helm_release::HelmRelease;
use crate::resource_detail::ResourceOwner;
use components::colors::{gray, indigo};
use components::design::{radius, spacing, typography};
use components::{
    BladeLayer, DetailCell, DetailRow, DetailTone, InspectorDetails, MoreButton, TableRowBuilder,
    TailwindTable, WorkspaceCard,
};
use serde::Deserialize;
use std::cell::RefCell;

#[derive(Debug)]
pub(super) struct HelmReleaseDetailBlade {
    cluster_key: i32,
    release_name: String,
    namespace: String,
    selected_revision: i64,
    pending_navigation: Option<ManifestNavigation>,
}

#[derive(Debug, Clone)]
struct ManifestNavigation {
    api_resource: ApiResource,
    name: String,
    namespace: Option<String>,
    uid: String,
}

impl HelmReleaseDetailBlade {
    pub(super) fn new(cluster_key: i32, release_name: String, namespace: String) -> Self {
        Self {
            cluster_key,
            release_name,
            namespace,
            selected_revision: 0,
            pending_navigation: None,
        }
    }

    fn selected<'a>(&self, releases: &'a [HelmRelease]) -> Option<&'a HelmRelease> {
        releases
            .iter()
            .find(|release| release.revision == self.selected_revision)
            .or_else(|| releases.iter().max_by_key(|release| release.revision))
    }
}

impl GlobalBladeContent for HelmReleaseDetailBlade {
    fn render_header(
        &mut self,
        ui: &mut egui::Ui,
        _layer: BladeLayer,
        context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult {
        let releases = context.helm_releases(self.cluster_key, &self.namespace, &self.release_name);
        let title = self
            .selected(&releases)
            .map(|release| release.name.as_str())
            .unwrap_or(&self.release_name);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            egui::Frame::new()
                .fill(indigo::_50)
                .corner_radius(radius::control())
                .inner_margin(egui::Margin::symmetric(
                    spacing::SM as i8,
                    spacing::XS as i8,
                ))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("Helm Release")
                            .font(typography::body())
                            .color(indigo::_600),
                    );
                });
            ui.add_space(spacing::MD);
            let title = egui::RichText::new(title)
                .font(typography::page_title())
                .color(gray::_900);
            ui.add_sized(
                egui::vec2(ui.available_width().max(0.0), 24.0),
                egui::Label::new(title)
                    .truncate()
                    .halign(egui::Align::RIGHT),
            );
        });
        GlobalBladeRenderResult::default()
    }

    fn render_body(
        &mut self,
        ui: &mut egui::Ui,
        _layer: BladeLayer,
        context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult {
        let mut releases =
            context.helm_releases(self.cluster_key, &self.namespace, &self.release_name);
        releases.sort_by_key(|release| std::cmp::Reverse(release.revision));
        let Some(release) = self.selected(&releases).cloned() else {
            ui.label(
                egui::RichText::new("This Helm release is no longer available.").color(gray::_500),
            );
            return GlobalBladeRenderResult::default();
        };
        WorkspaceCard::new()
            .padding(spacing::LG as i8)
            .show(ui, |ui| {
                InspectorDetails::show_properties(
                    ui,
                    &[
                        DetailRow::new([
                            DetailCell::new("Release", release.name.as_str()).copyable(),
                            DetailCell::new("Namespace", release.namespace.as_str()).copyable(),
                            DetailCell::status(
                                "Status",
                                release.status.as_str(),
                                helm_status_tone(&release.status),
                            ),
                        ]),
                        DetailRow::new([
                            DetailCell::new("Chart", release.chart.as_str()).copyable(),
                            DetailCell::new("Chart version", release.chart_version.as_str())
                                .copyable(),
                            DetailCell::new("App version", release.app_version.as_str()).copyable(),
                        ]),
                        DetailRow::new([
                            DetailCell::new("First deployed", release.first_deployed.as_str())
                                .copyable(),
                            DetailCell::new("Last deployed", release.last_deployed.as_str())
                                .copyable(),
                            DetailCell::new("Description", display_or_dash(&release.description))
                                .copyable(),
                        ]),
                        DetailRow::new([
                            DetailCell::new("Storage driver", release.storage.to_string())
                                .copyable(),
                            DetailCell::new("Storage record", release.storage_name.as_str())
                                .copyable(),
                        ]),
                    ],
                );
            });
        ui.add_space(spacing::MD);
        let mut column_settings = None;
        if let Some(revision) = show_revision_history(
            ui,
            &releases,
            release.revision,
            release.id(),
            context.table_preferences(),
            &mut column_settings,
        ) {
            self.selected_revision = revision;
        }
        ui.add_space(spacing::MD);
        show_release_notes(ui, &release.notes);
        ui.add_space(spacing::MD);
        let values = release.values_yaml();
        show_values_disclosure(ui, &values);
        ui.add_space(spacing::MD);
        let resources = manifest_resources(&release.manifest, &release.namespace);
        show_manifest_resources(
            ui,
            &resources,
            release.id(),
            &mut self.pending_navigation,
            self.cluster_key,
            context,
            &mut column_settings,
        );
        ui.add_space(spacing::MD);
        show_storage_metadata(ui, &release);
        GlobalBladeRenderResult {
            next_content: column_settings
                .map(|target| Box::new(target) as Box<dyn GlobalBladeContent>),
            ..Default::default()
        }
    }

    fn take_effect(&mut self) -> Option<Box<dyn GlobalBladeEffect>> {
        self.pending_navigation.take().map(|navigation| {
            Box::new(HelmReleaseDetailEffect {
                cluster_key: self.cluster_key,
                navigation,
            }) as Box<dyn GlobalBladeEffect>
        })
    }
}

#[derive(Debug)]
struct HelmReleaseDetailEffect {
    cluster_key: i32,
    navigation: ManifestNavigation,
}

impl GlobalBladeEffect for HelmReleaseDetailEffect {
    fn apply(
        self: Box<Self>,
        context: &mut GlobalBladeEffectContext<'_>,
        _navigation: &mut GlobalBladeNavigation<'_>,
    ) {
        context.ui_state.open_resource_detail(
            self.cluster_key,
            self.navigation.api_resource.clone(),
            self.navigation.name.clone(),
            self.navigation.namespace.clone(),
            self.navigation.uid.clone(),
            _navigation.commands_to_send(),
        );
    }
}

fn show_revision_history(
    ui: &mut egui::Ui,
    releases: &[HelmRelease],
    selected_revision: i64,
    release_id: String,
    table_preferences: &mut PersistedResourceTablePreferences,
    column_settings: &mut Option<super::resource_table_settings::ResourceTableSettingsTarget>,
) -> Option<i64> {
    ui.horizontal(|ui| {
        ui.heading("Revision history");
        ui.label(
            egui::RichText::new(format!("Revision {selected_revision} selected")).color(gray::_600),
        );
    });
    let column_definitions = vec![
        TableColumnDefinition::sortable("revision", "Revision", 110.0),
        TableColumnDefinition::sortable("status", "Status", 135.0),
        TableColumnDefinition::sortable("deployed", "Deployed", 195.0),
        TableColumnDefinition::sortable("description", "Description", 260.0),
    ];
    let table_key = helm_detail_table_key("helm-release-revisions");
    let visible_columns = table_preferences.resolved_columns(&table_key, &column_definitions);
    let sort_state = table_preferences
        .sort(&table_key, &column_definitions)
        .map(|(column_id, direction)| components::SortState::new(column_id, direction));
    let mut sorted_releases = releases.to_vec();
    if let Some(sort) = &sort_state {
        sorted_releases.sort_by(|left, right| {
            compare_revision_column(left, right, &sort.column_id, sort.direction)
        });
    }
    let mut table = TailwindTable::new(("helm-release-revisions", release_id)).roomy();
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
    let selected = RefCell::new(None);
    let table_preferences = RefCell::new(table_preferences);
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), 230.0),
        egui::Layout::top_down(egui::Align::Min),
        |table_ui| {
            table.show_configurable_with_row_response(
                table_ui,
                &sorted_releases,
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
                    table_preferences.borrow_mut().set_width(
                        &table_key,
                        &column_definitions,
                        id,
                        width,
                    );
                },
                |ui, release, column| match visible_columns[column].definition.id.as_str() {
                    "revision" => {
                        let label = if release.revision == selected_revision {
                            format!("{} (selected)", release.revision)
                        } else {
                            release.revision.to_string()
                        };
                        let response = TableRowBuilder::clickable_text(
                            ui,
                            &label,
                            indigo::_600,
                            format!("Select revision {}", release.revision),
                        );
                        response.widget_info(|| {
                            egui::WidgetInfo::selected(
                                egui::WidgetType::Button,
                                ui.is_enabled(),
                                release.revision == selected_revision,
                                format!("Select revision {}", release.revision),
                            )
                        });
                        if response.clicked() {
                            selected.replace(Some(release.revision));
                        }
                    }
                    "status" => show_status_cell(ui, &release.status),
                    "deployed" => {
                        TableRowBuilder::text(ui, display_or_dash(&release.last_deployed), false)
                    }
                    "description" => {
                        TableRowBuilder::text(ui, display_or_dash(&release.description), false)
                    }
                    _ => unreachable!("revision table columns are defined locally"),
                },
                |response, release, _column| {
                    if response.clicked() {
                        selected.replace(Some(release.revision));
                    }
                },
            )
        },
    );
    selected.into_inner()
}

fn show_manifest_resources(
    ui: &mut egui::Ui,
    resources: &[ManifestResource],
    release_id: String,
    pending_navigation: &mut Option<ManifestNavigation>,
    cluster_key: i32,
    context: &mut GlobalBladeRenderContext<'_>,
    column_settings: &mut Option<super::resource_table_settings::ResourceTableSettingsTarget>,
) {
    ui.horizontal(|ui| {
        ui.heading("Manifest resources");
        ui.label(
            egui::RichText::new(match resources.len() {
                1 => "1 resource".to_owned(),
                count => format!("{count} resources"),
            })
            .color(gray::_600),
        );
    });
    if resources.is_empty() {
        ui.label(
            egui::RichText::new("No Kubernetes objects were found in this revision's manifest.")
                .color(gray::_500),
        );
        return;
    }
    let navigation = context.resource_navigation(cluster_key);
    let rows = resources
        .iter()
        .map(|resource| {
            let owner = ResourceOwner {
                api_version: resource.api_version.clone(),
                kind: resource.kind.clone(),
                name: resource.name.clone(),
                uid: resource.name.clone(),
                controller: false,
            };
            let api_resource = navigation.api_resource_for_owner(&owner);
            let namespace = manifest_resource_namespace(resource, api_resource.as_ref());
            let uid = api_resource.as_ref().and_then(|api_resource| {
                context.cached_resource_uid(
                    cluster_key,
                    api_resource,
                    namespace.as_deref(),
                    &resource.name,
                )
            });
            ManifestInventoryRow {
                namespace,
                resource,
                api_resource,
                uid,
            }
        })
        .collect::<Vec<_>>();
    let column_definitions = vec![
        TableColumnDefinition::sortable("kind", "Kind", 150.0),
        TableColumnDefinition::sortable("name", "Name", 220.0),
        TableColumnDefinition::sortable("namespace", "Namespace", 170.0),
        TableColumnDefinition::sortable("api-version", "API version", 130.0),
    ];
    let table_key = helm_detail_table_key("helm-manifest-resources");
    let table_preferences = context.table_preferences();
    let visible_columns = table_preferences.resolved_columns(&table_key, &column_definitions);
    let sort_state = table_preferences
        .sort(&table_key, &column_definitions)
        .map(|(column_id, direction)| components::SortState::new(column_id, direction));
    let mut sorted_rows = rows;
    if let Some(sort) = &sort_state {
        sorted_rows.sort_by(|left, right| {
            compare_manifest_resource_column(left, right, &sort.column_id, sort.direction)
        });
    }
    let mut table = TailwindTable::new(("helm-manifest-resources", release_id)).roomy();
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
    let selected = RefCell::new(None);
    let table_preferences = RefCell::new(table_preferences);
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), 230.0),
        egui::Layout::top_down(egui::Align::Min),
        |table_ui| {
            table.show_configurable_with_row_response(
                table_ui,
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
                    table_preferences.borrow_mut().set_width(
                        &table_key,
                        &column_definitions,
                        id,
                        width,
                    );
                },
                |ui, row, column| match visible_columns[column].definition.id.as_str() {
                    "kind" => TableRowBuilder::text(ui, &row.resource.kind, true),
                    "name" => {
                        if row.api_resource.is_some()
                            && row.uid.is_some()
                            && TableRowBuilder::clickable_text(
                                ui,
                                &row.resource.name,
                                indigo::_600,
                                format!("Open details for {}", row.resource.name),
                            )
                            .clicked()
                        {
                            selected.replace(Some(row));
                        } else {
                            TableRowBuilder::text(ui, &row.resource.name, false);
                        }
                    }
                    "namespace" => TableRowBuilder::text(
                        ui,
                        row.namespace.as_deref().unwrap_or("Cluster-wide"),
                        false,
                    ),
                    "api-version" => TableRowBuilder::text(ui, &row.resource.api_version, false),
                    _ => unreachable!("manifest inventory columns are defined locally"),
                },
                |response, row, _column| {
                    if response.clicked() && row.api_resource.is_some() && row.uid.is_some() {
                        selected.replace(Some(row));
                    }
                },
            )
        },
    );
    if let Some(row) = selected.into_inner()
        && let Some(api_resource) = row.api_resource.clone()
        && let Some(uid) = row.uid.clone()
    {
        pending_navigation.replace(ManifestNavigation {
            namespace: api_resource
                .namespaced
                .then_some(row.namespace.clone())
                .flatten(),
            api_resource,
            name: row.resource.name.clone(),
            uid,
        });
    }
}

fn show_values_disclosure(ui: &mut egui::Ui, values: &str) {
    super::resource_detail::disclosure_card(
        ui,
        "helm-release-values-open",
        "Values (sensitive values may be present)",
        false,
        |ui| {
            ui.label(
                egui::RichText::new("Treat this content like a Kubernetes Secret.")
                    .color(gray::_700),
            );
            ui.add_space(spacing::SM);
            show_read_only_code(ui, values, 8, "Helm release values");
        },
    );
}

fn show_release_notes(ui: &mut egui::Ui, notes: &str) {
    WorkspaceCard::new().show(ui, |ui| {
        ui.heading("Release notes");
        if notes.is_empty() {
            ui.label(
                egui::RichText::new("This chart did not provide release notes.").color(gray::_500),
            );
        } else {
            ui.label(
                egui::RichText::new(
                    "These are the notes Helm printed after deploying this revision.",
                )
                .color(gray::_700),
            );
            ui.add_space(spacing::SM);
            show_read_only_code(ui, notes, 6, "Helm release notes");
        }
    });
}

fn show_read_only_code(ui: &mut egui::Ui, content: &str, desired_rows: usize, label: &str) {
    let mut content = content.to_owned();
    let response = ui.add(
        egui::TextEdit::multiline(&mut content)
            .code_editor()
            .interactive(false)
            .desired_width(f32::INFINITY)
            .desired_rows(desired_rows),
    );
    ui.ctx().accesskit_node_builder(response.id, |builder| {
        builder.set_label(label);
    });
}

fn show_storage_metadata(ui: &mut egui::Ui, release: &HelmRelease) {
    if release.storage_labels.is_empty() && release.storage_annotations.is_empty() {
        return;
    }
    WorkspaceCard::new().show(ui, |ui| {
        ui.heading("Storage metadata");
        if !release.storage_labels.is_empty() {
            ui.label(egui::RichText::new("Labels").strong().color(gray::_800));
            InspectorDetails::show_properties(
                ui,
                &[DetailRow::new(release.storage_labels.iter().map(
                    |(key, value)| {
                        DetailCell::new(key.as_str(), value.as_str())
                            .copyable_as(format!("{key}={value}"))
                    },
                ))],
            );
        }
        if !release.storage_annotations.is_empty() {
            if !release.storage_labels.is_empty() {
                ui.add_space(spacing::SM);
            }
            ui.label(
                egui::RichText::new("Annotations")
                    .strong()
                    .color(gray::_800),
            );
            InspectorDetails::show_properties(
                ui,
                &[DetailRow::new(release.storage_annotations.iter().map(
                    |(key, value)| {
                        DetailCell::new(key.as_str(), value.as_str())
                            .copyable_as(format!("{key}={value}"))
                    },
                ))],
            );
        }
    });
}

fn helm_detail_table_key(table_name: &str) -> ResourceTableKey {
    let helm_release = ApiResource::helm_releases();
    let table_resource = ApiResource {
        group: crate::helm_release::GROUP.to_owned(),
        version: crate::helm_release::VERSION.to_owned(),
        kind: "HelmReleaseInspectorTable".to_owned(),
        name: table_name.to_owned(),
        namespaced: false,
    };
    ResourceTableKey::detail(&helm_release, &table_resource)
}

fn compare_revision_column(
    left: &HelmRelease,
    right: &HelmRelease,
    column_id: &str,
    direction: components::SortDirection,
) -> std::cmp::Ordering {
    let ordering = match column_id {
        "revision" => left.revision.cmp(&right.revision),
        "status" => left.status.cmp(&right.status),
        "deployed" => left.last_deployed.cmp(&right.last_deployed),
        "description" => left.description.cmp(&right.description),
        _ => std::cmp::Ordering::Equal,
    };
    apply_sort_direction(ordering, direction).then_with(|| left.revision.cmp(&right.revision))
}

fn compare_manifest_resource_column(
    left: &ManifestInventoryRow<'_>,
    right: &ManifestInventoryRow<'_>,
    column_id: &str,
    direction: components::SortDirection,
) -> std::cmp::Ordering {
    let ordering = match column_id {
        "kind" => left.resource.kind.cmp(&right.resource.kind),
        "name" => left.resource.name.cmp(&right.resource.name),
        "namespace" => left.namespace.cmp(&right.namespace),
        "api-version" => left.resource.api_version.cmp(&right.resource.api_version),
        _ => std::cmp::Ordering::Equal,
    };
    apply_sort_direction(ordering, direction)
        .then_with(|| left.resource.kind.cmp(&right.resource.kind))
        .then_with(|| left.resource.name.cmp(&right.resource.name))
}

fn apply_sort_direction(
    ordering: std::cmp::Ordering,
    direction: components::SortDirection,
) -> std::cmp::Ordering {
    match direction {
        components::SortDirection::Ascending => ordering,
        components::SortDirection::Descending => ordering.reverse(),
    }
}

fn helm_status_tone(status: &str) -> DetailTone {
    match status {
        "deployed" => DetailTone::Success,
        "pending-install" | "pending-upgrade" | "pending-rollback" | "uninstalling" => {
            DetailTone::Warning
        }
        "failed" | "uninstalled" => DetailTone::Danger,
        _ => DetailTone::Neutral,
    }
}

fn display_or_dash(value: &str) -> &str {
    if value.is_empty() { "-" } else { value }
}

fn show_status_cell(ui: &mut egui::Ui, status: &str) {
    let color = match helm_status_tone(status) {
        DetailTone::Success => components::design::status::SUCCESS,
        DetailTone::Warning => components::design::status::WARNING,
        DetailTone::Danger => components::design::status::DANGER,
        DetailTone::Neutral => gray::_400,
    };
    ui.horizontal(|ui| {
        let (marker, _) = ui.allocate_exact_size(egui::vec2(12.0, 16.0), egui::Sense::hover());
        ui.painter().circle_filled(marker.center(), 4.0, color);
        TableRowBuilder::text(ui, status, false);
    });
}

#[derive(Debug)]
struct ManifestResource {
    api_version: String,
    kind: String,
    name: String,
    namespace: Option<String>,
}

#[derive(Clone)]
struct ManifestInventoryRow<'a> {
    resource: &'a ManifestResource,
    api_resource: Option<ApiResource>,
    namespace: Option<String>,
    uid: Option<String>,
}

fn manifest_resource_namespace(
    resource: &ManifestResource,
    api_resource: Option<&ApiResource>,
) -> Option<String> {
    api_resource
        .is_none_or(|api_resource| api_resource.namespaced)
        .then(|| resource.namespace.clone())
        .flatten()
}

fn manifest_resources(manifest: &str, release_namespace: &str) -> Vec<ManifestResource> {
    serde_yaml::Deserializer::from_str(manifest)
        .filter_map(|document| serde_yaml::Value::deserialize(document).ok())
        .filter_map(|document| {
            let api_version = document.get("apiVersion")?.as_str()?.to_owned();
            let kind = document.get("kind")?.as_str()?.to_owned();
            let metadata = document.get("metadata")?;
            let name = metadata.get("name")?.as_str()?.to_owned();
            let namespace = metadata
                .get("namespace")
                .and_then(serde_yaml::Value::as_str)
                .map(ToOwned::to_owned)
                .or_else(|| Some(release_namespace.to_owned()));
            Some(ManifestResource {
                api_version,
                kind,
                name,
                namespace,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{ManifestResource, manifest_resource_namespace, manifest_resources};
    use crate::api_resource::ApiResource;

    #[test]
    fn manifest_inventory_defaults_a_namespaced_object_to_the_release_namespace() {
        let resources = manifest_resources(
            "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: settings\n---\napiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: api\n  namespace: workloads\n",
            "apps",
        );

        assert_eq!(resources.len(), 2);
        assert_eq!(resources[0].namespace.as_deref(), Some("apps"));
        assert_eq!(resources[1].namespace.as_deref(), Some("workloads"));
    }

    #[test]
    fn manifest_inventory_marks_cluster_scoped_resources_as_cluster_wide() {
        let resource = ManifestResource {
            api_version: "rbac.authorization.k8s.io/v1".into(),
            kind: "ClusterRole".into(),
            name: "readers".into(),
            namespace: Some("apps".into()),
        };
        let api_resource = ApiResource {
            group: "rbac.authorization.k8s.io".into(),
            version: "v1".into(),
            kind: "ClusterRole".into(),
            name: "clusterroles".into(),
            namespaced: false,
        };

        assert_eq!(
            manifest_resource_namespace(&resource, Some(&api_resource)),
            None
        );
    }
}
