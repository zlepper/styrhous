//! The resource inspector (C19, boards A05): an [`InspectorHeader`] with its
//! readouts over a section rail and the sections of the resource.

use super::*;
use crate::minimal_resource::PodLogContainer;
use crate::resource_table::ContainerKind;
use crate::ui::clock::frame_age;
use components::design::theme::Tone as ThemeTone;
use components::{
    CodeBlock, Disclosure, InspectorBody, InspectorHeader, InspectorStatus, Property, PropertyGrid,
    Readout, ReadoutTone, Section, inspector_section_heading,
};

/// Lines of YAML the inspector shows before it sends the reader to the editor.
const YAML_LINE_LIMIT: usize = 300;
/// What a value the resource does not report reads as.
const MISSING: &str = "—";

/// The parts of an inspector, in the order they follow each other.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    Overview,
    Usage,
    Containers,
    Volumes,
    Conditions,
    Data,
    Managed(usize),
    Diagnostic(usize),
    Labels,
    Annotations,
    Events,
    Yaml,
}

struct SectionInfo {
    part: Part,
    name: String,
    count: Option<usize>,
    /// How many of the counted items are problems.
    alerts: usize,
}

impl SectionInfo {
    fn new(part: Part, name: impl Into<String>, count: Option<usize>) -> Self {
        Self {
            part,
            name: name.into(),
            count,
            alerts: 0,
        }
    }

    fn alerts(mut self, alerts: usize) -> Self {
        self.alerts = alerts;
        self
    }
}

/// The sections of the inspector of `entry`: only the overview while the
/// resource loads.
fn sections(entry: &ResourceDetailHistoryEntry) -> Vec<SectionInfo> {
    let mut sections = vec![SectionInfo::new(Part::Overview, "Overview", None)];
    let Some(detail) = entry
        .detail
        .as_ref()
        .filter(|_| entry.detail_error.is_none())
    else {
        return sections;
    };
    match &detail.payload {
        ResourceDetailPayload::Pod(pod) => {
            sections.push(SectionInfo::new(Part::Usage, "Usage", None));
            sections.push(SectionInfo::new(
                Part::Containers,
                "Containers",
                Some(pod.containers.len()),
            ));
            if !pod.volumes.is_empty() {
                sections.push(SectionInfo::new(
                    Part::Volumes,
                    "Volumes",
                    Some(pod.volumes.len()),
                ));
            }
            if !pod.conditions.is_empty() {
                sections.push(SectionInfo::new(
                    Part::Conditions,
                    "Conditions",
                    Some(pod.conditions.len()),
                ));
            }
        }
        ResourceDetailPayload::Node(_) => {
            sections.push(SectionInfo::new(Part::Usage, "Usage", None));
        }
        ResourceDetailPayload::ConfigMap(config_map) => sections.push(SectionInfo::new(
            Part::Data,
            "Data",
            Some(config_map.data.len()),
        )),
        ResourceDetailPayload::Secret(secret) => sections.push(SectionInfo::new(
            Part::Data,
            "Data",
            Some(secret.data.len()),
        )),
        ResourceDetailPayload::Diagnostic(diagnostic) => {
            for (index, section) in diagnostic.sections.iter().enumerate() {
                sections.push(SectionInfo::new(
                    Part::Diagnostic(index),
                    section.title.clone(),
                    None,
                ));
            }
        }
        ResourceDetailPayload::Generic => {}
    }
    for (index, (title, kind)) in managed_resource_table_kinds(&entry.api_resource)
        .iter()
        .enumerate()
    {
        sections.push(SectionInfo::new(
            Part::Managed(index),
            *title,
            Some(managed_resource_rows(&entry.managed_resources, kind).len()),
        ));
    }
    sections.push(SectionInfo::new(
        Part::Labels,
        "Labels",
        Some(detail.labels.len()),
    ));
    sections.push(SectionInfo::new(
        Part::Annotations,
        "Annotations",
        Some(detail.annotations.len()),
    ));
    sections.push(
        SectionInfo::new(Part::Events, "Events", Some(entry.events.len())).alerts(
            entry
                .events
                .iter()
                .filter(|event| event.type_ == "Warning")
                .count(),
        ),
    );
    if detail.yaml.is_some() {
        sections.push(SectionInfo::new(Part::Yaml, "YAML", None));
    }
    sections
}

/// What the header's keys and buttons asked for.
#[derive(Default)]
pub(super) struct HeaderOutcome {
    pub(super) action: Option<ResourceAction>,
    pub(super) navigation: components::HeaderAction,
}

/// A readout built from the entry, owning its words.
struct ReadoutText {
    label: &'static str,
    value: String,
    unit: Option<String>,
    tone: ReadoutTone,
}

impl ReadoutText {
    fn new(label: &'static str, value: impl Into<String>) -> Self {
        Self {
            label,
            value: value.into(),
            unit: None,
            tone: ReadoutTone::Neutral,
        }
    }

    fn unit(mut self, unit: Option<String>) -> Self {
        self.unit = unit;
        self
    }

    fn tone(mut self, tone: ReadoutTone) -> Self {
        self.tone = tone;
        self
    }

    fn readout(&self) -> Readout<'_> {
        let readout = Readout::new(self.label, &self.value).tone(self.tone);
        match &self.unit {
            Some(unit) => readout.unit(unit),
            None => readout,
        }
    }
}

