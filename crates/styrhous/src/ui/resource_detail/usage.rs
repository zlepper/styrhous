use super::*;

pub(super) fn show_pod_summary(
    ui: &mut egui::Ui,
    detail: &ResourceDetail,
    pod: &PodDetail,
    pending_action: &mut Option<ResourceAction>,
) {
    let ready = format!(
        "{}/{}",
        pod.containers
            .iter()
            .filter(|container| container.ready)
            .count(),
        pod.containers.len()
    );
    detail_summary_card(ui, |ui| {
        let node_action = ui.id().with("pod-summary-node");
        let node_cell = pod.node_name.as_deref().map_or_else(
            || DetailCell::unavailable("Node"),
            |node_name| DetailCell::link("Node", node_name, node_action),
        );
        let response = InspectorDetails::show_properties(
            ui,
            &[
                DetailRow::new([
                    DetailCell::new(
                        "Namespace",
                        detail.namespace.as_deref().unwrap_or("Cluster-wide"),
                    ),
                    DetailCell::status("Status", pod.phase.as_str(), pod_phase_tone(&pod.phase)),
                    node_cell,
                ]),
                DetailRow::new([
                    DetailCell::new("Pod IP", pod.pod_ip.as_deref().unwrap_or("-")).copyable(),
                    DetailCell::new("Host IP", pod.host_ip.as_deref().unwrap_or("-")).copyable(),
                    DetailCell::new("QoS class", pod.qos_class.as_deref().unwrap_or("-")),
                ]),
                DetailRow::new([
                    DetailCell::new("Ready", ready),
                    DetailCell::new("Age", format_age(detail.creation_timestamp)),
                ]),
            ],
        );
        if response.activated.contains(&node_action)
            && let Some(node_name) = pod.node_name.as_deref()
            && pending_action.is_none()
        {
            *pending_action = Some(ResourceAction::NavigateDetails {
                api_resource: crate::resource_handlers::node::api_resource(),
                name: node_name.to_owned(),
                namespace: None,
                uid: node_name.to_owned(),
            });
        }
    });
}

pub(super) fn show_pod_detail(ui: &mut egui::Ui, pod: &PodDetail, usage: PodUsageDisplay<'_>) {
    let theme = ui.tokens();
    let pod_requests =
        total_resource_thresholds(&pod.containers, |container| container.resource_requests);
    let pod_limits =
        total_resource_thresholds(&pod.containers, |container| container.resource_limits);
    show_pod_usage(ui, usage, pod_requests, pod_limits);
    ui.add_space(CARD_GAP);
    section_header(ui, "Containers", None);
    for container in &pod.containers {
        detail_item_card(
            ui,
            |ui| {
                ui.label(
                    egui::RichText::new(format!("⌃   {}", container.name))
                        .strong()
                        .color(theme.text.primary),
                );
            },
            |ui| {
                InspectorDetails::show_properties(
                    ui,
                    &[DetailRow::new([
                        DetailCell::new("Image", container.image.as_str()).copyable(),
                        DetailCell::new("State", container.state.as_str()),
                    ])],
                );
                show_container_usage(
                    ui,
                    &container.name,
                    usage
                        .usage
                        .and_then(|usage| usage.containers.get(&container.name)),
                    usage.history,
                    container.resource_requests,
                    container.resource_limits,
                    usage.missing,
                    usage.metrics_api_unavailable,
                    usage.error,
                );
                ui.add_space(6.0);
                InspectorDetails::show_properties(
                    ui,
                    &[DetailRow::new([
                        DetailCell::new("Ready", if container.ready { "Yes" } else { "No" }),
                        DetailCell::new("Restarts", container.restart_count.to_string()),
                    ])],
                );
                chip_rows(
                    ui,
                    [
                        ("Command", &container.command),
                        ("Args", &container.args),
                        ("Ports", &container.ports),
                    ],
                );
                if !container.environment_variables.is_empty() {
                    ui.add_space(10.0);
                    let secret_reveal_scope = ui
                        .make_persistent_id(("environment-variable-secret-scope", &container.name));
                    environment_variables(
                        ui,
                        secret_reveal_scope,
                        &container.environment_variables,
                    );
                }
                if let Some(reason) = &container.reason {
                    InspectorDetails::show_properties(
                        ui,
                        &[DetailRow::new([
                            DetailCell::new("Reason", reason.as_str()).copyable()
                        ])],
                    );
                }
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
            },
        );
        ui.add_space(CARD_GAP);
    }
    if !pod.volumes.is_empty() {
        section_header(ui, "Volumes", None);
        for (index, volume) in pod.volumes.iter().enumerate() {
            detail_item_card(
                ui,
                |ui| {
                    ui.label(
                        egui::RichText::new(format!("⌄   {}", volume.name))
                            .strong()
                            .color(theme.text.primary),
                    );
                },
                |ui| volume_detail_row(ui, volume),
            );
            if index + 1 < pod.volumes.len() {
                ui.add_space(CARD_GAP);
            }
        }
    }
}

