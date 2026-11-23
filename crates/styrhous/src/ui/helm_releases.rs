use super::global_blade::{
    GlobalBladeContent, GlobalBladeEffect, GlobalBladeEffectContext, GlobalBladeNavigation,
    GlobalBladeRenderContext, GlobalBladeRenderResult,
};
use super::state::PendingHelmRollback;
use crate::api_resource::ApiResource;
use crate::helm_release::{HelmRelease, HelmReleaseKey};
use components::design::theme::ThemeExt;
use components::design::{radius, space, type_scale};
use components::{BladeLayer, DetailCell, DetailRow, InspectorDetails, WorkspaceCard};

#[derive(Debug)]
pub(super) struct HelmReleaseDetailBlade {
    cluster_key: i32,
    release: HelmReleaseKey,
    selected_revision: i64,
    pending_navigation: Option<ManifestNavigation>,
    pending_rollback: Option<PendingHelmRollback>,
    parsed_manifest: Option<ParsedManifest>,
}

/// Manifest inventory of one revision, so the YAML is not re-parsed every frame.
#[derive(Debug)]
struct ParsedManifest {
    revision: i64,
    manifest: String,
    resources: Vec<ManifestResource>,
}

impl ParsedManifest {
    fn resources_for<'a>(
        cache: &'a mut Option<Self>,
        release: &HelmRelease,
    ) -> &'a [ManifestResource] {
        let stale = cache.as_ref().is_none_or(|parsed| {
            parsed.revision != release.revision || parsed.manifest != release.manifest
        });
        if stale {
            *cache = Some(Self {
                revision: release.revision,
                manifest: release.manifest.clone(),
                resources: manifest_resources(&release.manifest, &release.namespace),
            });
        }
        cache
            .as_ref()
            .map_or(&[], |parsed| parsed.resources.as_slice())
    }
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
            release: HelmReleaseKey {
                namespace,
                release_name,
            },
            selected_revision: 0,
            pending_navigation: None,
            pending_rollback: None,
            parsed_manifest: None,
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
        let theme = ui.tokens();
        let releases = context.helm_releases(
            self.cluster_key,
            &self.release.namespace,
            &self.release.release_name,
        );
        let title = self
            .selected(&releases)
            .map(|release| release.name.as_str())
            .unwrap_or(&self.release.release_name);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            egui::Frame::new()
                .fill(theme.accent.tint)
                .corner_radius(egui::CornerRadius::same(radius::KEY))
                .inner_margin(egui::Margin::symmetric(space::S8 as i8, space::S4 as i8))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("Helm Release")
                            .font(type_scale::BODY.font_id(ui.ctx()))
                            .color(theme.accent.text),
                    );
                });
            ui.add_space(space::S12);
            let title = egui::RichText::new(title)
                .font(type_scale::HEADING.font_id(ui.ctx()))
                .color(theme.text.primary);
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
        let theme = ui.tokens();
        let mut releases = context.helm_releases(
            self.cluster_key,
            &self.release.namespace,
            &self.release.release_name,
        );
        releases.sort_by_key(|release| std::cmp::Reverse(release.revision));
        let Some(release) = self.selected(&releases).cloned() else {
            ui.label(
                egui::RichText::new("This Helm release is no longer available.")
                    .color(theme.text.muted),
            );
            return GlobalBladeRenderResult::default();
        };
        WorkspaceCard::new()
            .margin(space::S16 as i8)
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
        ui.add_space(space::S12);
        let mut column_settings = None;
        // `releases` is sorted newest first.
        let current_revision = releases
            .first()
            .map_or(release.revision, |current| current.revision);
        let rollback = if release.revision == current_revision {
            RevisionRollback::Unavailable
        } else if context.is_helm_rollback_in_progress(self.cluster_key, &self.release) {
            RevisionRollback::InProgress
        } else {
            RevisionRollback::Available
        };
        let history = show_revision_history(
            ui,
            &releases,
            release.revision,
            rollback,
            release.id(),
            context.table_preferences(),
            &mut column_settings,
        );
        if let Some(revision) = history.selected_revision {
            self.selected_revision = revision;
        }
        if history.rollback_requested {
            self.pending_rollback = Some(PendingHelmRollback {
                release: self.release.clone(),
                target_revision: release.revision,
            });
        }
        ui.add_space(space::S12);
        show_release_notes(ui, &release.notes);
        ui.add_space(space::S12);
        let values = release.values_yaml();
        show_values_disclosure(ui, &values);
        ui.add_space(space::S12);
        let resources = ParsedManifest::resources_for(&mut self.parsed_manifest, &release);
        show_manifest_resources(
            ui,
            resources,
            release.id(),
            &mut self.pending_navigation,
            self.cluster_key,
            context,
            &mut column_settings,
        );
        ui.add_space(space::S12);
        show_storage_metadata(ui, &release);
        GlobalBladeRenderResult {
            next_content: column_settings
                .map(|target| Box::new(target) as Box<dyn GlobalBladeContent>),
            ..Default::default()
        }
    }

    fn take_effect(&mut self) -> Option<Box<dyn GlobalBladeEffect>> {
        let action = if let Some(rollback) = self.pending_rollback.take() {
            HelmReleaseDetailAction::RequestRollback(rollback)
        } else {
            HelmReleaseDetailAction::Navigate(self.pending_navigation.take()?)
        };
        Some(Box::new(HelmReleaseDetailEffect {
            cluster_key: self.cluster_key,
            action,
        }))
    }
}

#[derive(Debug)]
enum HelmReleaseDetailAction {
    Navigate(ManifestNavigation),
    /// Opens the rollback confirmation, which is drawn by the application.
    RequestRollback(PendingHelmRollback),
}

#[derive(Debug)]
struct HelmReleaseDetailEffect {
    cluster_key: i32,
    action: HelmReleaseDetailAction,
}

impl GlobalBladeEffect for HelmReleaseDetailEffect {
    fn apply(
        self: Box<Self>,
        context: &mut GlobalBladeEffectContext<'_>,
        navigation: &mut GlobalBladeNavigation<'_>,
    ) {
        match self.action {
            HelmReleaseDetailAction::Navigate(target) => {
                context.ui_state.open_resource_detail(
                    self.cluster_key,
                    target.api_resource,
                    target.name,
                    target.namespace,
                    target.uid,
                    navigation.commands_to_send(),
                );
            }
            HelmReleaseDetailAction::RequestRollback(rollback) => {
                if let Some(cluster) = context.ui_state.clusters.get_mut(&self.cluster_key) {
                    cluster.pending_helm_rollback = Some(rollback);
                }
            }
        }
    }
}

mod manifest;
mod tables;

pub(super) use manifest::{ManifestResource, manifest_resource_namespace, manifest_resources};
use tables::{
    RevisionRollback, display_or_dash, helm_status_tone, show_manifest_resources,
    show_release_notes, show_revision_history, show_storage_metadata, show_values_disclosure,
};

#[cfg(test)]
mod tests;
