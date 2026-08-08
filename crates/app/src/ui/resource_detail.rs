use super::state::{ResourceAction, ResourceDetailPanelState, UiState};
use crate::minimal_resource::{MinimalResource, format_age};
use crate::resource_detail::{PodDetail, ResourceDetail, ResourceDetailPayload, ResourceEvent};
use crate::worker::WorkerCommand;
use components::WorkspaceCard;
use components::colors::{WHITE, gray, indigo};
use std::cell::RefCell;

const PANEL_WIDTH: f32 = 744.0;
const PANEL_PADDING: i8 = 28;
const CARD_CONTENT_PADDING: i8 = 12;
const CARD_HEADER_HEIGHT: f32 = 40.0;
const CARD_HEADER_PADDING: f32 = 16.0;
const CARD_GAP: f32 = 12.0;

pub(super) fn show(
    ctx: &egui::Context,
    ui_state: &mut UiState,
    commands_to_send: &mut Vec<WorkerCommand>,
) {
    let Some(cluster_key) = ui_state.selected_cluster else {
        return;
    };
    if ui_state
        .clusters
        .get(&cluster_key)
        .and_then(|cluster| cluster.resource_detail_panel.as_ref())
        .is_none()
    {
        return;
    }

    let viewport = ctx.content_rect();
    let mut close = ctx.input(|input| input.key_pressed(egui::Key::Escape));
    let dismiss_on_outside_click = ui_state
        .clusters
        .get_mut(&cluster_key)
        .and_then(|cluster| cluster.resource_detail_panel.as_mut())
        .map(|panel| {
            let value = panel.dismiss_on_outside_click;
            panel.dismiss_on_outside_click = true;
            value
        })
        .unwrap_or(false);
    egui::Area::new(egui::Id::new("resource-detail-scrim"))
        .order(egui::Order::Foreground)
        .fixed_pos(viewport.min)
        .show(ctx, |ui| {
            ui.set_min_size(viewport.size());
            let response =
                ui.interact(ui.max_rect(), ui.id().with("dismiss"), egui::Sense::click());
            ui.painter().rect_filled(
                ui.max_rect(),
                0.0,
                egui::Color32::BLACK.gamma_multiply(0.58),
            );
            close |= dismiss_on_outside_click && response.clicked();
        });

    let panel = ui_state
        .clusters
        .get_mut(&cluster_key)
        .and_then(|cluster| cluster.resource_detail_panel.as_mut())
        .expect("resource detail panel was checked above");
    let mut action = None;
    egui::Area::new(egui::Id::new("resource-detail-panel"))
        .order(egui::Order::Tooltip)
        .fixed_pos(egui::pos2(viewport.max.x - PANEL_WIDTH, viewport.min.y))
        .show(ctx, |ui| {
            ui.set_width(PANEL_WIDTH);
            ui.set_height(viewport.height());
            egui::Frame::new()
                .fill(WHITE)
                .stroke(egui::Stroke::new(1.0, gray::_200))
                .shadow(egui::Shadow {
                    offset: [-4, 0],
                    blur: 16,
                    spread: 0,
                    color: egui::Color32::BLACK.gamma_multiply(0.12),
                })
                .inner_margin(egui::Margin::same(PANEL_PADDING))
                .show(ui, |ui| {
                    ui.set_min_height(viewport.height());
                    action = show_panel(ui, panel, &mut close);
                });
        });

    if let Some(action) = action {
        if let Some(cluster) = ui_state.clusters.get_mut(&cluster_key) {
            match action {
                ResourceAction::EditYaml { name, namespace } => {
                    commands_to_send.push(WorkerCommand::GetResourceYaml {
                        cluster_key: cluster.cluster_key,
                        api_resource: panel_api_resource(cluster),
                        namespace,
                        resource_name: name,
                    });
                }
                ResourceAction::RequestDelete { name, namespace } => {
                    cluster.pending_delete = Some(super::state::PendingDelete {
                        resource_name: name,
                        namespace,
                    });
                }
                ResourceAction::OpenDetails { .. } => {
                    unreachable!("inspector actions cannot open detail")
                }
            }
        }
    }
    if close {
        ui_state.close_resource_detail(cluster_key, commands_to_send);
    }
}