fn limit_unit(limit: Option<i64>, format: fn(i64) -> String) -> Option<String> {
    limit.map(|limit| format!("/ {}", format(limit)))
}

/// The few live values of the resource, checked first.
fn readouts(entry: &ResourceDetailHistoryEntry, now: time::OffsetDateTime) -> Vec<ReadoutText> {
    let Some(detail) = entry.detail.as_ref() else {
        return Vec::new();
    };
    let age = ReadoutText::new("Age", format_age(detail.creation_timestamp, now));
    match &detail.payload {
        ResourceDetailPayload::Pod(pod) => {
            let ready = pod
                .containers
                .iter()
                .filter(|container| container.ready)
                .count();
            let restarts: i32 = pod
                .containers
                .iter()
                .map(|container| container.restart_count)
                .sum();
            let limits =
                total_resource_thresholds(&pod.containers, |container| container.resource_limits);
            let requests =
                total_resource_thresholds(&pod.containers, |container| container.resource_requests);
            let usage = entry
                .pod_usage
                .as_ref()
                .filter(|_| entry.pod_usage_error.is_none() && !entry.pod_metrics_api_unavailable);
            vec![
                ReadoutText::new("Ready", format!("{ready} / {}", pod.containers.len())),
                ReadoutText::new("Restarts", restarts.to_string()).tone(if restarts > 0 {
                    ReadoutTone::Warning
                } else {
                    ReadoutTone::Neutral
                }),
                ReadoutText::new(
                    "CPU",
                    usage.map_or(MISSING.to_owned(), |usage| format_cpu(usage.cpu_nanocores)),
                )
                .unit(limit_unit(
                    limits.cpu_nanocores.or(requests.cpu_nanocores),
                    format_cpu,
                )),
                ReadoutText::new(
                    "Memory",
                    usage.map_or(MISSING.to_owned(), |usage| {
                        format_memory(usage.memory_bytes)
                    }),
                )
                .unit(limit_unit(
                    limits.memory_bytes.or(requests.memory_bytes),
                    format_memory,
                )),
                age,
            ]
        }
        ResourceDetailPayload::Node(node) => {
            let usage = entry.node_usage.as_ref().filter(|_| {
                entry.node_usage_error.is_none() && !entry.node_metrics_api_unavailable
            });
            vec![
                ReadoutText::new(
                    "CPU",
                    usage.map_or(MISSING.to_owned(), |usage| format_cpu(usage.cpu_nanocores)),
                )
                .unit(limit_unit(node.allocatable.cpu_nanocores, format_cpu)),
                ReadoutText::new(
                    "Memory",
                    usage.map_or(MISSING.to_owned(), |usage| {
                        format_memory(usage.memory_bytes)
                    }),
                )
                .unit(limit_unit(node.allocatable.memory_bytes, format_memory)),
                age,
            ]
        }
        _ => Vec::new(),
    }
}

/// The reason a Pod is not well: the first container with one.
fn pod_problem(pod: &PodDetail) -> Option<String> {
    pod.containers.iter().find_map(|container| {
        container
            .reason
            .as_ref()
            .map(|reason| format!("{} {reason}", container.name))
    })
}

fn shell_container(containers: &[PodLogContainer]) -> Option<&PodLogContainer> {
    containers
        .iter()
        .find(|container| matches!(container.kind, ContainerKind::App))
}

fn key_pressed(ui: &egui::Ui, key: egui::Key) -> bool {
    ui.input(|input| input.modifiers.is_none() && input.key_pressed(key))
}

