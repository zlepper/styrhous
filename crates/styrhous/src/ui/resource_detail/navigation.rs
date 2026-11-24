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
    let (pod_metrics_shown, node_metrics_shown) = cluster.metrics_shown_as_available();
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
        node_pod_resources: None,
        pod_usage: None,
        pod_usage_history: Vec::new(),
        pod_usage_missing: false,
        pod_metrics_api_unavailable: !pod_metrics_shown,
        pod_metrics_watched: pod_metrics_api_available,
        pod_usage_error: None,
        node_usage: None,
        node_usage_history: Vec::new(),
        node_metrics_api_unavailable: !node_metrics_shown,
        node_metrics_watched: node_metrics_api_available,
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