fn panel_api_resource(cluster: &super::state::ClusterState) -> crate::api_resource::ApiResource {
    cluster
        .resource_detail_panel
        .as_ref()
        .expect("detail panel remains open while an action is handled")
        .api_resource
        .clone()
}

fn show_panel(
    ui: &mut egui::Ui,
    panel: &ResourceDetailPanelState,
    close: &mut bool,
) -> Option<ResourceAction> {
    let pending_action = RefCell::new(None);
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width() - 10.0, 44.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(44.0, 28.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.centered_and_justified(|ui| {
                        egui::Frame::new()
                            .fill(egui::Color32::from_rgb(238, 242, 255))
                            .corner_radius(egui::CornerRadius::same(6))
                            .inner_margin(egui::Margin::symmetric(8, 5))
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(&panel.api_resource.kind)
                                        .size(13.0)
                                        .color(indigo::_600),
                                );
                            });
                    });
                },
            );
            ui.add_space(14.0);
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(&panel.resource_name)
                        .size(19.0)
                        .strong()
                        .color(gray::_900),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_sized(
                        egui::vec2(44.0, 44.0),
                        egui::Button::new(egui::RichText::new("×").size(24.0).color(gray::_600))
                            .fill(WHITE)
                            .stroke(egui::Stroke::new(1.0, gray::_200))
                            .corner_radius(egui::CornerRadius::same(8)),
                    )
                    .on_hover_text("Close inspector")
                    .clicked()
                {
                    *close = true;
                }
            });
        },
    );
    ui.horizontal(|ui| {
        ui.add_space(3.0);
        ui.label(
            egui::RichText::new(&panel.api_resource.kind)
                .size(14.0)
                .color(gray::_600),
        );
    });
    ui.add_space(15.0);
    egui::ScrollArea::vertical()
        .id_salt("resource-detail-scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_max_width(ui.available_width() - 9.0);
            if let Some(error) = &panel.detail_error {
                error_card(ui, "Unable to load resource details", error);
            } else if let Some(detail) = &panel.detail {
                show_detail(ui, detail);
            } else {
                ui.spinner();
                ui.label(egui::RichText::new("Loading resource details…").color(gray::_500));
            }
            ui.add_space(20.0);
            show_events(ui, &panel.events, panel.events_error.as_deref());
            ui.add_space(16.0);
            if let Some(detail) = &panel.detail {
                show_additional_sections(ui, detail);
                ui.add_space(16.0);
            }
            show_detail_actions(ui, panel, &mut pending_action.borrow_mut());
        });
    pending_action.into_inner()
}

fn show_detail(ui: &mut egui::Ui, detail: &ResourceDetail) {
    if let ResourceDetailPayload::Pod(pod) = &detail.payload {
        show_pod_summary(ui, detail, pod);
        ui.add_space(13.0);
        show_pod_detail(ui, pod);
    } else {
        show_generic_summary(ui, detail);
    }
    ui.add_space(16.0);
    metadata_maps(ui, detail);
}

fn show_generic_summary(ui: &mut egui::Ui, detail: &ResourceDetail) {
    WorkspaceCard::new().padding(18).show(ui, |ui| {
        detail_grid(ui, |ui, column| match column {
            0 => detail_value(
                ui,
                "Namespace",
                detail.namespace.as_deref().unwrap_or("Cluster-wide"),
            ),
            1 => detail_value(ui, "Age", &format_age(detail.creation_timestamp)),
            _ => detail_value(ui, "UID", &detail.uid),
        });
    });
}