/// The inspector header of `entry`: its identity and status, the primary
/// actions of its kind and its readouts.
pub(super) fn show_header(
    ui: &mut egui::Ui,
    entry: &ResourceDetailHistoryEntry,
    layer: components::BladeLayer,
    supports_scale: bool,
    debug_image_presets: &[DebugImagePreset],
) -> HeaderOutcome {
    let mut outcome = HeaderOutcome::default();
    let (resource, pod) = header_resource(entry);
    let now = crate::ui::clock::now(ui.ctx());
    let readouts = readouts(entry, now);
    let readouts: Vec<Readout<'_>> = readouts.iter().map(ReadoutText::readout).collect();
    let theme = ui.tokens();
    let category = theme
        .category
        .nth(crate::resource_catalog::kind_category(&entry.api_resource));
    let phase = pod.map(|pod| pod.phase.as_str());
    let problem = pod.and_then(pod_problem);
    let node = pod.and_then(|pod| pod.node_name.as_deref());
    let mut header = InspectorHeader::new(&entry.resource_name, layer.navigation())
        .kind(&entry.api_resource.kind, Some(category))
        .readouts(&readouts);
    if let Some(phase) = phase {
        header = header.status(InspectorStatus {
            tone: pod_phase_tone(phase),
            state: phase,
            reason: Some(problem.as_deref().unwrap_or("on"))
                .filter(|reason| problem.is_some() || node.is_some() && !reason.is_empty()),
            link: if problem.is_some() { None } else { node },
        });
    }
    let more_label = if layer.is_foreground {
        format!("More actions for {}", resource.name)
    } else {
        format!("More actions for {} in background blade", resource.name)
    };
    let mut action = None;
    let logs = || {
        pod?;
        let targets =
            super::super::resource_actions::log_stream_targets(&resource, &resource.log_containers);
        (!targets.is_empty()).then_some(ResourceAction::ViewLogs { targets })
    };
    let shell = || {
        pod?;
        shell_container(&resource.log_containers).map(|container| ResourceAction::Shell {
            name: resource.name.clone(),
            namespace: resource.namespace.clone(),
            container: container.clone(),
        })
    };
    let response = header.show(ui, |ui| {
        if pod.is_some() {
            let logs_button = Button::primary("Logs")
                .size(ButtonSize::Sm)
                .leading_small_icon(components::icons::Icon::DocumentText)
                .shortcut("L")
                .enabled(logs().is_some());
            if logs_button.show(ui).clicked() {
                action = logs();
            }
            let shell_button = Button::secondary("Shell")
                .size(ButtonSize::Sm)
                .leading_small_icon(components::icons::Icon::CommandLine)
                .shortcut("S")
                .enabled(shell().is_some());
            if shell_button.show(ui).clicked() {
                action = shell();
            }
        }
        MoreButton::new(more_label).show(ui, |menu| {
            show_resource_action_items(
                menu,
                &entry.api_resource,
                &resource,
                &resource.log_containers,
                debug_image_presets,
                supports_scale,
                &mut action,
            );
        });
    });
    outcome.navigation = if response.back {
        components::HeaderAction::Back
    } else if response.forward {
        components::HeaderAction::Forward
    } else if response.close {
        components::HeaderAction::Close
    } else {
        components::HeaderAction::None
    };
    if response.status_link
        && let Some(node_name) = node
    {
        action = Some(node_action(node_name));
    }
    // The keys of the primary actions, while nothing takes text.
    if layer.is_foreground
        && action.is_none()
        && !ui.ctx().egui_wants_keyboard_input()
        && ui.ctx().memory(|memory| memory.top_modal_layer().is_none())
    {
        if key_pressed(ui, egui::Key::L) {
            action = logs();
        } else if key_pressed(ui, egui::Key::S) {
            action = shell();
        } else if key_pressed(ui, egui::Key::V) && entry.detail.is_some() {
            action = Some(ResourceAction::EditYaml {
                name: resource.name.clone(),
                namespace: resource.namespace.clone(),
            });
        }
    }
    outcome.action = action;
    outcome
}

fn node_action(node_name: &str) -> ResourceAction {
    ResourceAction::NavigateDetails {
        api_resource: crate::resource_handlers::node::api_resource(),
        name: node_name.to_owned(),
        namespace: None,
        uid: node_name.to_owned(),
    }
}

/// The resource of `entry` as the action menu knows it, and its Pod detail.
fn header_resource(entry: &ResourceDetailHistoryEntry) -> (MinimalResource, Option<&PodDetail>) {
    let pod = entry
        .detail
        .as_ref()
        .and_then(|detail| match &detail.payload {
            ResourceDetailPayload::Pod(pod) => Some(pod.as_ref()),
            _ => None,
        });
    let mut resource = MinimalResource {
        uid: entry.resource_uid.clone(),
        name: entry.resource_name.clone(),
        namespace: entry.namespace.clone(),
        creation_timestamp: None,
        controller_owner: None,
        labels: Default::default(),
        annotations: Default::default(),
        cells: Default::default(),
        log_containers: pod
            .map(|pod| pod.log_containers.clone())
            .unwrap_or_default(),
    }
    .with_lifecycle_metadata(
        entry
            .detail
            .as_ref()
            .is_some_and(|detail| detail.is_deleting),
        entry
            .detail
            .as_ref()
            .map(|detail| detail.finalizers.clone())
            .unwrap_or_default(),
    );
    if let Some(ResourceDetailPayload::Node(node)) =
        entry.detail.as_ref().map(|detail| &detail.payload)
        && node.unschedulable
    {
        crate::resource_handlers::node::mark_cordoned(&mut resource);
    }
    (resource, pod)
}

/// Everything the sections of an inspector body need besides the entry.
pub(super) struct BodyContext<'a> {
    pub(super) resource_navigation: &'a ResourceNavigation,
    pub(super) table_preferences:
        &'a mut super::super::table_preferences::PersistedResourceTablePreferences,
    pub(super) column_settings:
        &'a mut Option<super::super::resource_table_settings::ResourceTableSettingsTarget>,
}

