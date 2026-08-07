use crate::cluster_connection_manager::minimal_resource_from_typed;
use crate::cluster_connection_manager::{
    ResourceWatcher, TypedWatcherContext, namespaced_typed_watcher,
};
use crate::minimal_resource::MinimalResource;
use crate::resource_handlers::{matches_namespaced_api_resource, matches_namespaced_resource};
use crate::resource_table::{
    CONTAINERS_COLUMN, CellValue, ContainerIndicator, ContainerKind, READY_COLUMN, RESTARTS_COLUMN,
    ResourceTableDefinition, STATUS_COLUMN, StatusTone, column, status_tone,
};
use k8s_openapi::api::core::v1::{ContainerStatus, Pod};
use std::collections::BTreeMap;

pub(crate) fn watcher(context: TypedWatcherContext) -> Option<Box<dyn ResourceWatcher>> {
    matches_namespaced_resource::<Pod>(&context)
        .then(|| namespaced_typed_watcher::<Pod>(context, extract))
}

pub(crate) fn table_definition(api_resource: &ApiResource) -> Option<ResourceTableDefinition> {
    matches_namespaced_api_resource::<Pod>(api_resource).then(|| ResourceTableDefinition {
        columns: vec![
            column(READY_COLUMN, "Ready", 90.0),
            column(CONTAINERS_COLUMN, "Containers", 150.0),
            column(STATUS_COLUMN, "Status", 128.0),
            column(RESTARTS_COLUMN, "Restarts", 120.0),
        ],
    })
}

pub(crate) fn extract(pod: &Pod) -> MinimalResource {
    let status = pod.status.as_ref();
    let containers = status.and_then(|status| status.container_statuses.as_ref());
    let total = containers.map_or(0, Vec::len);
    let ready = containers
        .map(|containers| {
            containers
                .iter()
                .filter(|container| container.ready)
                .count()
        })
        .unwrap_or(0);
    let restarts = containers
        .map(|containers| {
            containers
                .iter()
                .map(|container| i64::from(container.restart_count))
                .sum()
        })
        .unwrap_or(0);
    let phase = status
        .and_then(|status| status.phase.as_deref())
        .unwrap_or("Unknown");
    let indicators = status.map(container_indicators).unwrap_or_default();

    minimal_resource_from_typed(
        pod,
        BTreeMap::from([
            (
                READY_COLUMN.to_owned(),
                CellValue::Text(format!("{ready}/{total}")),
            ),
            (
                CONTAINERS_COLUMN.to_owned(),
                CellValue::ContainerIndicators(indicators),
            ),
            (
                STATUS_COLUMN.to_owned(),
                CellValue::Status {
                    label: phase.to_owned(),
                    tone: status_tone(phase),
                },
            ),
            (RESTARTS_COLUMN.to_owned(), CellValue::Number(restarts)),
        ]),
    )
}

fn container_indicators(status: &k8s_openapi::api::core::v1::PodStatus) -> Vec<ContainerIndicator> {
    let mut indicators = Vec::new();
    append_container_indicators(
        &mut indicators,
        status.init_container_statuses.as_deref(),
        ContainerKind::Init,
    );
    append_container_indicators(
        &mut indicators,
        status.container_statuses.as_deref(),
        ContainerKind::App,
    );
    append_container_indicators(
        &mut indicators,
        status.ephemeral_container_statuses.as_deref(),
        ContainerKind::Ephemeral,
    );
    indicators
}

fn append_container_indicators(
    indicators: &mut Vec<ContainerIndicator>,
    statuses: Option<&[ContainerStatus]>,
    kind: ContainerKind,
) {
    indicators.extend(
        statuses
            .unwrap_or_default()
            .iter()
            .map(|status| container_indicator(status, kind)),
    );
}