fn show_pod_summary(ui: &mut egui::Ui, detail: &ResourceDetail, pod: &PodDetail) {
    let ready = format!(
        "{}/{}",
        pod.containers
            .iter()
            .filter(|container| container.ready)
            .count(),
        pod.containers.len()
    );
    WorkspaceCard::new().padding(18).show(ui, |ui| {
        detail_grid(ui, |ui, column| match column {
            0 => detail_value(
                ui,
                "Namespace",
                detail.namespace.as_deref().unwrap_or("Cluster-wide"),
            ),
            1 => status_value(ui, "Status", &pod.phase),
            _ => detail_value(ui, "Node", pod.node_name.as_deref().unwrap_or("-")),
        });
        ui.separator();
        detail_grid(ui, |ui, column| match column {
            0 => detail_value(ui, "Pod IP", pod.pod_ip.as_deref().unwrap_or("-")),
            1 => detail_value(ui, "Host IP", pod.host_ip.as_deref().unwrap_or("-")),
            _ => detail_value(ui, "QoS class", pod.qos_class.as_deref().unwrap_or("-")),
        });
        ui.separator();
        detail_grid(ui, |ui, column| match column {
            0 => detail_value(ui, "Ready", &ready),
            1 => detail_value(ui, "Age", &format_age(detail.creation_timestamp)),
            _ => {}
        });
        // Keep the final, two-column row balanced with the full rows above.
        ui.add_space(8.0);
    });
}

fn show_pod_detail(ui: &mut egui::Ui, pod: &PodDetail) {
    section_title(ui, "Containers");
    for container in &pod.containers {
        WorkspaceCard::new()
            .padding(CARD_CONTENT_PADDING)
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(format!("⌃   {}", container.name))
                        .strong()
                        .color(gray::_800),
                );
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);
                detail_grid_columns(ui, 2, |ui, column| match column {
                    0 => detail_value(ui, "Image", &container.image),
                    1 => detail_value(ui, "State", &container.state),
                    _ => {}
                });
                ui.add_space(6.0);
                detail_grid_columns(ui, 2, |ui, column| match column {
                    0 => detail_value(ui, "Ready", if container.ready { "Yes" } else { "No" }),
                    1 => detail_value(ui, "Restarts", &container.restart_count.to_string()),
                    _ => {}
                });
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
                    environment_variables(ui, &container.environment_variables);
                }
                if let Some(reason) = &container.reason {
                    detail_value(ui, "Reason", reason);
                }
                if let Some(message) = &container.message {
                    ui.label(egui::RichText::new(message).size(12.0).color(gray::_500));
                }
            });
        ui.add_space(CARD_GAP);
    }
    if !pod.volumes.is_empty() {
        section_title(ui, "Volumes");
        for (index, volume) in pod.volumes.iter().enumerate() {
            WorkspaceCard::new()
                .padding(CARD_CONTENT_PADDING)
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(format!("⌄   {}", volume.name))
                            .strong()
                            .color(gray::_800),
                    );
                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);
                    volume_detail_row(ui, volume);
                });
            if index + 1 < pod.volumes.len() {
                ui.add_space(CARD_GAP);
            }
        }
    }
}

fn metadata_maps(ui: &mut egui::Ui, detail: &ResourceDetail) {
    disclosure_card(
        ui,
        "labels-and-annotations-open",
        "Labels & annotations",
        false,
        |ui| {
            ui.label(egui::RichText::new("Labels").strong().color(gray::_800));
            for (key, value) in &detail.labels {
                detail_row(ui, key, value);
            }
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("Annotations")
                    .strong()
                    .color(gray::_800),
            );
            for (key, value) in &detail.annotations {
                detail_row(ui, key, value);
            }
        },
    );
}