/// The body of the inspector of `entry`: the rail and its sections.
pub(super) fn show_body(
    ui: &mut egui::Ui,
    entry: &mut ResourceDetailHistoryEntry,
    foreground: bool,
    context: BodyContext<'_>,
) -> BladeResult {
    let mut result = BladeResult::default();
    let infos = sections(entry);
    let rail: Vec<Section<'_>> = infos
        .iter()
        .map(|info| match info.count {
            Some(count) => Section::new(&info.name).count(count).alerts(info.alerts),
            None => Section::new(&info.name),
        })
        .collect();
    let caption = format!("On this {}", on_this(&entry.api_resource.kind));
    let body_id = ("inspector-body", entry.history_entry_id);
    let ResourceDetailHistoryEntry {
        api_resource,
        resource_uid,
        detail,
        events,
        detail_error,
        events_error,
        managed_resources,
        managed_resources_error,
        node_pod_resources,
        pod_usage,
        pod_usage_history,
        pod_usage_missing,
        pod_metrics_api_unavailable,
        pod_usage_error,
        node_usage,
        node_usage_history,
        node_metrics_api_unavailable,
        node_usage_error,
        data_editor,
        resource_name,
        history_entry_id,
        ..
    } = entry;
    let foreground_editor = if foreground {
        data_editor.as_mut()
    } else {
        None
    };
    let mut data_editor = foreground_editor;
    let BodyContext {
        resource_navigation,
        table_preferences,
        column_settings,
    } = context;
    InspectorBody::new(body_id, &caption, &rail).show(ui, |ui, index| {
        let info = &infos[index];
        let detail = detail.as_ref().filter(|_| detail_error.is_none());
        match info.part {
            Part::Overview => {
                inspector_section_heading(ui, "Overview", None);
                if let Some(error) = detail_error.as_deref() {
                    error_card(ui, "Unable to load resource details", error);
                } else if let Some(detail) = detail {
                    show_overview(ui, detail, resource_navigation, &mut result.action);
                } else {
                    components::Spinner::new().show(ui);
                    ui.label(
                        egui::RichText::new("Loading resource details…")
                            .color(ui.tokens().text.muted),
                    );
                }
            }
            Part::Usage => {
                let Some(detail) = detail else { return };
                inspector_section_heading(
                    ui,
                    "Usage",
                    Some(&format!(
                        "metrics.k8s.io · last {} min",
                        POD_USAGE_HISTORY_WINDOW.whole_minutes()
                    )),
                );
                match &detail.payload {
                    ResourceDetailPayload::Pod(pod) => {
                        let requests = total_resource_thresholds(&pod.containers, |container| {
                            container.resource_requests
                        });
                        let limits = total_resource_thresholds(&pod.containers, |container| {
                            container.resource_limits
                        });
                        show_pod_usage(
                            ui,
                            PodUsageDisplay {
                                usage: pod_usage.as_ref(),
                                history: pod_usage_history,
                                missing: *pod_usage_missing,
                                metrics_api_unavailable: *pod_metrics_api_unavailable,
                                error: pod_usage_error.as_deref(),
                            },
                            requests,
                            limits,
                        );
                    }
                    ResourceDetailPayload::Node(node) => with_item_spacing(ui, |ui| {
                        show_node_usage(
                            ui,
                            NodeUsageDisplay {
                                usage: node_usage.as_ref(),
                                history: node_usage_history,
                                metrics_api_unavailable: *node_metrics_api_unavailable,
                                error: node_usage_error.as_deref(),
                                pod_resources: node_pod_resources.as_ref(),
                                pod_resources_error: managed_resources_error.as_deref(),
                            },
                            node.allocatable,
                        )
                    }),
                    _ => {}
                }
            }
            Part::Containers => {
                let Some(ResourceDetail {
                    payload: ResourceDetailPayload::Pod(pod),
                    ..
                }) = detail
                else {
                    return;
                };
                inspector_section_heading(
                    ui,
                    "Containers",
                    Some(&crate::ui::widgets::count_noun(
                        pod.containers.len(),
                        "container",
                    )),
                );
                let display = PodUsageDisplay {
                    usage: pod_usage.as_ref(),
                    history: pod_usage_history,
                    missing: *pod_usage_missing,
                    metrics_api_unavailable: *pod_metrics_api_unavailable,
                    error: pod_usage_error.as_deref(),
                };
                for (index, container) in pod.containers.iter().enumerate() {
                    if index > 0 {
                        ui.add_space(CARD_GAP);
                    }
                    container_card(ui, container, display);
                }
            }
            Part::Volumes => {
                let Some(ResourceDetail {
                    payload: ResourceDetailPayload::Pod(pod),
                    ..
                }) = detail
                else {
                    return;
                };
                inspector_section_heading(
                    ui,
                    "Volumes",
                    Some(&crate::ui::widgets::count_noun(pod.volumes.len(), "volume")),
                );
                for (index, volume) in pod.volumes.iter().enumerate() {
                    if index > 0 {
                        ui.add_space(CARD_GAP);
                    }
                    Disclosure::new(("volume", &volume.name), &volume.name)
                        .mono_title()
                        .default_open(true)
                        .show(ui, |ui| volume_detail_row(ui, volume));
                }
            }
            Part::Conditions => {
                let Some(ResourceDetail {
                    payload: ResourceDetailPayload::Pod(pod),
                    ..
                }) = detail
                else {
                    return;
                };
                inspector_section_heading(ui, "Conditions", None);
                Card::new().show(ui, |ui| {
                    with_item_spacing(ui, |ui| conditions_table(ui, pod))
                });
            }
            Part::Data => {
                let Some(detail) = detail else { return };
                with_item_spacing(ui, |ui| {
                    show_resource_data(ui, detail, data_editor.as_deref_mut(), &mut result.action);
                });
            }
            Part::Diagnostic(index) => {
                let Some(ResourceDetail {
                    payload: ResourceDetailPayload::Diagnostic(diagnostic),
                    ..
                }) = detail
                else {
                    return;
                };
                let section = &diagnostic.sections[index];
                inspector_section_heading(ui, &section.title, None);
                Card::new().show(ui, |ui| {
                    let cells: Vec<_> = section
                        .fields
                        .iter()
                        .map(|field| {
                            DetailCell::new(field.label.as_str(), field.value.as_str()).copyable()
                        })
                        .collect();
                    InspectorDetails::show_properties(ui, &[DetailRow::new(cells)]);
                });
            }
            Part::Managed(table) => {
                let (title, kind) = managed_resource_table_kinds(api_resource)[table];
                let rows = managed_resource_rows(managed_resources, kind);
                inspector_section_heading(ui, title, Some(&format!("{} resources", rows.len())));
                show_managed_resource_table(
                    ui,
                    api_resource,
                    resource_uid,
                    kind,
                    &rows,
                    api_resource.is_node(),
                    &mut result.action,
                    table_preferences,
                    column_settings,
                );
                if rows.is_empty() {
                    ui.add_space(space::S8);
                    ui.label(
                        egui::RichText::new(format!("No {title} found."))
                            .color(ui.tokens().text.muted),
                    );
                }
                if table + 1 == managed_resource_table_kinds(api_resource).len()
                    && let Some(error) = managed_resources_error.as_deref()
                {
                    ui.add_space(space::S8);
                    error_card(ui, "Unable to load all managed resources", error);
                }
            }
            Part::Labels => {
                let Some(detail) = detail else { return };
                inspector_section_heading(ui, "Labels", None);
                metadata_card(ui, "Labels", "No labels", &detail.labels);
            }
            Part::Annotations => {
                let Some(detail) = detail else { return };
                inspector_section_heading(ui, "Annotations", None);
                metadata_card(ui, "Annotations", "No annotations", &detail.annotations);
            }
            Part::Events => {
                inspector_section_heading(ui, "Events", None);
                with_item_spacing(ui, |ui| {
                    show_events(ui, events, events_error.as_deref(), *history_entry_id);
                });
            }
            Part::Yaml => {
                let Some(yaml) = detail.and_then(|detail| detail.yaml.as_deref()) else {
                    return;
                };
                inspector_section_heading(ui, "YAML", Some("read-only"));
                show_yaml(
                    ui,
                    resource_name,
                    yaml,
                    &mut result.action,
                    entry_namespace(detail),
                );
            }
        }
    });
    result
}

