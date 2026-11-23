use super::*;

pub(super) fn metadata_maps(ui: &mut egui::Ui, detail: &ResourceDetail) {
    let theme = ui.tokens();
    disclosure_card(
        ui,
        "labels-and-annotations-open",
        "Labels & annotations",
        false,
        |ui| {
            ui.label(
                egui::RichText::new("Labels")
                    .strong()
                    .color(theme.text.primary),
            );
            InspectorDetails::show_properties(
                ui,
                &[DetailRow::new(detail.labels.iter().map(|(key, value)| {
                    DetailCell::new(key.as_str(), value.as_str())
                        .copyable_as(format!("{key}={value}"))
                }))],
            );
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("Annotations")
                    .strong()
                    .color(theme.text.primary),
            );
            InspectorDetails::show_properties(
                ui,
                &[DetailRow::new(detail.annotations.iter().map(
                    |(key, value)| {
                        DetailCell::new(key.as_str(), value.as_str())
                            .copyable_as(format!("{key}={value}"))
                    },
                ))],
            );
        },
    );
}

pub(super) fn show_events(ui: &mut egui::Ui, events: &[ResourceEvent], error: Option<&str>) {
    let theme = ui.tokens();
    WorkspaceCard::new().margin(0).show(ui, |ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), CARD_HEADER_HEIGHT),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.add_space(CARD_HEADER_PADDING);
                ui.label(
                    egui::RichText::new("Events")
                        .strong()
                        .font(type_scale::TITLE.font_id(ui.ctx()))
                        .color(theme.text.primary),
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
                    ui.label(egui::RichText::new("No events recorded.").color(theme.text.muted));
                } else {
                    let rows = events
                        .iter()
                        .map(|event| {
                            DetailTableRow::new([
                                DetailTableCell::new(DetailValue::Status {
                                    text: event.type_.as_str().into(),
                                    tone: event_tone(&event.type_),
                                }),
                                DetailTableCell::new(DetailValue::Text(
                                    event.reason.as_str().into(),
                                ))
                                .copyable(),
                                DetailTableCell::new(DetailValue::Text(
                                    event.message.as_str().into(),
                                ))
                                .copyable(),
                                DetailTableCell::new(DetailValue::Text(
                                    event.source.as_deref().unwrap_or("Kubernetes").into(),
                                ))
                                .copyable(),
                                DetailTableCell::new(DetailValue::Text(
                                    format!("{} ago", format_age(event.last_timestamp)).into(),
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
            });
    });
}

pub(super) fn show_additional_sections(
    ui: &mut egui::Ui,
    detail: &ResourceDetail,
    resource_navigation: &ResourceNavigation,
    pending_action: &mut Option<ResourceAction>,
) {
    let theme = ui.tokens();
    if let ResourceDetailPayload::Pod(pod) = &detail.payload {
        disclosure_card(ui, "conditions", "Conditions", false, |ui| {
            if pod.conditions.is_empty() {
                ui.label(egui::RichText::new("No conditions reported.").color(theme.text.muted));
            } else {
                let rows = pod
                    .conditions
                    .iter()
                    .map(|condition| {
                        DetailTableRow::new([
                            DetailTableCell::new(DetailValue::Text(
                                condition.type_.as_str().into(),
                            )),
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
        });
        ui.add_space(16.0);
    }
    disclosure_card(ui, "owner-references", "Owner references", false, |ui| {
        if detail.owners.is_empty() {
            ui.label(egui::RichText::new("No owner references.").color(theme.text.muted));
        } else {
            for owner in &detail.owners {
                let label = owner.label();
                if let Some(action) = resource_owner::navigation_action(
                    resource_navigation,
                    owner,
                    detail.namespace.as_deref(),
                ) {
                    let response = ui
                        .add(
                            egui::Label::new(
                                egui::RichText::new(&label)
                                    .font(type_scale::META.font_id(ui.ctx()))
                                    .color(theme.accent.text),
                            )
                            .truncate()
                            .sense(egui::Sense::click()),
                        )
                        .on_hover_text(&label)
                        .with_pointing_hand();
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::Button,
                            response.enabled(),
                            format!("Open details for {label}"),
                        )
                    });
                    if response.clicked() {
                        resource_owner::queue_navigation_action(pending_action, action);
                    }
                } else {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(&label)
                                .font(type_scale::META.font_id(ui.ctx()))
                                .color(theme.text.primary),
                        )
                        .truncate(),
                    )
                    .on_hover_text(resource_owner::unavailable_tooltip(owner));
                }
            }
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
                InspectorDetails::show_properties(
                    ui,
                    &[DetailRow::new([
                        DetailCell::new(
                            "Restart policy",
                            pod.restart_policy.as_deref().unwrap_or("-"),
                        ),
                        DetailCell::new(
                            "Service account",
                            pod.service_account_name.as_deref().unwrap_or("-"),
                        ),
                        DetailCell::new("DNS policy", pod.dns_policy.as_deref().unwrap_or("-")),
                    ])],
                );
            },
        );
    }
}

pub(super) fn pod_phase_tone(phase: &str) -> Tone {
    match phase {
        "Running" | "Succeeded" => Tone::Success,
        "Pending" => Tone::Warning,
        "Failed" => Tone::Danger,
        _ => Tone::Neutral,
    }
}

pub(super) fn event_tone(event_type: &str) -> Tone {
    match event_type {
        "Normal" => Tone::Success,
        "Warning" => Tone::Warning,
        _ => Tone::Neutral,
    }
}

pub(super) fn condition_tone(condition_status: &str) -> Tone {
    match condition_status {
        "True" => Tone::Success,
        "False" => Tone::Neutral,
        "Unknown" => Tone::Warning,
        _ => Tone::Neutral,
    }
}

pub(crate) fn disclosure_card(
    ui: &mut egui::Ui,
    id_source: &str,
    title: &str,
    default_open: bool,
    add_content: impl FnOnce(&mut egui::Ui),
) {
    let theme = ui.tokens();
    WorkspaceCard::new().margin(0).show(ui, |ui| {
        let id = ui.id().with(id_source);
        let mut open = ui
            .data(|data| data.get_temp::<bool>(id))
            .unwrap_or(default_open);
        let (header, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), CARD_HEADER_HEIGHT),
            egui::Sense::click(),
        );
        let response = response.with_pointing_hand();
        if response.clicked() {
            open = !open;
            ui.data_mut(|data| data.insert_temp(id, open));
        }
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::CollapsingHeader,
                ui.is_enabled(),
                open,
                title,
            )
        });
        let painter = ui.painter();
        painter.text(
            header.left_center() + egui::vec2(CARD_HEADER_PADDING, 0.0),
            egui::Align2::LEFT_CENTER,
            title,
            type_scale::BODY.font_id(ui.ctx()),
            theme.text.primary,
        );
        painter.text(
            header.right_center() - egui::vec2(CARD_HEADER_PADDING, 0.0),
            egui::Align2::RIGHT_CENTER,
            if open { "⌃" } else { "⌄" },
            type_scale::TITLE.font_id(ui.ctx()),
            theme.text.secondary,
        );
        if open {
            ui.separator();
            egui::Frame::new()
                .inner_margin(egui::Margin::same(CARD_CONTENT_PADDING))
                .show(ui, add_content);
        }
    });
}

pub(super) fn section_header(ui: &mut egui::Ui, title: &str, detail: Option<String>) {
    let theme = ui.tokens();
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(title)
                .strong()
                .font(type_scale::TITLE.font_id(ui.ctx()))
                .color(theme.text.primary),
        );
        if let Some(detail) = detail {
            ui.label(
                egui::RichText::new(detail)
                    .font(type_scale::BODY.font_id(ui.ctx()))
                    .color(theme.text.secondary),
            );
        }
    });
    ui.add_space(6.0);
}
