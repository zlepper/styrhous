use super::clock;
use super::global_blade::{
    GlobalBladeContent, GlobalBladeEffect, GlobalBladeEffectContext, GlobalBladeNavigation,
    GlobalBladeRenderContext, GlobalBladeRenderResult,
};
use super::state::PendingHelmRollback;
use crate::api_resource::ApiResource;
use crate::helm_release::{HelmRelease, HelmReleaseKey};
use crate::resource_catalog::kind_category;
use components::design::theme::{ThemeExt, Tone};
use components::icons::Icon;
use components::{
    BladeDescription, BladeLayer, BladeNavigation, BladePeek, BladeStyle, Button, ButtonSize, Card,
    HeaderAction, InspectorBody, InspectorHeader, InspectorStatus, MenuItem, MoreButton, Property,
    PropertyGrid, Readout, ReadoutTone, Section, inspector_section_heading,
};

#[derive(Debug)]
pub(super) struct HelmReleaseDetailBlade {
    cluster_key: i32,
    release: HelmReleaseKey,
    selected_revision: i64,
    pending_navigation: Option<ManifestNavigation>,
    pending_rollback: Option<PendingHelmRollback>,
    parsed_manifest: Option<ParsedManifest>,
    /// The section a key or the header's Values button asked to scroll to.
    pending_scroll: Option<usize>,
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
            pending_scroll: None,
        }
    }

    fn selected<'a>(&self, releases: &'a [HelmRelease]) -> Option<&'a HelmRelease> {
        releases
            .iter()
            .find(|release| release.revision == self.selected_revision)
            .or_else(|| releases.iter().max_by_key(|release| release.revision))
    }
}

/// The parts of a release inspector, in order.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Part {
    Overview,
    Revisions,
    Notes,
    Values,
    Manifest,
    Storage,
}

/// "15 Aug" / "1 Aug 2026, 12:00": a Helm timestamp as the design prints it in
/// `zone`, or the recorded text when it is not a timestamp.
fn helm_time(recorded: &str, with_year_and_time: bool, zone: clock::Zone) -> String {
    let format = if with_year_and_time {
        "[day padding:none] [month repr:short] [year], [hour]:[minute]"
    } else {
        "[day padding:none] [month repr:short]"
    };
    time::OffsetDateTime::parse(recorded, &time::format_description::well_known::Rfc3339)
        .ok()
        .and_then(|time| {
            let time = zone.local(time);
            time::format_description::parse(format)
                .ok()
                .and_then(|format| time.format(&format).ok())
        })
        .unwrap_or_else(|| recorded.to_owned())
}

/// "deployed" as "Deployed".
fn status_word(status: &str) -> String {
    let mut characters = status.chars();
    characters
        .next()
        .map(|first| first.to_uppercase().chain(characters).collect())
        .unwrap_or_default()
}

fn status_readout_tone(status: &str) -> ReadoutTone {
    match helm_status_tone(status) {
        Tone::Success => ReadoutTone::Ok,
        Tone::Warning => ReadoutTone::Warning,
        Tone::Danger => ReadoutTone::Problem,
        _ => ReadoutTone::Neutral,
    }
}

impl GlobalBladeContent for HelmReleaseDetailBlade {
    fn description(&self) -> BladeDescription {
        BladeDescription {
            peek: BladePeek {
                kind: Some("Helm release".to_owned()),
                category: Some(kind_category(&ApiResource::helm_releases())),
                name: self.release.release_name.clone(),
            },
            style: BladeStyle::Inspector,
        }
    }