fn entry_namespace(detail: Option<&ResourceDetail>) -> Option<String> {
    detail.and_then(|detail| detail.namespace.clone())
}

/// "pod" in "On this pod": the kind in lower case.
fn on_this(kind: &str) -> String {
    let mut words = String::new();
    let mut previous_lower = false;
    for character in kind.chars() {
        if character.is_uppercase() && previous_lower {
            words.push(' ');
        }
        previous_lower = character.is_lowercase();
        words.push(character.to_ascii_lowercase());
    }
    words
}

/// The overview card: the identity of the resource in a two-column grid.
fn show_overview(
    ui: &mut egui::Ui,
    detail: &ResourceDetail,
    navigation: &ResourceNavigation,
    pending_action: &mut Option<ResourceAction>,
) {
    let pod = match &detail.payload {
        ResourceDetailPayload::Pod(pod) => Some(pod.as_ref()),
        _ => None,
    };
    let now = crate::ui::clock::now(ui.ctx());
    let node_id = ui.id().with("overview-node");
    // The controller first, then any other owners; each links to its inspector when the
    // cluster serves its kind.
    let mut owners: Vec<_> = detail.owners.iter().collect();
    owners.sort_by_key(|owner| !owner.controller);
    let owner_links: Vec<_> = owners
        .iter()
        .enumerate()
        .map(|(index, owner)| {
            let action =
                resource_owner::navigation_action(navigation, owner, detail.namespace.as_deref());
            (
                ui.id().with(("overview-owner", index)),
                format!("{} {}", owner.kind, owner.name),
                action,
            )
        })
        .collect();
    let namespace = detail.namespace.as_deref().unwrap_or("Cluster-wide");
    let age = format_age(detail.creation_timestamp, now);
    let mut properties = vec![Property::text("Namespace", namespace).copyable()];
    if let Some(pod) = pod {
        properties.push(Property::text(
            "QoS class",
            pod.qos_class.as_deref().unwrap_or(MISSING),
        ));
        properties.push(match pod.pod_ip.as_deref() {
            Some(ip) => Property::mono("Pod IP", ip).copyable(),
            None => Property::unavailable("Pod IP"),
        });
        properties.push(match pod.host_ip.as_deref() {
            Some(ip) => Property::mono("Host IP", ip).copyable(),
            None => Property::unavailable("Host IP"),
        });
    } else {
        properties.push(Property::text("Age", age.as_str()));
        properties.push(Property::text("Kind", detail.api_resource.kind.as_str()).copyable());
        properties
            .push(Property::text("Resource version", detail.resource_version.as_str()).copyable());
    }
    for (owner, (id, label, action)) in owners.iter().zip(&owner_links) {
        let caption = if owner.controller {
            "Controlled by"
        } else {
            "Owner"
        };
        properties.push(if action.is_some() {
            Property::link(caption, label.as_str(), *id)
        } else {
            Property::text(caption, label.as_str()).hint(resource_owner::unavailable_tooltip(owner))
        });
    }
    if let Some(pod) = pod {
        properties.push(Property::text(
            "Service account",
            pod.service_account_name.as_deref().unwrap_or(MISSING),
        ));
        properties.push(match pod.node_name.as_deref() {
            Some(node) => Property::link("Node", node, node_id),
            None => Property::unavailable("Node"),
        });
    }
    if let ResourceDetailPayload::Node(node) = &detail.payload {
        let taints = if node.taints.is_empty() {
            "None".to_owned()
        } else {
            node.taints.join(", ")
        };
        properties.push(Property::status(
            "Scheduling",
            if node.unschedulable {
                "Scheduling disabled"
            } else {
                "Schedulable"
            },
            if node.unschedulable {
                ThemeTone::Warning
            } else {
                ThemeTone::Success
            },
        ));
        properties.push(Property::text(
            "Provider ID",
            node.provider_id.as_deref().unwrap_or(MISSING),
        ));
        properties.push(Property::text(
            "Pod CIDRs",
            if node.pod_cidrs.is_empty() {
                MISSING.to_owned()
            } else {
                node.pod_cidrs.join(", ")
            },
        ));
        properties.push(Property::text("Taints", taints));
    }
    properties.push(Property::mono("UID", detail.uid.as_str()).copyable());
    let response = Card::new().show(ui, |ui| {
        PropertyGrid::new(&properties).max_columns(2).show(ui)
    });
    for (id, _, action) in owner_links {
        if response.activated.contains(&id)
            && let Some(action) = action
        {
            pending_action.get_or_insert(action);
        }
    }
    if response.activated.contains(&node_id)
        && let Some(node) = pod.and_then(|pod| pod.node_name.as_deref())
    {
        pending_action.get_or_insert_with(|| node_action(node));
    }
}