fn show_events(ui: &mut egui::Ui, events: &[ResourceEvent], error: Option<&str>) {
    WorkspaceCard::new().padding(0).show(ui, |ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), CARD_HEADER_HEIGHT),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.add_space(CARD_HEADER_PADDING);
                ui.label(
                    egui::RichText::new("Events")
                        .strong()
                        .size(15.0)
                        .color(gray::_800),
                );
            },
        );
        ui.separator();
        egui::Frame::new()
            .inner_margin(egui::Margin::same(CARD_CONTENT_PADDING))
            .show(ui, |ui| {
                if let Some(error) = error {
                    error_card(ui, "Unable to load events", error);
                } else if events.is_empty() {
                    ui.label(egui::RichText::new("No events recorded.").color(gray::_500));
                } else {
                    detail_grid_columns(ui, 5, |ui, column| match column {
                        0 => event_header(ui, "Type"),
                        1 => event_header(ui, "Reason"),
                        2 => event_header(ui, "Message"),
                        3 => event_header(ui, "Source"),
                        _ => event_header(ui, "Time"),
                    });
                    ui.separator();
                    for event in events {
                        detail_grid_columns(ui, 5, |ui, column| match column {
                            0 => status_value(ui, "", &event.type_),
                            1 => detail_value(ui, "", &event.reason),
                            2 => detail_value(ui, "", &event.message),
                            3 => detail_value(
                                ui,
                                "",
                                event.source.as_deref().unwrap_or("Kubernetes"),
                            ),
                            _ => detail_value(
                                ui,
                                "",
                                &format!("{} ago", format_age(event.last_timestamp)),
                            ),
                        });
                    }
                }
            });
    });
}

fn show_additional_sections(ui: &mut egui::Ui, detail: &ResourceDetail) {
    if let ResourceDetailPayload::Pod(pod) = &detail.payload {
        disclosure_card(ui, "conditions", "Conditions", false, |ui| {
            if pod.conditions.is_empty() {
                ui.label(egui::RichText::new("No conditions reported.").color(gray::_500));
            } else {
                detail_grid_columns(ui, 4, |ui, column| match column {
                    0 => event_header(ui, "Type"),
                    1 => event_header(ui, "Status"),
                    2 => event_header(ui, "Reason"),
                    _ => event_header(ui, "Message"),
                });
                ui.separator();
                for condition in &pod.conditions {
                    detail_grid_columns(ui, 4, |ui, column| match column {
                        0 => detail_value(ui, "", &condition.type_),
                        1 => status_value(ui, "", &condition.status),
                        2 => detail_value(ui, "", condition.reason.as_deref().unwrap_or("-")),
                        _ => detail_value(ui, "", condition.message.as_deref().unwrap_or("-")),
                    });
                }
            }
        });
        ui.add_space(16.0);
    }
    disclosure_card(ui, "owner-references", "Owner references", false, |ui| {
        if let Some(owner) = &detail.owner {
            detail_row(ui, "Kind", &owner.kind);
            detail_row(ui, "Name", &owner.name);
        } else {
            ui.label(egui::RichText::new("No owner references.").color(gray::_500));
        }
    });
    if let ResourceDetailPayload::Pod(pod) = &detail.payload {
        ui.add_space(16.0);
        disclosure_card(
            ui,
            "resource-configuration",
            "Resource configuration",
            true,
            |ui| {
                detail_row(
                    ui,
                    "Restart policy",
                    pod.restart_policy.as_deref().unwrap_or("-"),
                );
                detail_row(
                    ui,
                    "Service account",
                    pod.service_account_name.as_deref().unwrap_or("-"),
                );
                detail_row(ui, "DNS policy", pod.dns_policy.as_deref().unwrap_or("-"));
            },
        );
    }
}

fn disclosure_card(
    ui: &mut egui::Ui,
    id_source: &str,
    title: &str,
    default_open: bool,
    add_content: impl FnOnce(&mut egui::Ui),
) {
    WorkspaceCard::new().padding(0).show(ui, |ui| {
        let id = ui.id().with(id_source);
        let mut open = ui
            .data(|data| data.get_temp::<bool>(id))
            .unwrap_or(default_open);
        let (header, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), CARD_HEADER_HEIGHT),
            egui::Sense::click(),
        );
        if response.clicked() {
            open = !open;
            ui.data_mut(|data| data.insert_temp(id, open));
        }
        let painter = ui.painter();
        painter.text(
            header.left_center() + egui::vec2(CARD_HEADER_PADDING, 0.0),
            egui::Align2::LEFT_CENTER,
            title,
            egui::FontId::proportional(14.0),
            gray::_800,
        );
        painter.text(
            header.right_center() - egui::vec2(CARD_HEADER_PADDING, 0.0),
            egui::Align2::RIGHT_CENTER,
            if open { "⌃" } else { "⌄" },
            egui::FontId::proportional(16.0),
            gray::_700,
        );
        if open {
            ui.separator();
            egui::Frame::new()
                .inner_margin(egui::Margin::same(CARD_CONTENT_PADDING))
                .show(ui, add_content);
        }
    });
}

