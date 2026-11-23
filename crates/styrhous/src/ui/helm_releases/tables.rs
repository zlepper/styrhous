use super::super::global_blade::GlobalBladeRenderContext;
use super::super::table_preferences::{
    PersistedResourceTablePreferences, ResourceTableKey, TableColumnDefinition,
};
use super::manifest::ManifestInventoryRow;
use super::{ManifestNavigation, ManifestResource, manifest_resource_namespace};
use crate::api_resource::ApiResource;
use crate::helm_release::HelmRelease;
use crate::resource_detail::ResourceOwner;
use crate::ui::preference_table::{ColumnControls, PreferenceTable};
use components::design::space;
use components::design::theme::{ThemeExt, Tone};
use components::{Button, ButtonSize, Card, TableRowBuilder};
use std::cell::RefCell;

/// The height of the release inspector's tables.
const HELM_TABLE_HEIGHT: f32 = 230.0;

/// Whether the selected revision can be rolled back to from the history header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RevisionRollback {
    /// The selected revision is the release's current one.
    Unavailable,
    Available,
    /// A rollback of this release is still running.
    InProgress,
}

#[derive(Debug)]
pub(super) struct RevisionHistoryResponse {
    pub(super) selected_revision: Option<i64>,
    pub(super) rollback_requested: bool,
}

