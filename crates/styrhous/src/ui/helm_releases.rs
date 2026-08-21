use super::global_blade::{
    GlobalBladeContent, GlobalBladeEffect, GlobalBladeEffectContext, GlobalBladeNavigation,
    GlobalBladeRenderContext, GlobalBladeRenderResult,
};
use crate::api_resource::ApiResource;
use crate::helm_release::HelmRelease;
use components::colors::{gray, indigo};
use components::design::{radius, spacing, typography};
use components::{BladeLayer, DetailCell, DetailRow, InspectorDetails, WorkspaceCard};

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

mod manifest;
mod tables;

pub(super) use manifest::{ManifestResource, manifest_resource_namespace, manifest_resources};
use tables::{
    display_or_dash, helm_status_tone, show_manifest_resources, show_release_notes,
    show_revision_history, show_storage_metadata, show_values_disclosure,
};

#[cfg(test)]
mod tests;