fn section_title(ui: &mut egui::Ui, title: &str) {
    ui.label(
        egui::RichText::new(title)
            .strong()
            .size(15.0)
            .color(gray::_800),
    );
    ui.add_space(6.0);
}

fn detail_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new(format!("{label}: "))
                .size(12.0)
                .color(gray::_500),
        );
        ui.label(egui::RichText::new(value).size(12.0).color(gray::_800));
    });
}

fn environment_variables(
    ui: &mut egui::Ui,
    variables: &[crate::resource_detail::PodEnvironmentVariableDetail],
) {
    ui.label(
        egui::RichText::new("Environment variables")
            .size(12.0)
            .color(gray::_500),
    );
    ui.add_space(4.0);
    egui::Frame::new()
        .fill(gray::_100)
        .stroke(egui::Stroke::new(1.0, gray::_200))
        .corner_radius(egui::CornerRadius::same(4))
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            environment_variable_header(ui);
            for variable in variables {
                ui.add_space(2.0);
                environment_variable_row(ui, variable);
            }
        });
}

fn environment_variable_header(ui: &mut egui::Ui) {
    ui.columns(3, |columns| {
        environment_variable_cell(&mut columns[0], "Key", true);
        environment_variable_cell(&mut columns[1], "Value", true);
        environment_variable_cell(&mut columns[2], "Source", true);
    });
}

fn environment_variable_row(
    ui: &mut egui::Ui,
    variable: &crate::resource_detail::PodEnvironmentVariableDetail,
) {
    ui.columns(3, |columns| {
        environment_variable_cell(&mut columns[0], &variable.name, false);
        environment_variable_value_cell(&mut columns[1], variable);
        environment_variable_source_cell(&mut columns[2], variable);
    });
}

fn environment_variable_cell(ui: &mut egui::Ui, value: &str, header: bool) {
    let text = egui::RichText::new(value)
        .monospace()
        .size(11.0)
        .color(if header { gray::_600 } else { gray::_800 });
    let text = if header { text.strong() } else { text };
    egui::Frame::new()
        .fill(WHITE)
        .stroke(egui::Stroke::new(1.0, gray::_200))
        .corner_radius(egui::CornerRadius::same(3))
        .inner_margin(egui::Margin::symmetric(6, 4))
        .show(ui, |ui| {
            ui.set_max_width(ui.available_width());
            ui.add(egui::Label::new(text).selectable(!header).wrap());
        });
}

fn environment_variable_value_cell(
    ui: &mut egui::Ui,
    variable: &crate::resource_detail::PodEnvironmentVariableDetail,
) {
    let secret = matches!(
        variable.source,
        crate::resource_detail::PodEnvironmentVariableSource::SecretKey { .. }
    );
    let revealed = secret && environment_variable_secret_revealed(ui, variable);
    let value = if secret && !revealed {
        "••••••"
    } else {
        variable.value.as_deref().unwrap_or("Unavailable")
    };
    egui::Frame::new()
        .fill(WHITE)
        .stroke(egui::Stroke::new(1.0, gray::_200))
        .corner_radius(egui::CornerRadius::same(3))
        .inner_margin(egui::Margin::symmetric(6, 4))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.set_max_width(ui.available_width());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                if secret && variable.value.is_some() {
                    let action = if revealed { "Hide" } else { "Reveal" };
                    let response = components::icons::eye_button(ui, 14.0, gray::_600, action);
                    if response.on_hover_text(action).clicked() {
                        ui.data_mut(|data| {
                            data.insert_temp(
                                environment_variable_secret_id(ui, variable),
                                !revealed,
                            )
                        });
                    }
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(value)
                                .monospace()
                                .size(11.0)
                                .color(gray::_800),
                        )
                        .selectable(true)
                        .wrap(),
                    );
                });
            });
        });
}

