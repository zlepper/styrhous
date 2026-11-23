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

    let title = format!("Scale {}?", pending.resource_name);
    let kind = pending.api_resource.kind.clone();
    let description = match pending.namespace.as_deref() {
        Some(namespace) => Prose::new()
            .plain(format!("Sets the desired replica count of the {kind} "))
            .strong(pending.resource_name.clone())
            .plain(" in ")
            .strong(namespace.to_owned())
            .plain("."),
        None => Prose::new()
            .plain(format!("Sets the desired replica count of the {kind} "))
            .strong(pending.resource_name.clone())
            .plain("."),
    };
    let current = pending.current_replicas;
    // The body edits the count; the footer reads it after this frame's edit.
    let edited = std::cell::Cell::new(None);
    let response = Dialog::new(
        egui::Id::new("resource-scale-dialog"),
        components::icons::Icon::ArrowsUpDown,
        components::design::theme::Tone::Accent,
        &title,
    )
    .description(description)
    .show(
        ctx,
        |ui| {
            let valid = parse_replicas(&pending.desired_replicas).is_some();
            let help = format!("Current: {current} · Min 0");
            FormField::new("Desired replicas")
                .help(&help)
                .error((!valid).then_some("Enter a whole number of zero or greater."))
                .show(ui, |ui| {
                    let field = NumberField::new(&mut pending.desired_replicas)
                        .id_salt("desired-replicas")
                        .range(0..=i64::from(i32::MAX))
                        .fallback(i64::from(current))
                        .accessibility_labels(
                            "Desired replicas",
                            "Decrease desired replicas",
                            "Increase desired replicas",
                        )
                        .invalid(!valid);
                    let field = field.show(ui);
                    // The error line and ring read the value from before this
                    // frame's edit; show the new verdict on the next frame.
                    if field.response.changed() || field.stepped {
                        ui.ctx().request_repaint();
                    }
                });
            edited.set(parse_replicas(&pending.desired_replicas));
        },
        |ui| {
            // The confirm button repeats the verb with the new count.
            let replicas = edited.get();
            let confirm_label = replicas.map_or_else(
                || "Scale".to_owned(),
                |replicas| format!("Scale to {replicas}"),
            );
            let scale = Button::primary(confirm_label)
                .enabled(replicas.is_some())
                .show(ui)
                .clicked();
            let cancel = Button::secondary("Cancel").show(ui).clicked();
            (scale.then_some(replicas).flatten(), cancel)
        },
    );
    let (replicas, cancel) = response.inner;
    let scale_request = replicas.map(|replicas| {
        (
            pending.api_resource.clone(),
            pending.namespace.clone(),
            pending.resource_name.clone(),
            replicas,
        )
    });
    if cancel || response.dismissed || scale_request.is_some() {
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

/// The desired replica count `text` holds, if it is a whole number of zero
/// or more.
fn parse_replicas(text: &str) -> Option<i32> {
    text.trim()
        .parse::<i32>()
        .ok()
        .filter(|replicas| *replicas >= 0)
}
