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
                        .color(gray::_800),
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
                if !container.command.is_empty() {
                    chip_row(ui, "Command", &container.command);
                }
                if !container.args.is_empty() {
                    chip_row(ui, "Args", &container.args);
                }
                if !container.ports.is_empty() {
                    chip_row(ui, "Ports", &container.ports);
                }
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
                        .copyable()])],
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
                            .color(gray::_800),
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
    WorkspaceCard::new().show(ui, |ui| {
        if usage.metrics_api_unavailable {
            show_metrics_api_unavailable(ui, requests, limits);
            return;
        }
        let cpu_references = usage_references(
            requests.cpu_nanocores,
            limits.cpu_nanocores,
            ["Request", "Limit"],
        );
        let memory_references = usage_references(
            requests.memory_bytes,
            limits.memory_bytes,
            ["Request", "Limit"],
        );
        let displayed_usage = displayed_usage_values(
            usage
                .usage
                .map(|usage| (usage.cpu_nanocores, usage.memory_bytes)),
            usage.error,
        );
        show_usage_value_grid(ui, displayed_usage);
        if !usage.history.is_empty()
            || has_usage_references(&cpu_references)
            || has_usage_references(&memory_references)
        {
            if usage.usage.is_none() {
                usage_chart_pair_labels(ui);
            }
            ui.add_space(8.0);
            ui.columns(2, |columns| {
                usage_chart(
                    &mut columns[0],
                    "Pod CPU usage chart",
                    usage
                        .history
                        .iter()
                        .map(|sample| (sample.timestamp, sample.cpu_nanocores))
                        .collect(),
                    format_cpu,
                    if usage.error.is_some() {
                        gray::_400
                    } else {
                        indigo::_600
                    },
                    usage.error.is_some(),
                    &cpu_references,
                );
                usage_chart(
                    &mut columns[1],
                    "Pod memory usage chart",
                    usage
                        .history
                        .iter()
                        .map(|sample| (sample.timestamp, sample.memory_bytes))
                        .collect(),
                    format_memory,
                    if usage.error.is_some() {
                        gray::_400
                    } else {
                        status::SUCCESS
                    },
                    usage.error.is_some(),
                    &memory_references,
                );
            });
        }
    });
}

pub(super) fn show_node_usage(
    ui: &mut egui::Ui,
    usage: NodeUsageDisplay<'_>,
    allocatable: PodResourceThresholds,
) {
    section_header(ui, "Resource usage", None);
    WorkspaceCard::new().show(ui, |ui| {
        if usage.metrics_api_unavailable {
            show_node_metrics_api_unavailable(ui, allocatable);
            return;
        }
        let cpu_references = usage_references(allocatable.cpu_nanocores, None, ["Allocatable", ""]);
        let memory_references =
            usage_references(allocatable.memory_bytes, None, ["Allocatable", ""]);
        show_usage_value_grid(
            ui,
            displayed_usage_values(
                usage
                    .usage
                    .map(|usage| (usage.cpu_nanocores, usage.memory_bytes)),
                usage.error,
            ),
        );
        if !usage.history.is_empty()
            || has_usage_references(&cpu_references)
            || has_usage_references(&memory_references)
        {
            if usage.usage.is_none() {
                usage_chart_pair_labels(ui);
            }
            ui.add_space(8.0);
            ui.columns(2, |columns| {
                usage_chart(
                    &mut columns[0],
                    "Node CPU usage chart",
                    usage
                        .history
                        .iter()
                        .map(|sample| (sample.timestamp, sample.cpu_nanocores))
                        .collect(),
                    format_cpu,
                    if usage.error.is_some() {
                        gray::_400
                    } else {
                        indigo::_600
                    },
                    usage.error.is_some(),
                    &cpu_references,
                );
                usage_chart(
                    &mut columns[1],
                    "Node memory usage chart",
                    usage
                        .history
                        .iter()
                        .map(|sample| (sample.timestamp, sample.memory_bytes))
                        .collect(),
                    format_memory,
                    if usage.error.is_some() {
                        gray::_400
                    } else {
                        status::SUCCESS
                    },
                    usage.error.is_some(),
                    &memory_references,
                );
            });
        }
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
    let cpu_references = usage_references(
        requests.cpu_nanocores,
        limits.cpu_nanocores,
        ["Request", "Limit"],
    );
    let memory_references = usage_references(
        requests.memory_bytes,
        limits.memory_bytes,
        ["Request", "Limit"],
    );
    let displayed_usage = displayed_usage_values(
        usage.map(|usage| (usage.cpu_nanocores, usage.memory_bytes)),
        error,
    );
    show_usage_value_grid(ui, displayed_usage);
    if history
        .iter()
        .any(|sample| sample.containers.contains_key(name))
        || has_usage_references(&cpu_references)
        || has_usage_references(&memory_references)
    {
        if usage.is_none() {
            usage_chart_pair_labels(ui);
        }
        ui.add_space(6.0);
        ui.columns(2, |columns| {
            usage_chart(
                &mut columns[0],
                &format!("{name} CPU usage chart"),
                history
                    .iter()
                    .filter_map(|sample| {
                        sample
                            .containers
                            .get(name)
                            .map(|usage| (sample.timestamp, usage.cpu_nanocores))
                    })
                    .collect(),
                format_cpu,
                if error.is_some() {
                    gray::_400
                } else {
                    indigo::_600
                },
                error.is_some(),
                &cpu_references,
            );
            usage_chart(
                &mut columns[1],
                &format!("{name} memory usage chart"),
                history
                    .iter()
                    .filter_map(|sample| {
                        sample
                            .containers
                            .get(name)
                            .map(|usage| (sample.timestamp, usage.memory_bytes))
                    })
                    .collect(),
                format_memory,
                if error.is_some() {
                    gray::_400
                } else {
                    status::SUCCESS
                },
                error.is_some(),
                &memory_references,
            );
        });
    }
}