/// A container: its lamp, name, image and restarts over what it runs with.
fn container_card(ui: &mut egui::Ui, container: &PodContainerDetail, usage: PodUsageDisplay<'_>) {
    let tone = container_tone(container);
    let restarts = match container.restart_count {
        1 => "1 restart".to_owned(),
        count => format!("{count} restarts"),
    };
    Disclosure::new(("container-card", &container.name), &container.name)
        .lamp(tone)
        .mono_title()
        .subtitle(&container.image)
        .trailing(&restarts)
        .default_open(true)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = space::S12;
            let limits = |threshold: PodResourceThresholds| {
                format!(
                    "{} · {}",
                    threshold
                        .cpu_nanocores
                        .map_or(MISSING.to_owned(), format_cpu),
                    threshold
                        .memory_bytes
                        .map_or(MISSING.to_owned(), format_memory)
                )
            };
            let requests_and_limits = format!(
                "{} / {}",
                limits(container.resource_requests),
                limits(container.resource_limits)
            );
            ui.spacing_mut().item_spacing.x = space::S24;
            ui.columns(2, |columns| {
                caption_value(&mut columns[0], "Requests / limits", &requests_and_limits);
                if !container.ports.is_empty() {
                    columns[1].vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        type_scale::CAPTION.label(ui, "PORTS", ui.tokens().text.muted);
                        components::chip_flow(ui, |ui| {
                            for port in &container.ports {
                                components::Chip::new(port)
                                    .copyable_quietly("Port")
                                    .show(ui);
                            }
                        });
                    });
                }
            });
            chip_rows(
                ui,
                [("Command", &container.command), ("Args", &container.args)],
            );
            if !container.state.eq_ignore_ascii_case("running")
                || container.reason.is_some()
                || container.message.is_some()
            {
                let mut cells = vec![DetailCell::new("State", container.state.as_str())];
                if let Some(reason) = &container.reason {
                    cells.push(DetailCell::new("Reason", reason.as_str()).copyable());
                }
                InspectorDetails::show_properties(ui, &[DetailRow::new(cells)]);
                if let Some(message) = &container.message {
                    InspectorDetails::show_properties(
                        ui,
                        &[DetailRow::new([DetailCell::new(
                            "Message",
                            message.as_str(),
                        )
                        .copyable()
                        .free_text()])],
                    );
                }
            }
            if let Some(container_usage) = usage
                .usage
                .and_then(|usage| usage.containers.get(&container.name))
            {
                show_container_usage(
                    ui,
                    &container.name,
                    Some(container_usage),
                    usage.history,
                    container.resource_requests,
                    container.resource_limits,
                    usage.missing,
                    usage.metrics_api_unavailable,
                    usage.error,
                );
            }
            if !container.environment_variables.is_empty() {
                let secret_reveal_scope =
                    ui.make_persistent_id(("environment-variable-secret-scope", &container.name));
                environment_variables(ui, secret_reveal_scope, &container.environment_variables);
            }
        });
}

/// The lamp of a container: ready, still starting, or failing.
fn container_tone(container: &PodContainerDetail) -> ThemeTone {
    if container.ready {
        ThemeTone::Success
    } else if container.state.eq_ignore_ascii_case("terminated")
        || container
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("Error") || reason.contains("BackOff"))
    {
        ThemeTone::Danger
    } else {
        ThemeTone::Warning
    }
}

/// Lay `add` out with the app's item spacing, for content built to sit in a blade body
/// rather than to set its own gaps.
fn with_item_spacing<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing = ui.ctx().global_style().spacing.item_spacing;
        add(ui)
    })
    .inner
}

/// A property caption over its value, for layouts the grid cannot make.
fn caption_value(ui: &mut egui::Ui, caption: &str, value: &str) {
    let theme = ui.tokens();
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        type_scale::CAPTION.label(ui, &caption.to_uppercase(), theme.text.muted);
        type_scale::UI.label(ui, value, theme.text.primary);
    });
}

