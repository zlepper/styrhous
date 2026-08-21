use super::super::global_blade::GlobalBladeRenderContext;
use super::super::table_preferences::{
    PersistedResourceTablePreferences, ResourceTableKey, TableColumnDefinition,
};
use super::manifest::ManifestInventoryRow;
use super::{ManifestNavigation, ManifestResource, manifest_resource_namespace};
use crate::api_resource::ApiResource;
use crate::helm_release::HelmRelease;
use crate::resource_detail::ResourceOwner;
use components::colors::{gray, indigo};
use components::design::spacing;
use components::{
    DetailCell, DetailRow, DetailTone, InspectorDetails, MoreButton, TableRowBuilder,
    TailwindTable, WorkspaceCard,
};
use std::cell::RefCell;

pub(super) fn show_revision_history(
    ui: &mut egui::Ui,
    releases: &[HelmRelease],
    selected_revision: i64,
    release_id: String,
    table_preferences: &mut PersistedResourceTablePreferences,
    column_settings: &mut Option<
        super::super::resource_table_settings::ResourceTableSettingsTarget,
    >,
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

pub(super) fn show_manifest_resources(
    ui: &mut egui::Ui,
    resources: &[ManifestResource],
    release_id: String,
    pending_navigation: &mut Option<ManifestNavigation>,
    cluster_key: i32,
    context: &mut GlobalBladeRenderContext<'_>,
    column_settings: &mut Option<
        super::super::resource_table_settings::ResourceTableSettingsTarget,
    >,
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

pub(super) fn show_values_disclosure(ui: &mut egui::Ui, values: &str) {
    super::super::resource_detail::disclosure_card(
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

pub(super) fn show_release_notes(ui: &mut egui::Ui, notes: &str) {
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

pub(super) fn show_storage_metadata(ui: &mut egui::Ui, release: &HelmRelease) {
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

pub(super) fn helm_status_tone(status: &str) -> DetailTone {
    match status {
        "deployed" => DetailTone::Success,
        "pending-install" | "pending-upgrade" | "pending-rollback" | "uninstalling" => {
            DetailTone::Warning
        }
        "failed" | "uninstalled" => DetailTone::Danger,
        _ => DetailTone::Neutral,
    }
}

pub(super) fn display_or_dash(value: &str) -> &str {
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