pub(super) fn show_revision_history(
    ui: &mut egui::Ui,
    releases: &[HelmRelease],
    selected_revision: i64,
    rollback: RevisionRollback,
    release_id: String,
    table_preferences: &mut PersistedResourceTablePreferences,
    column_settings: &mut Option<
        super::super::resource_table_settings::ResourceTableSettingsTarget,
    >,
) -> RevisionHistoryResponse {
    let theme = ui.tokens();
    let mut rollback_requested = false;
    // Reserve the button's height whether or not it is shown, so the table does
    // not jump when another revision is selected, and centre the row on it.
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), ButtonSize::Sm.height()),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.heading("Revision history");
            ui.label(
                egui::RichText::new(format!("Revision {selected_revision} selected"))
                    .color(theme.text.secondary),
            );
            if rollback != RevisionRollback::Unavailable {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let in_progress = rollback == RevisionRollback::InProgress;
                    let label = if in_progress {
                        "Rolling back…".into()
                    } else {
                        format!("Roll back to revision {selected_revision}")
                    };
                    rollback_requested = Button::secondary(label)
                        .size(ButtonSize::Sm)
                        .enabled(!in_progress)
                        .show(ui)
                        .clicked();
                });
            }
        },
    );
    let column_definitions = vec![
        TableColumnDefinition::sortable("revision", "Revision", 110.0),
        TableColumnDefinition::sortable("status", "Status", 135.0),
        TableColumnDefinition::sortable("deployed", "Deployed", 195.0),
        TableColumnDefinition::sortable("description", "Description", 260.0),
    ];
    let layout = PreferenceTable::new(
        table_preferences,
        helm_detail_table_key("helm-release-revisions"),
        column_definitions,
    );
    let sorted_releases = layout.sorted(releases, compare_revision_column);
    let table = layout.table(("helm-release-revisions", release_id));
    let selected = RefCell::new(None);
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), HELM_TABLE_HEIGHT),
        egui::Layout::top_down(egui::Align::Min),
        |table_ui| {
            layout.show(
                table_ui,
                table,
                &sorted_releases,
                ColumnControls {
                    preferences: table_preferences,
                    metadata_key_suggestions: None,
                    column_settings,
                },
                |ui, release, column_id| match column_id {
                    "revision" => {
                        let label = if release.revision == selected_revision {
                            format!("{} (selected)", release.revision)
                        } else {
                            release.revision.to_string()
                        };
                        let response = TableRowBuilder::clickable_text(
                            ui,
                            &label,
                            theme.accent.text,
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
                    "status" => {
                        TableRowBuilder::problem_status(
                            ui,
                            &release.status,
                            helm_status_tone(&release.status),
                        );
                    }
                    "deployed" => TableRowBuilder::text_or_missing(
                        ui,
                        Some(&release.last_deployed),
                        "Helm recorded no deployment time",
                    ),
                    "description" => TableRowBuilder::text_or_missing(
                        ui,
                        Some(&release.description),
                        "Helm recorded no description",
                    ),
                    _ => unreachable!("revision table columns are defined locally"),
                },
                |response, release, _| {
                    if response.clicked() {
                        selected.replace(Some(release.revision));
                    }
                },
            );
        },
    );
    RevisionHistoryResponse {
        selected_revision: selected.into_inner(),
        rollback_requested,
    }
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
    let theme = ui.tokens();
    ui.horizontal(|ui| {
        ui.heading("Manifest resources");
        ui.label(
            egui::RichText::new(match resources.len() {
                1 => "1 resource".to_owned(),
                count => format!("{count} resources"),
            })
            .color(theme.text.secondary),
        );
    });
    if resources.is_empty() {
        ui.label(
            egui::RichText::new("No Kubernetes objects were found in this revision's manifest.")
                .color(theme.text.muted),
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
    let table_preferences = context.table_preferences();
    let layout = PreferenceTable::new(
        table_preferences,
        helm_detail_table_key("helm-manifest-resources"),
        column_definitions,
    );
    let sorted_rows = layout.sorted(&rows, compare_manifest_resource_column);
    let table = layout.table(("helm-manifest-resources", release_id));
    let selected = RefCell::new(None);
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), HELM_TABLE_HEIGHT),
        egui::Layout::top_down(egui::Align::Min),
        |table_ui| {
            layout.show(
                table_ui,
                table,
                &sorted_rows,
                ColumnControls {
                    preferences: table_preferences,
                    metadata_key_suggestions: None,
                    column_settings,
                },
                |ui, row, column_id| match column_id {
                    "kind" => TableRowBuilder::text(ui, &row.resource.kind, true),
                    "name" if row.is_navigable() => {
                        let response = TableRowBuilder::clickable_text(
                            ui,
                            &row.resource.name,
                            theme.accent.text,
                            format!("Open details for {}", row.resource.name),
                        );
                        if response.clicked() {
                            selected.replace(Some(*row));
                        }
                    }
                    "name" => TableRowBuilder::text(ui, &row.resource.name, false),
                    "namespace" => TableRowBuilder::text(
                        ui,
                        row.namespace.as_deref().unwrap_or("Cluster-wide"),
                        false,
                    ),
                    "api-version" => TableRowBuilder::text(ui, &row.resource.api_version, false),
                    _ => unreachable!("manifest inventory columns are defined locally"),
                },
                |response, row, _| {
                    if response.clicked() && row.is_navigable() {
                        selected.replace(Some(*row));
                    }
                },
            );
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
    let theme = ui.tokens();
    // Values may be secret: open only for this session, never on restart.
    components::Disclosure::new(
        "helm-release-values-open",
        "Values (sensitive values may be present)",
    )
    .session_only()
    .show(ui, |ui| {
        ui.label(
            egui::RichText::new("Treat this content like a Kubernetes Secret.")
                .color(theme.text.secondary),
        );
        ui.add_space(space::S8);
        show_read_only_code(ui, values, 8, "Helm release values");
    });
}

pub(super) fn show_release_notes(ui: &mut egui::Ui, notes: &str) {
    let theme = ui.tokens();
    Card::new().show(ui, |ui| {
        ui.heading("Release notes");
        if notes.is_empty() {
            ui.label(
                egui::RichText::new("This chart did not provide release notes.")
                    .color(theme.text.muted),
            );
        } else {
            ui.label(
                egui::RichText::new(
                    "These are the notes Helm printed after deploying this revision.",
                )
                .color(theme.text.secondary),
            );
            ui.add_space(space::S8);
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
    Card::new().show(ui, |ui| {
        ui.heading("Storage metadata");
        ui.spacing_mut().item_spacing.y = space::S12;
        crate::ui::widgets::key_value_chips(ui, "Labels", &release.storage_labels);
        crate::ui::widgets::key_value_chips(ui, "Annotations", &release.storage_annotations);
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

pub(in crate::ui) fn helm_status_tone(status: &str) -> Tone {
    match status {
        "deployed" => Tone::Success,
        "pending-install" | "pending-upgrade" | "pending-rollback" | "uninstalling" => {
            Tone::Warning
        }
        "failed" | "uninstalled" => Tone::Danger,
        _ => Tone::Neutral,
    }
}

pub(super) fn display_or_dash(value: &str) -> &str {
    if value.is_empty() { "—" } else { value }
}