fn environment_variable_source_cell(
    ui: &mut egui::Ui,
    variable: &crate::resource_detail::PodEnvironmentVariableDetail,
) {
    let source = environment_variable_source_label(variable);
    egui::Frame::new()
        .fill(WHITE)
        .stroke(egui::Stroke::new(1.0, gray::_200))
        .corner_radius(egui::CornerRadius::same(3))
        .inner_margin(egui::Margin::symmetric(6, 4))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.set_max_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.add(
                    egui::Label::new(egui::RichText::new(source).size(11.0).color(gray::_600))
                        .wrap(),
                );
            });
        });
}

fn environment_variable_secret_revealed(
    ui: &egui::Ui,
    variable: &crate::resource_detail::PodEnvironmentVariableDetail,
) -> bool {
    ui.data(|data| {
        data.get_temp::<bool>(environment_variable_secret_id(ui, variable))
            .unwrap_or(false)
    })
}

fn environment_variable_secret_id(
    _ui: &egui::Ui,
    variable: &crate::resource_detail::PodEnvironmentVariableDetail,
) -> egui::Id {
    egui::Id::new((
        "environment-variable-secret",
        &variable.name,
        environment_variable_source_label(variable),
    ))
}

fn environment_variable_source_label(
    variable: &crate::resource_detail::PodEnvironmentVariableDetail,
) -> String {
    use crate::resource_detail::PodEnvironmentVariableSource;

    let resolved = if variable.value.is_some() {
        "resolved"
    } else {
        "unavailable"
    };
    match &variable.source {
        PodEnvironmentVariableSource::Literal => "Literal".to_owned(),
        PodEnvironmentVariableSource::ConfigMapKey {
            name,
            key,
            optional,
        } => {
            format!(
                "ConfigMap {name}/{key}{} · {resolved}",
                optional_label(*optional)
            )
        }
        PodEnvironmentVariableSource::SecretKey {
            name,
            key,
            optional,
        } => {
            format!(
                "Secret {name}/{key}{} · {resolved}",
                optional_label(*optional)
            )
        }
        PodEnvironmentVariableSource::Field { path } => format!("Field {path} · {resolved}"),
        PodEnvironmentVariableSource::ResourceField {
            resource,
            container_name,
        } => format!(
            "Resource field {resource}{} · {resolved}",
            container_name
                .as_deref()
                .map(|name| format!(" ({name})"))
                .unwrap_or_default()
        ),
        PodEnvironmentVariableSource::ConfigMapImport { name, optional, .. } => {
            format!(
                "ConfigMap import {name}{} · {resolved}",
                optional_label(*optional)
            )
        }
        PodEnvironmentVariableSource::SecretImport { name, optional, .. } => {
            format!(
                "Secret import {name}{} · {resolved}",
                optional_label(*optional)
            )
        }
        PodEnvironmentVariableSource::Unspecified => "Unspecified source".to_owned(),
    }
}

fn optional_label(optional: bool) -> &'static str {
    if optional { " (optional)" } else { "" }
}

fn detail_grid(ui: &mut egui::Ui, add_column: impl Fn(&mut egui::Ui, usize)) {
    detail_grid_columns(ui, 3, add_column);
}

fn detail_grid_columns(
    ui: &mut egui::Ui,
    column_count: usize,
    add_column: impl Fn(&mut egui::Ui, usize),
) {
    ui.columns(column_count, |columns| {
        for (index, column) in columns.iter_mut().enumerate() {
            add_column(column, index);
        }
    });
    ui.add_space(10.0);
}

