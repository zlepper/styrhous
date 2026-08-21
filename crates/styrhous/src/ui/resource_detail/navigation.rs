use super::*;

pub(super) fn navigate_resource_detail_in_navigator(
    ui_state: &mut UiState,
    navigation: &mut GlobalBladeNavigation<'_>,
    cluster_key: i32,
    api_resource: crate::api_resource::ApiResource,
    name: String,
    namespace: Option<String>,
    uid: String,
) {
    let Some(cluster) = ui_state.clusters.get_mut(&cluster_key) else {
        return;
    };
    cluster.next_detail_generation += 1;
    let history_entry_id = cluster.next_detail_generation;
    let pod_metrics_api_available = cluster.pod_metrics_api_available;
    let node_metrics_api_available = cluster.node_metrics_api_available;
    if cluster.resource_detail_panel.is_none() {
        return;
    }
    navigation.push(Box::new(ResourceDetailHistoryEntry {
        history_entry_id,
        cluster_key,
        api_resource: api_resource.clone(),
        namespace: namespace.clone(),
        resource_name: name.clone(),
        resource_uid: uid.clone(),
        detail: None,
        events: Vec::new(),
        detail_error: None,
        events_error: None,
        managed_resources: Vec::new(),
        managed_resources_error: None,
        pod_usage: None,
        pod_usage_history: Vec::new(),
        pod_usage_missing: false,
        pod_metrics_api_unavailable: !pod_metrics_api_available,
        pod_usage_error: None,
        node_usage: None,
        node_usage_history: Vec::new(),
        node_metrics_api_unavailable: !node_metrics_api_available,
        node_usage_error: None,
        data_editor: None,
        pending_action: None,
    }));
    navigation
        .commands_to_send()
        .push(Box::new(crate::worker::StartResourceDetailWatch {
            cluster_key: cluster.cluster_key,
            history_entry_id,
            api_resource,
            namespace,
            resource_name: name,
            resource_uid: uid,
            pod_metrics_api_available,
            node_metrics_api_available,
        }));
}

pub(super) fn show_resource_detail_header(
    ui: &mut egui::Ui,
    entry: &ResourceDetailHistoryEntry,
    is_foreground: bool,
    supports_scale: bool,
    debug_image_presets: &[DebugImagePreset],
) -> BladeResult {
    let mut result = BladeResult::default();
    let log_containers = entry
        .detail
        .as_ref()
        .and_then(|detail| match &detail.payload {
            ResourceDetailPayload::Pod(pod) => Some(pod.log_containers.clone()),
            _ => None,
        })
        .unwrap_or_default();
    let resource = MinimalResource {
        uid: entry.resource_uid.clone(),
        name: entry.resource_name.clone(),
        namespace: entry.namespace.clone(),
        creation_timestamp: None,
        controller_owner: None,
        labels: Default::default(),
        annotations: Default::default(),
        cells: Default::default(),
        log_containers,
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
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let more_label = if is_foreground {
            format!("More actions for {}", resource.name)
        } else {
            format!("More actions for {} in background blade", resource.name)
        };
        MoreButton::new(more_label).show(ui, |menu| {
            show_resource_action_items(
                menu,
                &entry.api_resource,
                &resource,
                &resource.log_containers,
                debug_image_presets,
                supports_scale,
                &mut result.action,
            );
        });
        ui.add_space(spacing::MD);
        let kind_width = ui
            .painter()
            .layout_no_wrap(
                entry.api_resource.kind.clone(),
                typography::body(),
                indigo::_600,
            )
            .size()
            .x
            + spacing::SM * 2.0;
        let available_name_width = (ui.available_width() - kind_width - spacing::MD).max(0.0);
        let natural_name_width = ui
            .painter()
            .layout_no_wrap(
                entry.resource_name.clone(),
                typography::page_title(),
                gray::_900,
            )
            .size()
            .x;
        let name = egui::RichText::new(&entry.resource_name)
            .font(typography::page_title())
            .color(gray::_900);
        if natural_name_width <= available_name_width {
            ui.label(name);
        } else {
            ui.add_sized(
                egui::vec2(available_name_width, 24.0),
                egui::Label::new(name).truncate().halign(egui::Align::RIGHT),
            );
        }
        ui.add_space(spacing::MD);
        egui::Frame::new()
            .fill(indigo::_50)
            .corner_radius(radius::control())
            .inner_margin(egui::Margin::symmetric(
                spacing::SM as i8,
                spacing::XS as i8,
            ))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(&entry.api_resource.kind)
                        .font(typography::body())
                        .color(indigo::_600),
                );
            });
    });
    result
}

#[allow(clippy::too_many_arguments)]
pub(super) fn show_resource_detail_blade(
    ui: &mut egui::Ui,
    resource_navigation: &ResourceNavigation,
    api_resource: &crate::api_resource::ApiResource,
    _namespace: &Option<String>,
    _resource_name: &str,
    resource_uid: &str,
    detail: &Option<ResourceDetail>,
    events: &[ResourceEvent],
    detail_error: Option<&str>,
    events_error: Option<&str>,
    managed_resources: &[ManagedResource],
    managed_resources_error: Option<&str>,
    pod_usage: Option<&PodUsage>,
    pod_usage_history: &[PodUsage],
    pod_usage_missing: bool,
    pod_metrics_api_unavailable: bool,
    pod_usage_error: Option<&str>,
    node_usage: Option<&NodeUsage>,
    node_usage_history: &[NodeUsage],
    node_metrics_api_unavailable: bool,
    node_usage_error: Option<&str>,
    data_editor: Option<&mut super::super::state::ResourceDataEditorState>,
    table_preferences: Option<
        &mut super::super::table_preferences::PersistedResourceTablePreferences,
    >,
    column_settings: Option<
        &mut Option<super::super::resource_table_settings::ResourceTableSettingsTarget>,
    >,
) -> BladeResult {
    let mut result = BladeResult::default();
    ui.set_max_width(ui.available_width() - 9.0);
    if let Some(error) = detail_error {
        error_card(ui, "Unable to load resource details", error);
    } else if let Some(detail) = detail {
        show_detail(
            ui,
            detail,
            PodUsageDisplay {
                usage: pod_usage,
                history: pod_usage_history,
                missing: pod_usage_missing,
                metrics_api_unavailable: pod_metrics_api_unavailable,
                error: pod_usage_error,
            },
            NodeUsageDisplay {
                usage: node_usage,
                history: node_usage_history,
                metrics_api_unavailable: node_metrics_api_unavailable,
                error: node_usage_error,
            },
            &mut result.action,
        );
        ui.add_space(16.0);
        show_resource_data(ui, detail, data_editor, &mut result.action);
        metadata_maps(ui, detail);
    } else {
        ui.spinner();
        ui.label(egui::RichText::new("Loading resource details…").color(gray::_500));
    }
    ui.add_space(20.0);
    show_managed_resources_for(
        ui,
        api_resource,
        resource_uid,
        managed_resources,
        managed_resources_error,
        &mut result.action,
        table_preferences,
        column_settings,
    );
    ui.add_space(16.0);
    show_events(ui, events, events_error);
    ui.add_space(16.0);
    if let Some(detail) = detail {
        show_additional_sections(ui, detail, resource_navigation, &mut result.action);
        ui.add_space(16.0);
    }
    result
}
