use super::*;

pub(crate) fn show_scale_dialog(
    ctx: &egui::Context,
    ui_state: &mut UiState,
    commands_to_send: &mut Vec<WorkerCommandBox>,
) {
    let Some(cluster_id) = ui_state.selected_cluster else {
        return;
    };
    let Some(cluster) = ui_state.clusters.get_mut(&cluster_id) else {
        return;
    };
    let Some(pending) = cluster.pending_scale.as_mut() else {
        return;
    };

    let mut cancel = false;
    let mut scale_request = None;
    let response = Modal::new(egui::Id::new("resource-scale-dialog"))
        .area(
            Modal::default_area(egui::Id::new("resource-scale-dialog"))
                .default_width(SCALE_DIALOG_WIDTH)
                .fade_in(false),
        )
        .backdrop_color(Color32::from_black_alpha(122))
        .frame(
            Frame::new()
                .fill(WHITE)
                .stroke(surface::muted_border())
                .corner_radius(radius::surface())
                .shadow(Shadow {
                    offset: [0, 4],
                    blur: 18,
                    spread: 0,
                    color: Color32::BLACK.gamma_multiply(0.16),
                })
                .inner_margin(Margin::same(spacing::XL as i8)),
        )
        .show(ctx, |ui| {
            ui.set_width(SCALE_DIALOG_WIDTH);
            ui.label(
                egui::RichText::new(pending.api_resource.kind.to_ascii_uppercase())
                    .font(typography::metadata())
                    .color(gray::_500),
            );
            ui.add_space(spacing::SM);
            ui.label(
                egui::RichText::new(format!("Scale {}", pending.resource_name))
                    .font(typography::semibold(24.0))
                    .color(gray::_900),
            );
            ui.add_space(spacing::MD);
            let scope = pending.namespace.as_deref().map_or_else(
                || "the cluster".to_owned(),
                |namespace| format!("the {namespace} namespace"),
            );
            ui.label(
                egui::RichText::new(format!(
                    "Set the desired replica count for this {} in {scope}.",
                    pending.api_resource.kind
                ))
                .font(typography::body())
                .color(gray::_600),
            );
            ui.add_space(spacing::LG);
            ui.label(
                egui::RichText::new("Desired replicas")
                    .font(typography::semibold(14.0))
                    .color(gray::_800),
            );
            ui.add_space(spacing::XS);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let field_width = ui.available_width() - 60.0;
                let desired_replicas = ui.add_sized(
                    egui::vec2(field_width, 30.0),
                    egui::TextEdit::singleline(&mut pending.desired_replicas)
                        .id(egui::Id::new("desired-replicas"))
                        .frame(
                            Frame::new()
                                .fill(WHITE)
                                .stroke(surface::control_border())
                                .corner_radius(radius::control())
                                .inner_margin(Margin::symmetric(spacing::SM as i8, 2)),
                        )
                        .font(typography::body())
                        .vertical_align(Align::Center),
                );
                ui.ctx()
                    .accesskit_node_builder(desired_replicas.id, |builder| {
                        builder.set_label("Desired replicas");
                    });
                Frame::new()
                    .fill(WHITE)
                    .stroke(surface::control_border())
                    .corner_radius(radius::control())
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        let parsed = pending.desired_replicas.parse::<i32>().ok();
                        let decrement = show_scale_stepper_button(
                            ui,
                            "−",
                            "Decrease desired replicas",
                            parsed.is_some_and(|replicas| replicas > 0),
                        );
                        ui.separator();
                        let increment =
                            show_scale_stepper_button(ui, "+", "Increase desired replicas", true);
                        if decrement {
                            pending.desired_replicas = (parsed.unwrap_or_default() - 1).to_string();
                        }
                        if increment {
                            pending.desired_replicas = parsed
                                .unwrap_or(pending.current_replicas)
                                .saturating_add(1)
                                .to_string();
                        }
                    });
            });
            let replicas = pending
                .desired_replicas
                .parse::<i32>()
                .ok()
                .filter(|replicas| *replicas >= 0);
            ui.add_space(spacing::SM);
            ui.label(
                egui::RichText::new(format!(
                    "Current desired replicas: {}",
                    pending.current_replicas
                ))
                .font(typography::body())
                .color(gray::_500),
            );
            if replicas.is_none() {
                ui.add_space(spacing::XS);
                ui.label(
                    egui::RichText::new("Enter a whole number of zero or greater.")
                        .font(typography::metadata())
                        .color(components::design::status::DANGER),
                );
            }
            ui.add_space(spacing::XL);
            ui.separator();
            ui.add_space(spacing::MD);
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add_enabled_ui(replicas.is_some(), |ui| {
                        TailwindButton::primary("Update scale")
                            .size(ButtonSize::Md)
                            .show(ui)
                    })
                    .inner
                    .clicked()
                {
                    scale_request = replicas.map(|replicas| {
                        (
                            pending.api_resource.clone(),
                            pending.namespace.clone(),
                            pending.resource_name.clone(),
                            replicas,
                        )
                    });
                }
                if TailwindButton::secondary("Cancel")
                    .size(ButtonSize::Md)
                    .show(ui)
                    .clicked()
                {
                    cancel = true;
                }
            });
        });

    let escape_pressed = response.is_top_modal
        && !response.any_popup_open
        && ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape));
    if cancel || escape_pressed || scale_request.is_some() {
        cluster.pending_scale = None;
    }
    if let Some((api_resource, namespace, resource_name, replicas)) = scale_request {
        commands_to_send.push(Box::new(UpdateResourceScale {
            cluster_key: cluster.cluster_key,
            api_resource,
            namespace,
            resource_name,
            replicas,
        }));
    }
}

fn show_scale_stepper_button(ui: &mut egui::Ui, glyph: &str, label: &str, enabled: bool) -> bool {
    let response = ui
        .add_enabled_ui(enabled, |ui| {
            let (rect, response) =
                ui.allocate_exact_size(egui::Vec2::splat(28.0), egui::Sense::click());
            if response.hovered() {
                ui.painter()
                    .rect_filled(rect, radius::subtle(), components::colors::gray::_50);
            }
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                glyph,
                typography::body(),
                if enabled { gray::_700 } else { gray::_400 },
            );
            response
        })
        .inner
        .with_pointing_hand();
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label.to_owned())
    });
    response.on_hover_text(label).clicked()
}

pub(crate) fn show_scale_error(ctx: &egui::Context, ui_state: &mut UiState) {
    let Some(cluster_id) = ui_state.selected_cluster else {
        return;
    };
    let Some(error) = ui_state
        .clusters
        .get(&cluster_id)
        .and_then(|cluster| cluster.scale_error.as_deref())
    else {
        return;
    };
    if matches!(
        (ErrorDialog {
            id: egui::Id::new("resource-scale-error"),
            eyebrow: "SCALE",
            title: "Couldn’t update scale",
            message: "Styrhous could not read or update this resource’s scale.",
            details: Some(error),
            recovery: Some("Check the resource’s current state and your Kubernetes permissions."),
            primary_action_label: None,
        })
        .show(ctx),
        ErrorDialogAction::Dismiss
    ) && let Some(cluster) = ui_state.clusters.get_mut(&cluster_id)
    {
        cluster.scale_error = None;
    }
}
