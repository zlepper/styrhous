use super::*;

pub(super) fn metadata_maps(ui: &mut egui::Ui, detail: &ResourceDetail) {
    disclosure_card(
        ui,
        "labels-and-annotations-open",
        "Labels & annotations",
        false,
        |ui| {
            ui.label(egui::RichText::new("Labels").strong().color(gray::_800));
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
                    .color(gray::_800),
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
    WorkspaceCard::new().padding(0).show(ui, |ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), CARD_HEADER_HEIGHT),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.add_space(CARD_HEADER_PADDING);
                ui.label(
                    egui::RichText::new("Events")
                        .strong()
                        .font(typography::section_heading())
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
    if let ResourceDetailPayload::Pod(pod) = &detail.payload {
        disclosure_card(ui, "conditions", "Conditions", false, |ui| {
            if pod.conditions.is_empty() {
                ui.label(egui::RichText::new("No conditions reported.").color(gray::_500));
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
            ui.label(egui::RichText::new("No owner references.").color(gray::_500));
        } else {
            for owner in &detail.owners {
                let label = owner.label();
                let copy_value = format!("{}/{} {}", owner.api_version, owner.kind, owner.name);
                ui.horizontal(|ui| {
                    if let Some(action) = resource_owner::navigation_action(
                        resource_navigation,
                        owner,
                        detail.namespace.as_deref(),
                    ) {
                        let response = ui.add(
                            egui::Label::new(
                                egui::RichText::new(&label)
                                    .font(typography::metadata())
                                    .color(indigo::_600),
                            )
                            .sense(egui::Sense::click()),
                        );
                        response.clone().with_pointing_hand().widget_info(|| {
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
                        ui.label(
                            egui::RichText::new(&label)
                                .font(typography::metadata())
                                .color(gray::_900),
                        )
                        .on_hover_text(resource_owner::unavailable_tooltip(owner));
                    }
                    if TailwindButton::secondary(format!("Copy owner {label}"))
                        .size(ButtonSize::Sm)
                        .show(ui)
                        .clicked()
                    {
                        ui.ctx().copy_text(copy_value);
                    }
                });
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

pub(super) fn pod_phase_tone(phase: &str) -> DetailTone {
    match phase {
        "Running" | "Succeeded" => DetailTone::Success,
        "Pending" => DetailTone::Warning,
        "Failed" => DetailTone::Danger,
        _ => DetailTone::Neutral,
    }
}

pub(super) fn event_tone(event_type: &str) -> DetailTone {
    match event_type {
        "Normal" => DetailTone::Success,
        "Warning" => DetailTone::Warning,
        _ => DetailTone::Neutral,
    }
}

pub(super) fn condition_tone(condition_status: &str) -> DetailTone {
    match condition_status {
        "True" => DetailTone::Success,
        "False" => DetailTone::Neutral,
        "Unknown" => DetailTone::Warning,
        _ => DetailTone::Neutral,
    }
}

pub(crate) fn disclosure_card(
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
            typography::body(),
            gray::_800,
        );
        painter.text(
            header.right_center() - egui::vec2(CARD_HEADER_PADDING, 0.0),
            egui::Align2::RIGHT_CENTER,
            if open { "⌃" } else { "⌄" },
            typography::section_heading(),
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

pub(super) fn section_header(ui: &mut egui::Ui, title: &str, detail: Option<String>) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(title)
                .strong()
                .font(typography::section_heading())
                .color(gray::_800),
        );
        if let Some(detail) = detail {
            ui.label(
                egui::RichText::new(detail)
                    .font(typography::body())
                    .color(gray::_600),
            );
        }
    });
    ui.add_space(6.0);
}
