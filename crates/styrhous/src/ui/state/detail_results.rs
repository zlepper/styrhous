use super::*;

impl WorkerResult for crate::worker::ResourceDetailPodUsageUpdated {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(entry) = ui
            .resource_detail_entry_mut(self.history_entry_id)
            .filter(|entry| entry.cluster_key == self.cluster_key)
        {
            entry.record_pod_usage(self.usage);
        }
    }
}
impl WorkerResult for crate::worker::ResourceDetailNodeUsageUpdated {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(entry) = ui
            .resource_detail_entry_mut(self.history_entry_id)
            .filter(|entry| entry.cluster_key == self.cluster_key)
        {
            entry.record_node_usage(self.usage);
        }
    }
}
impl WorkerResult for crate::worker::ResourceDetailNodeUsageFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(entry) = ui
            .resource_detail_entry_mut(self.history_entry_id)
            .filter(|entry| entry.cluster_key == self.cluster_key)
        {
            entry.prune_node_usage_history(time::OffsetDateTime::now_utc());
            entry.node_usage_error = Some(self.error);
        }
    }
}
impl WorkerResult for crate::worker::ResourceDetailNodeUsageMissing {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(entry) = ui
            .resource_detail_entry_mut(self.history_entry_id)
            .filter(|entry| entry.cluster_key == self.cluster_key)
        {
            entry.prune_node_usage_history(time::OffsetDateTime::now_utc());
            entry.node_usage = None;
            entry.node_usage_error = None;
        }
    }
}
impl WorkerResult for crate::worker::ResourceDetailPodUsageFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(entry) = ui
            .resource_detail_entry_mut(self.history_entry_id)
            .filter(|entry| entry.cluster_key == self.cluster_key)
        {
            entry.prune_pod_usage_history(time::OffsetDateTime::now_utc());
            entry.pod_usage_error = Some(self.error);
        }
    }
}
impl WorkerResult for crate::worker::ResourceDetailPodUsageMissing {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(entry) = ui
            .resource_detail_entry_mut(self.history_entry_id)
            .filter(|entry| entry.cluster_key == self.cluster_key)
        {
            entry.prune_pod_usage_history(time::OffsetDateTime::now_utc());
            entry.pod_usage = None;
            entry.pod_usage_missing = true;
            entry.pod_usage_error = None;
        }
    }
}
impl WorkerResult for crate::worker::ResourceDetailUpdated {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::ResourceDetailUpdated {
            cluster_key,
            history_entry_id,
            detail,
        } = self;
        if let Some(entry) = ui
            .resource_detail_entry_mut(history_entry_id)
            .filter(|entry| entry.cluster_key == cluster_key)
        {
            sync_resource_data_editor(&mut entry.data_editor, &detail);
            entry.detail = Some(*detail);
            entry.detail_error = None;
        }
    }
}
impl WorkerResult for crate::worker::ResourceEventsReplaced {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::ResourceEventsReplaced {
            cluster_key,
            history_entry_id,
            events,
        } = self;
        if let Some(entry) = ui
            .resource_detail_entry_mut(history_entry_id)
            .filter(|entry| entry.cluster_key == cluster_key)
        {
            entry.events = events;
            entry.events_error = None;
        }
    }
}
impl WorkerResult for crate::worker::ResourceDetailWatchFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::ResourceDetailWatchFailed {
            cluster_key,
            history_entry_id,
            events,
            error,
        } = self;
        if let Some(entry) = ui
            .resource_detail_entry_mut(history_entry_id)
            .filter(|entry| entry.cluster_key == cluster_key)
        {
            if events {
                entry.events_error = Some(error);
            } else {
                entry.detail_error = Some(error);
            }
        }
    }
}
impl WorkerResult for crate::worker::ResourceDetailDeleted {
    fn apply(self, ui: &mut UiState, commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::ResourceDetailDeleted {
            cluster_key,
            history_entry_id,
        } = self;
        let closes_active_blade = ui.global_blades.navigator().is_some_and(|navigator| {
            navigator.current().resource_detail().is_some_and(|entry| {
                entry.cluster_key == cluster_key && entry.history_entry_id == history_entry_id
            }) || navigator
                .current()
                .is_owned_by_resource_detail(history_entry_id)
        });
        if closes_active_blade {
            if let Some(cluster) = ui.clusters.get_mut(&cluster_key) {
                cluster.resource_detail_panel = None;
            }
            UiState::stop_discarded_blades(ui.global_blades.clear(), commands);
        } else if let Some(navigator) = ui.global_blades.navigator_mut() {
            navigator.back_stack_mut().retain(|entry| {
                entry.resource_detail().is_none_or(|entry| {
                    entry.cluster_key != cluster_key || entry.history_entry_id != history_entry_id
                }) && !entry.is_owned_by_resource_detail(history_entry_id)
            });
            navigator.forward_stack_mut().retain(|entry| {
                entry.resource_detail().is_none_or(|entry| {
                    entry.cluster_key != cluster_key || entry.history_entry_id != history_entry_id
                }) && !entry.is_owned_by_resource_detail(history_entry_id)
            });
            stop_resource_detail_watches(cluster_key, [history_entry_id], commands);
        }
    }
}
impl WorkerResult for crate::worker::ManagedResourcesReplaced {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::ManagedResourcesReplaced {
            cluster_key,
            history_entry_id,
            resources,
        } = self;
        if let Some(entry) = ui
            .resource_detail_entry_mut(history_entry_id)
            .filter(|entry| entry.cluster_key == cluster_key)
        {
            entry.managed_resources = resources;
            entry.managed_resources_error = None;
        }
    }
}
impl WorkerResult for crate::worker::ManagedResourcesWatchFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::ManagedResourcesWatchFailed {
            cluster_key,
            history_entry_id,
            error,
        } = self;
        if let Some(entry) = ui
            .resource_detail_entry_mut(history_entry_id)
            .filter(|entry| entry.cluster_key == cluster_key)
        {
            entry.managed_resources_error = Some(error);
        }
    }
}
fn sync_resource_data_editor(
    data_editor: &mut Option<ResourceDataEditorState>,
    detail: &ResourceDetail,
) {
    let values = match &detail.payload {
        ResourceDetailPayload::ConfigMap(config_map) => Some(config_map.data.clone()),
        ResourceDetailPayload::Secret(secret) => Some(
            secret
                .data
                .iter()
                .filter_map(|(key, value)| {
                    value.text.as_ref().map(|text| (key.clone(), text.clone()))
                })
                .collect(),
        ),
        ResourceDetailPayload::Generic
        | ResourceDetailPayload::Diagnostic(_)
        | ResourceDetailPayload::Pod(_)
        | ResourceDetailPayload::Node(_) => None,
    };
    match (data_editor.as_mut(), values) {
        (Some(editor), Some(values)) => {
            editor.accept_watched_values(values, detail.resource_version.clone())
        }
        (None, Some(values)) => {
            *data_editor = Some(ResourceDataEditorState::new(
                values,
                detail.resource_version.clone(),
            ))
        }
        (_, None) => *data_editor = None,
    }
}