fn conditions_table(ui: &mut egui::Ui, pod: &PodDetail) {
    let rows = pod
        .conditions
        .iter()
        .map(|condition| {
            DetailTableRow::new([
                DetailTableCell::new(DetailValue::Text(condition.type_.as_str().into())),
                DetailTableCell::new(DetailValue::Status {
                    text: condition.status.as_str().into(),
                    tone: condition_tone(&condition.status),
                }),
                DetailTableCell::new(DetailValue::Text(
                    condition.reason.as_deref().unwrap_or("-").into(),
                ))
                .copyable(),
                DetailTableCell::new(DetailValue::Text(
                    condition.message.as_deref().unwrap_or("-").into(),
                ))
                .copyable(),
            ])
        })
        .collect::<Vec<_>>();
    InspectorDetails::show_table(
        ui,
        &[
            DetailColumn::new("Type"),
            DetailColumn::new("Status"),
            DetailColumn::new("Reason"),
            DetailColumn::new("Message").weight(2.0),
        ],
        &rows,
    );
}

/// Labels or annotations as `key=value` chips in a card.
fn metadata_card(
    ui: &mut egui::Ui,
    caption: &str,
    empty: &str,
    entries: &BTreeMap<String, String>,
) {
    Card::new().show(ui, |ui| {
        if entries.is_empty() {
            type_scale::META.label(ui, empty, ui.tokens().text.muted);
            return;
        }
        components::chip_flow(ui, |ui| {
            for (key, value) in entries {
                let key = format!("{key}=");
                let copy = format!("{key}{value}");
                components::Chip::new(value)
                    .key(&key)
                    .copyable(caption)
                    .copy_as(&copy)
                    .show(ui);
            }
        });
    });
}

fn show_normal_events_id(history_entry_id: u64) -> egui::Id {
    egui::Id::new(("show-normal-events", history_entry_id))
}

/// Whether the reader asked for the normal events of this inspector.
fn show_normal_events(ctx: &egui::Context, history_entry_id: u64) -> bool {
    ctx.data(|data| data.get_temp(show_normal_events_id(history_entry_id)))
        .unwrap_or(false)
}

/// The warning events of the resource, with a note on the normal ones left out.
fn show_events(
    ui: &mut egui::Ui,
    events: &[ResourceEvent],
    error: Option<&str>,
    history_entry_id: u64,
) {
    let theme = ui.tokens();
    if let Some(error) = error {
        Card::new().show(ui, |ui| error_card(ui, "Unable to load events", error));
        return;
    }
    let show_all_id = show_normal_events_id(history_entry_id);
    let show_all = show_normal_events(ui.ctx(), history_entry_id);
    let hidden = events
        .iter()
        .filter(|event| event.type_ == "Normal")
        .count();
    let shown: Vec<&ResourceEvent> = events
        .iter()
        .filter(|event| show_all || event.type_ != "Normal")
        .collect();
    let note = match (shown.is_empty(), hidden) {
        (true, 0) => "No events recorded.".to_owned(),
        (true, hidden) => format!(
            "No warning events. {} hidden.",
            crate::ui::widgets::count_noun(hidden, "normal event")
        ),
        (false, _) => String::new(),
    };
    Card::new().show(ui, |ui| {
        if !note.is_empty() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = space::S8;
                components::icons::Icon::CheckCircle.show(
                    ui,
                    components::design::size::ICON_SM,
                    theme.text.muted,
                );
                type_scale::META.label(ui, &note, theme.text.muted);
            });
        } else {
            let rows = shown
                .iter()
                .map(|event| {
                    DetailTableRow::new([
                        DetailTableCell::new(DetailValue::Status {
                            text: event.type_.as_str().into(),
                            tone: event_tone(&event.type_),
                        }),
                        DetailTableCell::new(DetailValue::Text(event.reason.as_str().into()))
                            .copyable(),
                        DetailTableCell::new(DetailValue::Text(event.message.as_str().into()))
                            .copyable(),
                        DetailTableCell::new(DetailValue::Text(
                            event.source.as_deref().unwrap_or("Kubernetes").into(),
                        ))
                        .copyable(),
                        DetailTableCell::new(DetailValue::Text(
                            format!("{} ago", frame_age(ui.ctx(), event.last_timestamp)).into(),
                        )),
                    ])
                })
                .collect::<Vec<_>>();
            InspectorDetails::show_table(
                ui,
                &[
                    DetailColumn::new("Type"),
                    DetailColumn::new("Reason"),
                    DetailColumn::new("Message").weight(2.0),
                    DetailColumn::new("Source"),
                    DetailColumn::new("Time"),
                ],
                &rows,
            );
        }
        if hidden > 0 {
            ui.add_space(space::S8);
            let label = if show_all {
                "Hide normal events".to_owned()
            } else {
                format!(
                    "Show {}",
                    crate::ui::widgets::count_noun(hidden, "normal event")
                )
            };
            if Button::link(label).show(ui).clicked() {
                ui.ctx()
                    .data_mut(|data| data.insert_temp(show_all_id, !show_all));
            }
        }
    });
}