fn container_indicator(status: &ContainerStatus, kind: ContainerKind) -> ContainerIndicator {
    let (state, reason, message, tone) = match status.state.as_ref() {
        Some(state) if state.running.is_some() => (
            "Running".to_owned(),
            None,
            None,
            if status.ready {
                StatusTone::Success
            } else {
                StatusTone::Warning
            },
        ),
        Some(state) if state.waiting.is_some() => {
            let waiting = state.waiting.as_ref().expect("checked waiting state");
            (
                "Waiting".to_owned(),
                waiting.reason.clone(),
                waiting.message.clone(),
                StatusTone::Warning,
            )
        }
        Some(state) if state.terminated.is_some() => {
            let terminated = state.terminated.as_ref().expect("checked terminated state");
            (
                "Terminated".to_owned(),
                terminated.reason.clone(),
                terminated.message.clone(),
                if terminated.exit_code == 0 {
                    StatusTone::Success
                } else {
                    StatusTone::Danger
                },
            )
        }
        _ => ("Unknown".to_owned(), None, None, StatusTone::Neutral),
    };

    ContainerIndicator {
        name: status.name.clone(),
        kind,
        state,
        reason,
        message,
        ready: status.ready,
        restart_count: status.restart_count,
        tone,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::api::core::v1::{
        ContainerState, ContainerStateRunning, ContainerStateTerminated, ContainerStateWaiting,
        PodStatus,
    };

    fn container_status(
        name: &str,
        state: ContainerState,
        ready: bool,
        restart_count: i32,
    ) -> ContainerStatus {
        ContainerStatus {
            name: name.to_owned(),
            state: Some(state),
            ready,
            restart_count,
            ..Default::default()
        }
    }

    #[test]
    fn extract_includes_all_container_categories_with_state_aware_tones() {
        let pod = Pod {
            status: Some(PodStatus {
                init_container_statuses: Some(vec![container_status(
                    "setup",
                    ContainerState {
                        terminated: Some(ContainerStateTerminated {
                            exit_code: 0,
                            reason: Some("Completed".to_owned()),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                    false,
                    0,
                )]),
                container_statuses: Some(vec![
                    container_status(
                        "api",
                        ContainerState {
                            running: Some(ContainerStateRunning::default()),
                            ..Default::default()
                        },
                        true,
                        2,
                    ),
                    container_status(
                        "worker",
                        ContainerState {
                            running: Some(ContainerStateRunning::default()),
                            ..Default::default()
                        },
                        false,
                        0,
                    ),
                    container_status(
                        "sidecar",
                        ContainerState {
                            waiting: Some(ContainerStateWaiting {
                                reason: Some("ContainerCreating".to_owned()),
                                message: Some("Waiting for volume mount".to_owned()),
                            }),
                            ..Default::default()
                        },
                        false,
                        3,
                    ),
                ]),
                ephemeral_container_statuses: Some(vec![container_status(
                    "debugger",
                    ContainerState {
                        terminated: Some(ContainerStateTerminated {
                            exit_code: 1,
                            reason: Some("Error".to_owned()),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                    false,
                    1,
                )]),
                ..Default::default()
            }),
            ..Default::default()
        };

        let resource = extract(&pod);
        let CellValue::ContainerIndicators(indicators) = resource
            .cells
            .get(CONTAINERS_COLUMN)
            .expect("containers cell should exist")
        else {
            panic!("containers cell should contain indicators");
        };

        assert_eq!(
            indicators
                .iter()
                .map(|indicator| (indicator.name.as_str(), indicator.kind, indicator.tone))
                .collect::<Vec<_>>(),
            vec![
                ("setup", ContainerKind::Init, StatusTone::Success),
                ("api", ContainerKind::App, StatusTone::Success),
                ("worker", ContainerKind::App, StatusTone::Warning),
                ("sidecar", ContainerKind::App, StatusTone::Warning),
                ("debugger", ContainerKind::Ephemeral, StatusTone::Danger),
            ]
        );
        assert_eq!(
            resource.cells.get(READY_COLUMN),
            Some(&CellValue::Text("1/3".to_owned()))
        );
        assert_eq!(
            resource.cells.get(RESTARTS_COLUMN),
            Some(&CellValue::Number(5))
        );
        assert_eq!(indicators[3].reason.as_deref(), Some("ContainerCreating"));
        assert_eq!(
            indicators[3].message.as_deref(),
            Some("Waiting for volume mount")
        );
    }

    #[test]
    fn container_without_a_reported_state_is_neutral() {
        let indicator = container_indicator(
            &ContainerStatus {
                name: "api".to_owned(),
                ..Default::default()
            },
            ContainerKind::App,
        );

        assert_eq!(indicator.state, "Unknown");
        assert_eq!(indicator.tone, StatusTone::Neutral);
    }
}
use crate::api_resource::ApiResource;