pub(super) fn show_pod_usage(
    ui: &mut egui::Ui,
    usage: PodUsageDisplay<'_>,
    requests: PodResourceThresholds,
    limits: PodResourceThresholds,
) {
    section_header(ui, "Resource usage", None);
    Card::new().show(ui, |ui| {
        if usage.metrics_api_unavailable {
            show_metrics_api_unavailable(ui, requests, limits);
            return;
        }
        let displayed_usage = displayed_usage_values(
            usage
                .usage
                .map(|usage| (usage.cpu_nanocores, usage.memory_bytes)),
            usage.error,
        );
        show_usage(
            ui,
            "Pod",
            displayed_usage,
            usage
                .history
                .iter()
                .map(|sample| (sample.timestamp, sample.cpu_nanocores, sample.memory_bytes))
                .collect(),
            [
                usage_references(
                    requests.cpu_nanocores,
                    limits.cpu_nanocores,
                    ["Request", "Limit"],
                ),
                usage_references(
                    requests.memory_bytes,
                    limits.memory_bytes,
                    ["Request", "Limit"],
                ),
            ],
            usage.error.is_some(),
        );
    });
}

/// The current CPU and memory usage: as charts when there is history or a
/// reference line to draw, otherwise as plain values.
fn show_usage(
    ui: &mut egui::Ui,
    subject: &str,
    usage: Option<(i64, i64)>,
    history: Vec<(time::OffsetDateTime, i64, i64)>,
    [cpu_references, memory_references]: [UsageReferences; 2],
    stale: bool,
) {
    if history.is_empty()
        && !has_usage_references(&cpu_references)
        && !has_usage_references(&memory_references)
    {
        show_usage_value_grid(ui, usage);
        return;
    }
    usage_chart_pair(
        ui,
        UsageSeries {
            name: format!("{subject} CPU usage chart"),
            samples: history
                .iter()
                .map(|(timestamp, cpu, _)| (*timestamp, *cpu))
                .collect(),
            value: usage.map(|(cpu, _)| cpu),
            references: cpu_references,
        },
        UsageSeries {
            name: format!("{subject} memory usage chart"),
            samples: history
                .iter()
                .map(|(timestamp, _, memory)| (*timestamp, *memory))
                .collect(),
            value: usage.map(|(_, memory)| memory),
            references: memory_references,
        },
        stale,
    );
}

pub(super) fn show_node_usage(
    ui: &mut egui::Ui,
    usage: NodeUsageDisplay<'_>,
    allocatable: PodResourceThresholds,
) {
    section_header(ui, "Resource usage", None);
    Card::new().show(ui, |ui| {
        if usage.metrics_api_unavailable {
            show_node_metrics_api_unavailable(ui);
            ui.add_space(10.0);
            show_node_pod_resources(
                ui,
                allocatable,
                usage.pod_resources,
                usage.pod_resources_error,
            );
            return;
        }
        show_node_pod_resources(
            ui,
            allocatable,
            usage.pod_resources,
            usage.pod_resources_error,
        );
        ui.add_space(10.0);
        show_usage(
            ui,
            "Node",
            displayed_usage_values(
                usage
                    .usage
                    .map(|usage| (usage.cpu_nanocores, usage.memory_bytes)),
                usage.error,
            ),
            usage
                .history
                .iter()
                .map(|sample| (sample.timestamp, sample.cpu_nanocores, sample.memory_bytes))
                .collect(),
            [
                usage_references(allocatable.cpu_nanocores, None, ["Allocatable", ""]),
                usage_references(allocatable.memory_bytes, None, ["Allocatable", ""]),
            ],
            usage.error.is_some(),
        );
    });
}

#[allow(clippy::too_many_arguments)]
pub(super) fn show_container_usage(
    ui: &mut egui::Ui,
    name: &str,
    usage: Option<&ContainerUsage>,
    history: &[PodUsage],
    requests: PodResourceThresholds,
    limits: PodResourceThresholds,
    _missing: bool,
    metrics_api_unavailable: bool,
    error: Option<&str>,
) {
    ui.add_space(10.0);
    if metrics_api_unavailable {
        show_metrics_api_unavailable(ui, requests, limits);
        return;
    }
    show_usage(
        ui,
        name,
        displayed_usage_values(
            usage.map(|usage| (usage.cpu_nanocores, usage.memory_bytes)),
            error,
        ),
        history
            .iter()
            .filter_map(|sample| {
                sample
                    .containers
                    .get(name)
                    .map(|usage| (sample.timestamp, usage.cpu_nanocores, usage.memory_bytes))
            })
            .collect(),
        [
            usage_references(
                requests.cpu_nanocores,
                limits.cpu_nanocores,
                ["Request", "Limit"],
            ),
            usage_references(
                requests.memory_bytes,
                limits.memory_bytes,
                ["Request", "Limit"],
            ),
        ],
        error.is_some(),
    );
}