/// The YAML of the resource, read-only, with the way into the editor.
fn show_yaml(
    ui: &mut egui::Ui,
    name: &str,
    yaml: &str,
    pending_action: &mut Option<ResourceAction>,
    namespace: Option<String>,
) {
    let lines = yaml.lines().count();
    let shown = if lines > YAML_LINE_LIMIT {
        yaml.lines()
            .take(YAML_LINE_LIMIT)
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        yaml.to_owned()
    };
    Card::new().show(ui, |ui| {
        CodeBlock::new(shown.trim_end())
            .filename(&format!("{name}.yaml"))
            .show(ui);
        ui.add_space(space::S12);
        ui.horizontal(|ui| {
            if lines > YAML_LINE_LIMIT {
                type_scale::META.label(
                    ui,
                    &format!("Showing the first {YAML_LINE_LIMIT} of {lines} lines."),
                    ui.tokens().text.muted,
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if Button::secondary("Edit YAML")
                    .size(ButtonSize::Sm)
                    .shortcut("V")
                    .show(ui)
                    .clicked()
                {
                    pending_action.get_or_insert(ResourceAction::EditYaml {
                        name: name.to_owned(),
                        namespace,
                    });
                }
            });
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::tests::design_fixtures::{
        coredns_deployment_detail, coredns_pod_detail, coredns_pod_events, design_now,
    };

    fn entry(detail: ResourceDetail) -> ResourceDetailHistoryEntry {
        ResourceDetailHistoryEntry {
            history_entry_id: 1,
            cluster_key: 1,
            api_resource: detail.api_resource.clone(),
            namespace: detail.namespace.clone(),
            resource_name: detail.name.clone(),
            resource_uid: detail.uid.clone(),
            detail: Some(detail),
            events: coredns_pod_events(),
            detail_error: None,
            events_error: None,
            managed_resources: Vec::new(),
            managed_resources_error: None,
            node_pod_resources: None,
            pod_usage: None,
            pod_usage_history: Vec::new(),
            pod_usage_missing: false,
            pod_metrics_api_unavailable: false,
            pod_metrics_watched: false,
            pod_usage_error: None,
            node_usage: None,
            node_usage_history: Vec::new(),
            node_metrics_api_unavailable: false,
            node_metrics_watched: false,
            node_usage_error: None,
            data_editor: None,
            pending_action: None,
        }
    }

    #[test]
    fn the_rail_caption_names_the_kind_in_lower_case_words() {
        assert_eq!(on_this("Pod"), "pod");
        assert_eq!(on_this("ReplicaSet"), "replica set");
        assert_eq!(on_this("CronJob"), "cron job");
    }

    #[test]
    fn a_pod_lists_its_sections_with_counts_and_a_loading_one_only_the_overview() {
        let mut pod = entry(coredns_pod_detail());
        let names: Vec<_> = sections(&pod)
            .into_iter()
            .map(|section| (section.name, section.count))
            .collect();
        assert_eq!(
            names,
            [
                ("Overview".to_owned(), None),
                ("Usage".to_owned(), None),
                ("Containers".to_owned(), Some(1)),
                ("Labels".to_owned(), Some(3)),
                ("Annotations".to_owned(), Some(1)),
                ("Events".to_owned(), Some(2)),
                ("YAML".to_owned(), None),
            ]
        );

        pod.detail = None;
        assert_eq!(sections(&pod).len(), 1);
    }

    #[test]
    fn a_secret_has_no_inline_yaml_section() {
        let mut detail = coredns_deployment_detail();
        detail.yaml = None;
        let names: Vec<_> = sections(&entry(detail))
            .into_iter()
            .map(|section| section.name)
            .collect();
        assert!(!names.contains(&"YAML".to_owned()));
        assert!(names.contains(&"Events".to_owned()));
    }

    #[test]
    fn pod_readouts_show_usage_against_the_limit_or_else_the_request() {
        let mut pod = entry(coredns_pod_detail());
        pod.pod_usage = Some(crate::pod_metrics::PodUsage {
            timestamp: design_now(),
            cpu_nanocores: 15_000_000,
            memory_bytes: 22 * 1024 * 1024,
            containers: Default::default(),
        });
        let readouts = readouts(&pod, design_now());
        let text: Vec<_> = readouts
            .iter()
            .map(|readout| {
                (
                    readout.label,
                    readout.value.as_str(),
                    readout.unit.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            text,
            [
                ("Ready", "1 / 1", None),
                ("Restarts", "0", None),
                ("CPU", "15m", Some("/ 100m")),
                ("Memory", "22Mi", Some("/ 170Mi")),
                ("Age", "220d", None),
            ]
        );

        // Without a sample the usage is an em dash, never a made-up value.
        pod.pod_usage = None;
        assert_eq!(readouts_value(&pod, "CPU"), MISSING);
    }

    fn readouts_value(entry: &ResourceDetailHistoryEntry, label: &str) -> String {
        readouts(entry, design_now())
            .into_iter()
            .find(|readout| readout.label == label)
            .map(|readout| readout.value)
            .expect("the readout")
    }

    #[test]
    fn restarts_turn_the_readout_to_a_warning() {
        let mut detail = coredns_pod_detail();
        if let ResourceDetailPayload::Pod(pod) = &mut detail.payload {
            pod.containers[0].restart_count = 4;
        }
        let readouts = readouts(&entry(detail), design_now());
        let restarts = readouts
            .iter()
            .find(|readout| readout.label == "Restarts")
            .expect("restarts");
        assert_eq!(restarts.value, "4");
        assert_eq!(restarts.tone, ReadoutTone::Warning);
    }

    #[test]
    fn a_failing_container_gives_the_pod_its_status_reason() {
        let mut detail = coredns_pod_detail();
        let ResourceDetailPayload::Pod(pod) = &mut detail.payload else {
            unreachable!("the fixture is a pod");
        };
        assert_eq!(pod_problem(pod.as_ref()), None);
        pod.containers[0].reason = Some("CrashLoopBackOff".into());
        assert_eq!(
            pod_problem(pod.as_ref()).as_deref(),
            Some("coredns CrashLoopBackOff")
        );
    }
}
