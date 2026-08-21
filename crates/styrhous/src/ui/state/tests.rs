use super::super::log_state::rebase_display_row;
use super::*;
use crate::api_resource::ApiResource;
use crate::cluster_connection_manager::Cluster;
use crate::log_store::LogPageRow;
use crate::minimal_resource::MinimalResource;
use crate::pod_metrics::{ContainerUsage, POD_USAGE_HISTORY_WINDOW};
use crate::resource_table::ContainerKind;
use crate::worker::*;
use std::collections::VecDeque;

fn pod_usage(timestamp: time::OffsetDateTime, cpu_nanocores: i64) -> PodUsage {
    PodUsage {
        timestamp,
        cpu_nanocores,
        memory_bytes: cpu_nanocores,
        containers: BTreeMap::from([(
            "app".to_owned(),
            ContainerUsage {
                cpu_nanocores,
                memory_bytes: cpu_nanocores,
            },
        )]),
    }
}

fn pod_detail_history_entry() -> ResourceDetailHistoryEntry {
    ResourceDetailHistoryEntry {
        history_entry_id: 1,
        cluster_key: 1,
        api_resource: ApiResource {
            group: "core".to_owned(),
            version: "v1".to_owned(),
            kind: "Pod".to_owned(),
            name: "pods".to_owned(),
            namespaced: true,
        },
        namespace: Some("default".to_owned()),
        resource_name: "api".to_owned(),
        resource_uid: "uid".to_owned(),
        detail: None,
        events: Vec::new(),
        detail_error: None,
        events_error: None,
        managed_resources: Vec::new(),
        managed_resources_error: None,
        pod_usage: None,
        pod_usage_history: Vec::new(),
        pod_usage_missing: false,
        pod_metrics_api_unavailable: false,
        pod_usage_error: None,
        node_usage: None,
        node_usage_history: Vec::new(),
        node_metrics_api_unavailable: false,
        node_usage_error: None,
        data_editor: None,
        pending_action: None,
    }
}

mod cluster_selection;
mod editors;
mod log_state;
mod metrics;
mod workflows;

fn test_log_row(display_row: usize, text: &str) -> LogPageRow {
    LogPageRow {
        display_row,
        line_index: display_row,
        timestamp: None,
        text: text.to_owned(),
        style_spans: Vec::new(),
        match_ranges: Vec::new(),
    }
}
