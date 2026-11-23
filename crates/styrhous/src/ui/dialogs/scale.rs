use super::*;

pub(crate) fn show_scale_dialog(
    ctx: &egui::Context,
    ui_state: &mut UiState,
    commands_to_send: &mut Vec<WorkerCommandBox>,
) {
    let theme = ctx.tokens();
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
    let dialog_id = egui::Id::new("resource-scale-dialog");
    let response = components::dialog_modal(ctx, dialog_id, SCALE_DIALOG_WIDTH).show(ctx, |ui| {
        theme::scope(ui, ThemeScope::Dialog, |ui| {
            ui.set_width(SCALE_DIALOG_WIDTH);
            let scope = pending.namespace.as_deref().map_or_else(
                || "the cluster".to_owned(),
                |namespace| format!("the {namespace} namespace"),
            );
            components::show_dialog_header(
                ui,
                &pending.api_resource.kind.to_ascii_uppercase(),
                &format!("Scale {}", pending.resource_name),
                &format!(
                    "Set the desired replica count for this {} in {scope}.",
                    pending.api_resource.kind
                ),
            );
            ui.add_space(space::S16);
            ui.label(
                egui::RichText::new("Desired replicas")
                    .font(type_scale::BODY_STRONG.font_id(ui.ctx()))
                    .color(theme.text.primary),
            );
            ui.add_space(space::S4);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let field_width = ui.available_width() - 60.0;
                let desired_replicas = ui.add_sized(
                    egui::vec2(field_width, 30.0),
                    egui::TextEdit::singleline(&mut pending.desired_replicas)
                        .id(egui::Id::new("desired-replicas"))
                        .frame(
                            Frame::new()
                                .fill(theme.components.field_bg)
                                .stroke(stroke::control(&theme))
                                .corner_radius(egui::CornerRadius::same(radius::KEY))
                                .inner_margin(Margin::symmetric(space::S8 as i8, 2)),
                        )
                        .font(type_scale::BODY.font_id(ui.ctx()))
                        .vertical_align(Align::Center),
                );
                ui.ctx()
                    .accesskit_node_builder(desired_replicas.id, |builder| {
                        builder.set_label("Desired replicas");
                    });
                Frame::new()
                    .fill(theme.control.fill)
                    .stroke(stroke::control(&theme))
                    .corner_radius(egui::CornerRadius::same(radius::KEY))
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
            ui.add_space(space::S8);
            ui.label(
                egui::RichText::new(format!(
                    "Current desired replicas: {}",
                    pending.current_replicas
                ))
                .font(type_scale::BODY.font_id(ui.ctx()))
                .color(theme.text.muted),
            );
            if replicas.is_none() {
                ui.add_space(space::S4);
                ui.label(
                    egui::RichText::new("Enter a whole number of zero or greater.")
                        .font(type_scale::META.font_id(ui.ctx()))
                        .color(theme.danger.text),
                );
            }
            ui.add_space(space::S24);
            ui.separator();
            ui.add_space(space::S12);
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
        })
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
    let theme = ui.tokens();
    let response = ui
        .add_enabled_ui(enabled, |ui| {
            let (rect, response) =
                ui.allocate_exact_size(egui::Vec2::splat(28.0), egui::Sense::click());
            if response.hovered() {
                ui.painter().rect_filled(
                    rect,
                    egui::CornerRadius::same(radius::KEY),
                    theme.control.fill_hover,
                );
            }
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                glyph,
                type_scale::BODY.font_id(ui.ctx()),
                if enabled {
                    theme.text.secondary
                } else {
                    theme.text.disabled
                },
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