fn show_detail_actions(
    ui: &mut egui::Ui,
    panel: &ResourceDetailPanelState,
    pending_action: &mut Option<ResourceAction>,
) {
    let resource = MinimalResource {
        uid: panel.resource_uid.clone(),
        name: panel.resource_name.clone(),
        namespace: panel.namespace.clone(),
        creation_timestamp: None,
        cells: Default::default(),
    };
    ui.horizontal(|ui| {
        if ui.button("Edit YAML").clicked() && pending_action.is_none() {
            *pending_action = Some(ResourceAction::EditYaml {
                name: resource.name.clone(),
                namespace: resource.namespace.clone(),
            });
        }
        if ui
            .button(egui::RichText::new("Delete").color(egui::Color32::from_rgb(185, 28, 28)))
            .clicked()
            && pending_action.is_none()
        {
            *pending_action = Some(ResourceAction::RequestDelete {
                name: resource.name,
                namespace: resource.namespace,
            });
        }
    });
}

fn detail_value(ui: &mut egui::Ui, label: &str, value: &str) {
    if !label.is_empty() {
        ui.label(egui::RichText::new(label).size(12.0).color(gray::_500));
    }
    ui.label(egui::RichText::new(value).size(12.0).color(gray::_900));
}

fn status_value(ui: &mut egui::Ui, label: &str, value: &str) {
    if !label.is_empty() {
        ui.label(egui::RichText::new(label).size(12.0).color(gray::_500));
    }
    ui.horizontal(|ui| {
        ui.colored_label(egui::Color32::from_rgb(34, 197, 94), "●");
        ui.label(egui::RichText::new(value).size(12.0).color(gray::_900));
    });
}

fn event_header(ui: &mut egui::Ui, label: &str) {
    ui.label(egui::RichText::new(label).size(11.0).color(gray::_500));
}

fn chip_row(ui: &mut egui::Ui, label: &str, values: &[String]) {
    ui.label(egui::RichText::new(label).size(12.0).color(gray::_500));
    ui.with_layout(
        egui::Layout::left_to_right(egui::Align::TOP).with_main_wrap(true),
        |ui| {
            for value in values {
                let chip_width = (value.chars().count() as f32 * 6.7 + 10.0).clamp(36.0, 320.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(chip_width, 0.0),
                    egui::Layout::top_down(egui::Align::LEFT),
                    |ui| {
                        egui::Frame::new()
                            .fill(gray::_100)
                            .stroke(egui::Stroke::new(1.0, gray::_200))
                            .corner_radius(egui::CornerRadius::same(4))
                            .inner_margin(egui::Margin::symmetric(5, 0))
                            .show(ui, |ui| {
                                ui.set_max_width(chip_width - 10.0);
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(value)
                                            .monospace()
                                            .size(11.0)
                                            .color(gray::_800),
                                    )
                                    .wrap(),
                                );
                            });
                    },
                );
            }
        },
    );
}

fn volume_detail_row(ui: &mut egui::Ui, volume: &crate::resource_detail::PodVolumeDetail) {
    const COLUMN_WIDTHS: [f32; 3] = [117.0, 159.0, 307.0];
    ui.horizontal_top(|ui| {
        for (width, label, value) in [
            (COLUMN_WIDTHS[0], "Type", volume.kind.as_str()),
            (COLUMN_WIDTHS[1], "Source", volume.source.as_str()),
            (
                COLUMN_WIDTHS[2],
                "Mount path",
                volume.mount_path.as_deref().unwrap_or("-"),
            ),
        ] {
            ui.allocate_ui_with_layout(
                egui::vec2(width, 0.0),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| detail_value(ui, label, value),
            );
        }
        detail_value(
            ui,
            "Read-only",
            if volume.read_only { "true" } else { "false" },
        );
    });
}

fn error_card(ui: &mut egui::Ui, title: &str, error: &str) {
    ui.label(
        egui::RichText::new(title)
            .strong()
            .color(egui::Color32::from_rgb(185, 28, 28)),
    );
    ui.label(egui::RichText::new(error).size(12.0).color(gray::_600));
}