    fn render_header(
        &mut self,
        ui: &mut egui::Ui,
        layer: BladeLayer,
        context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult {
        let mut releases = context.helm_releases(
            self.cluster_key,
            &self.release.namespace,
            &self.release.release_name,
        );
        releases.sort_by_key(|release| std::cmp::Reverse(release.revision));
        let release = self.selected(&releases);
        let title = release.map_or(self.release.release_name.as_str(), |release| {
            release.name.as_str()
        });
        let category = ui
            .tokens()
            .category
            .nth(kind_category(&ApiResource::helm_releases()));
        let navigation = BladeNavigation::new(layer.can_go_back, layer.can_go_forward);
        let state = release.map(|release| status_word(&release.status));
        let reason = release.map(|release| {
            format!(
                "revision {} of {} · {}",
                release.revision,
                releases
                    .first()
                    .map_or(release.revision, |latest| latest.revision),
                release.namespace
            )
        });
        let zone = clock::zone(ui.ctx());
        let readout_texts = release.map(|release| {
            [
                (
                    "Status",
                    status_word(&release.status),
                    status_readout_tone(&release.status),
                ),
                (
                    "Chart",
                    format!("{} {}", release.chart, release.chart_version),
                    ReadoutTone::Neutral,
                ),
                ("App", release.app_version.clone(), ReadoutTone::Neutral),
                (
                    "Revision",
                    release.revision.to_string(),
                    ReadoutTone::Neutral,
                ),
                (
                    "Updated",
                    helm_time(&release.last_deployed, false, zone),
                    ReadoutTone::Neutral,
                ),
            ]
        });
        let readouts: Vec<Readout<'_>> = readout_texts
            .iter()
            .flatten()
            .map(|(label, value, tone)| Readout::new(label, value).tone(*tone))
            .collect();
        let mut header = InspectorHeader::new(title, navigation)
            .kind("Helm release", Some(category))
            .readouts(&readouts);
        if let (Some(release), Some(state), Some(reason)) =
            (release, state.as_deref(), reason.as_deref())
        {
            header = header.status(InspectorStatus {
                tone: helm_status_tone(&release.status),
                state,
                reason: Some(reason),
                link: None,
            });
        }
        let mut jump_to_values = false;
        let response = header.show(ui, |ui| {
            jump_to_values = Button::secondary("Values")
                .size(ButtonSize::Sm)
                .shortcut("V")
                .show(ui)
                .clicked();
            MoreButton::new(format!("More actions for {title}")).show(ui, |menu| {
                let copy = menu.item(MenuItem::new("Copy name").icon(Icon::ClipboardDocument));
                if copy.clicked() {
                    copy.ctx.copy_text(title.to_owned());
                }
            });
        });
        if layer.is_foreground
            && !ui.ctx().egui_wants_keyboard_input()
            && ui.ctx().memory(|memory| memory.top_modal_layer().is_none())
            && ui.input(|input| input.modifiers.is_none() && input.key_pressed(egui::Key::V))
        {
            jump_to_values = true;
        }
        if jump_to_values {
            self.pending_scroll = Some(Part::Values as usize);
        }
        GlobalBladeRenderResult {
            header_action: if response.back {
                HeaderAction::Back
            } else if response.forward {
                HeaderAction::Forward
            } else if response.close {
                HeaderAction::Close
            } else {
                HeaderAction::None
            },
            ..Default::default()
        }
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
        let resources = ParsedManifest::resources_for(&mut self.parsed_manifest, &release).to_vec();
        let has_storage_metadata =
            !release.storage_labels.is_empty() || !release.storage_annotations.is_empty();
        let mut parts = vec![
            (Part::Overview, "Overview", None),
            (Part::Revisions, "Revisions", Some(releases.len())),
            (Part::Notes, "Release notes", None),
            (Part::Values, "Values", None),
            (Part::Manifest, "Manifest", Some(resources.len())),
        ];
        if has_storage_metadata {
            parts.push((Part::Storage, "Storage metadata", None));
        }
        let rail: Vec<Section<'_>> = parts
            .iter()
            .map(|(_, name, count)| match count {
                Some(count) => Section::new(name).count(*count),
                None => Section::new(name),
            })
            .collect();
        let scroll_to = self.pending_scroll.take().and_then(|part| {
            parts
                .iter()
                .position(|(candidate, ..)| *candidate as usize == part)
        });
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
        let values = release.values_yaml();
        let cluster_key = self.cluster_key;
        InspectorBody::new(
            (
                "helm-release-inspector",
                self.cluster_key,
                self.release.namespace.as_str(),
                self.release.release_name.as_str(),
            ),
            "On this release",
            &rail,
        )
        .scroll_to(scroll_to)
        .show(ui, |ui, index| match parts[index].0 {
            Part::Overview => {
                inspector_section_heading(ui, "Overview", None);
                show_overview(ui, &release);
            }
            Part::Revisions => {
                inspector_section_heading(
                    ui,
                    "Revisions",
                    Some(&format!("revision {} selected", release.revision)),
                );
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
            }
            Part::Notes => {
                inspector_section_heading(
                    ui,
                    "Release notes",
                    Some(&format!(
                        "printed by Helm after revision {}",
                        release.revision
                    )),
                );
                show_release_notes(ui, &release.notes);
            }
            Part::Values => {
                inspector_section_heading(ui, "Values", Some("user-supplied"));
                show_values_disclosure(ui, &values);
            }
            Part::Manifest => {
                inspector_section_heading(
                    ui,
                    "Manifest",
                    Some(&match resources.len() {
                        1 => "1 resource".to_owned(),
                        count => format!("{count} resources"),
                    }),
                );
                show_manifest_resources(
                    ui,
                    &resources,
                    release.id(),
                    &mut self.pending_navigation,
                    cluster_key,
                    context,
                    &mut column_settings,
                );
            }
            Part::Storage => {
                inspector_section_heading(ui, "Storage metadata", None);
                show_storage_metadata(ui, &release);
            }
        });
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

/// The overview card: where the release lives and how it is stored.
fn show_overview(ui: &mut egui::Ui, release: &HelmRelease) {
    let first_deployed = helm_time(&release.first_deployed, true, clock::zone(ui.ctx()));
    let properties = [
        Property::text("Namespace", release.namespace.as_str()),
        Property::text("First deployed", first_deployed.as_str()),
        Property::text("Storage", release.storage.to_string()),
        Property::mono("Record", release.storage_name.as_str()).copyable(),
    ];
    Card::new().show(ui, |ui| {
        PropertyGrid::new(&properties).max_columns(2).show(ui)
    });
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
pub(super) use tables::helm_status_tone;
use tables::{
    RevisionRollback, show_manifest_resources, show_release_notes, show_revision_history,
    show_storage_metadata, show_values_disclosure,
};

#[cfg(test)]
mod tests;
