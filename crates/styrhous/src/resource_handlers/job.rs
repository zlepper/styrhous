use crate::api_resource::ApiResource;
use crate::cluster_connection_manager::{
    ResourceWatcher, TypedWatcherContext, namespaced_typed_watcher,
};
use crate::minimal_resource::{MinimalResource, from_kubernetes_resource};
use crate::resource_handlers::{matches_namespaced_api_resource, matches_namespaced_resource};
use crate::resource_table::{
    COMPLETIONS_COLUMN, CellValue, ResourceTableDefinition, STATUS_COLUMN, column, status_tone,
};
use k8s_openapi::api::batch::v1::Job;
use std::collections::BTreeMap;

pub(crate) fn watcher(context: TypedWatcherContext) -> Option<Box<dyn ResourceWatcher>> {
    matches_namespaced_resource::<Job>(&context)
        .then(|| namespaced_typed_watcher::<Job>(context, extract))
}

pub(crate) fn table_definition(api_resource: &ApiResource) -> Option<ResourceTableDefinition> {
    matches_namespaced_api_resource::<Job>(api_resource).then(|| ResourceTableDefinition {
        columns: vec![
            column(COMPLETIONS_COLUMN, "Completions", 112.0),
            column(STATUS_COLUMN, "Status", 124.0),
        ],
    })
}

const DEFAULT_BACKOFF_LIMIT: i32 = 6;

fn has_condition(job: &Job, types: &[&str]) -> bool {
    job.status
        .as_ref()
        .and_then(|status| status.conditions.as_ref())
        .into_iter()
        .flatten()
        .any(|condition| condition.status == "True" && types.contains(&condition.type_.as_str()))
}

/// Derives the display phase the way Kubernetes does: from the Job's conditions.
/// `status.failed` also counts pods retried within `backoffLimit`, so it only
/// matters here once it exceeds the limit and no condition has been written yet.
fn job_phase(job: &Job) -> &'static str {
    let spec = job.spec.as_ref();
    let status = job.status.as_ref();

    if has_condition(job, &["Failed", "FailureTarget"]) {
        return "Failed";
    }
    if has_condition(job, &["Complete", "SuccessCriteriaMet"]) {
        return "Succeeded";
    }
    if spec.and_then(|spec| spec.suspend).unwrap_or_default() || has_condition(job, &["Suspended"])
    {
        return "Suspended";
    }

    let succeeded = status
        .and_then(|status| status.succeeded)
        .unwrap_or_default();
    let active = status.and_then(|status| status.active).unwrap_or_default();
    let failed = status.and_then(|status| status.failed).unwrap_or_default();
    let backoff_limit = spec
        .and_then(|spec| spec.backoff_limit)
        .unwrap_or(DEFAULT_BACKOFF_LIMIT);
    let finished_successfully = match spec.and_then(|spec| spec.completions) {
        Some(completions) => succeeded >= completions,
        None => succeeded > 0 && active == 0,
    };
    let started =
        status.is_some_and(|status| status.start_time.is_some()) || succeeded > 0 || failed > 0;

    if finished_successfully {
        "Succeeded"
    } else if active > 0 {
        "Running"
    } else if failed > backoff_limit {
        "Failed"
    } else if started {
        "Running"
    } else {
        "Pending"
    }
}

pub(crate) fn extract(resource: &Job) -> MinimalResource {
    let succeeded = resource
        .status
        .as_ref()
        .and_then(|status| status.succeeded)
        .unwrap_or_default();
    let desired = resource
        .spec
        .as_ref()
        .and_then(|spec| spec.completions)
        .unwrap_or(1);
    let phase = job_phase(resource);
    from_kubernetes_resource(
        resource,
        BTreeMap::from([
            (
                COMPLETIONS_COLUMN.to_owned(),
                CellValue::Text(format!("{succeeded}/{desired}")),
            ),
            (
                STATUS_COLUMN.to_owned(),
                CellValue::Status {
                    label: phase.to_owned(),
                    tone: status_tone(phase),
                },
            ),
        ]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resource_table::StatusTone;
    use k8s_openapi::serde_json::{Value, json};

    fn job(spec: Value, status: Value) -> Job {
        k8s_openapi::serde_json::from_value(json!({
            "apiVersion": "batch/v1",
            "kind": "Job",
            "metadata": {"name": "nightly", "namespace": "default"},
            "spec": spec,
            "status": status,
        }))
        .unwrap()
    }

    fn cell(job: &Job, column: &str) -> CellValue {
        extract(job).cells[column].clone()
    }

    fn status_of(job: &Job) -> (String, StatusTone) {
        match cell(job, STATUS_COLUMN) {
            CellValue::Status { label, tone } => (label, tone),
            other => panic!("expected status cell, got {other:?}"),
        }
    }

    fn condition(type_: &str) -> Value {
        json!({"type": type_, "status": "True"})
    }

    #[test]
    fn succeeded_after_retry_is_succeeded() {
        let job = job(
            json!({"completions": 1}),
            json!({"failed": 1, "succeeded": 1, "conditions": [condition("Complete")]}),
        );

        assert_eq!(
            status_of(&job),
            ("Succeeded".to_owned(), StatusTone::Success)
        );
        assert_eq!(
            cell(&job, COMPLETIONS_COLUMN),
            CellValue::Text("1/1".to_owned())
        );
    }

    #[test]
    fn still_retrying_is_running() {
        let job = job(
            json!({"completions": 1, "backoffLimit": 6}),
            json!({"failed": 1, "active": 1}),
        );

        assert_eq!(status_of(&job), ("Running".to_owned(), StatusTone::Success));
    }

    #[test]
    fn waiting_between_retries_is_running() {
        let job = job(
            json!({"completions": 1, "backoffLimit": 6}),
            json!({"failed": 1, "active": 0}),
        );

        assert_eq!(status_of(&job).0, "Running");
    }

    #[test]
    fn failed_condition_is_failed() {
        let job = job(
            json!({"completions": 1, "backoffLimit": 1}),
            json!({"failed": 2, "conditions": [condition("Failed")]}),
        );

        assert_eq!(status_of(&job), ("Failed".to_owned(), StatusTone::Danger));
    }

    #[test]
    fn failure_target_is_failed_and_success_criteria_met_is_succeeded() {
        let failing = job(
            json!({}),
            json!({"active": 1, "conditions": [condition("FailureTarget")]}),
        );
        let succeeding = job(
            json!({}),
            json!({"active": 1, "conditions": [condition("SuccessCriteriaMet")]}),
        );

        assert_eq!(status_of(&failing).0, "Failed");
        assert_eq!(status_of(&succeeding).0, "Succeeded");
    }

    #[test]
    fn false_conditions_are_ignored() {
        let job = job(
            json!({"completions": 1}),
            json!({"active": 1, "conditions": [{"type": "Failed", "status": "False"}]}),
        );

        assert_eq!(status_of(&job).0, "Running");
    }

    #[test]
    fn no_pods_yet_is_pending() {
        let job = job(json!({"completions": 1}), json!({}));

        assert_eq!(status_of(&job), ("Pending".to_owned(), StatusTone::Warning));
        assert_eq!(
            cell(&job, COMPLETIONS_COLUMN),
            CellValue::Text("0/1".to_owned())
        );
    }

    #[test]
    fn suspended_spec_or_condition_is_suspended() {
        let by_spec = job(json!({"suspend": true}), json!({}));
        let by_condition = job(json!({}), json!({"conditions": [condition("Suspended")]}));

        assert_eq!(
            status_of(&by_spec),
            ("Suspended".to_owned(), StatusTone::Neutral)
        );
        assert_eq!(status_of(&by_condition).0, "Suspended");
    }

    #[test]
    fn complete_wins_over_suspend() {
        let job = job(
            json!({"suspend": true}),
            json!({"succeeded": 1, "conditions": [condition("Complete")]}),
        );

        assert_eq!(status_of(&job).0, "Succeeded");
    }

    #[test]
    fn without_conditions_succeeded_counts_win_over_failed_pods() {
        let job = job(
            json!({"completions": 2}),
            json!({"failed": 3, "succeeded": 2}),
        );

        assert_eq!(status_of(&job).0, "Succeeded");
    }

    #[test]
    fn without_conditions_exhausted_backoff_limit_is_failed() {
        let job = job(
            json!({"completions": 1, "backoffLimit": 2}),
            json!({"failed": 3}),
        );

        assert_eq!(status_of(&job).0, "Failed");
    }

    #[test]
    fn parallel_job_without_completions_is_running_while_pods_are_active() {
        let job = job(json!({"parallelism": 3}), json!({"active": 3}));

        assert_eq!(status_of(&job).0, "Running");
    }

    #[test]
    fn work_queue_job_with_active_pods_is_running_despite_a_success() {
        let job = job(
            json!({"parallelism": 3}),
            json!({"succeeded": 1, "active": 2}),
        );

        assert_eq!(status_of(&job).0, "Running");
    }

    #[test]
    fn work_queue_job_succeeds_once_no_pods_are_active() {
        let job = job(json!({"parallelism": 3}), json!({"succeeded": 3}));

        assert_eq!(status_of(&job).0, "Succeeded");
    }

    #[test]
    fn backoff_limit_boundary_without_conditions() {
        let spec = json!({"completions": 1, "backoffLimit": 2});

        assert_eq!(
            status_of(&job(spec.clone(), json!({"failed": 2}))).0,
            "Running"
        );
        assert_eq!(status_of(&job(spec, json!({"failed": 3}))).0, "Failed");
    }

    #[test]
    fn failed_condition_wins_over_succeeded_counts() {
        let job = job(
            json!({"completions": 1}),
            json!({"succeeded": 1, "conditions": [condition("Failed")]}),
        );

        assert_eq!(status_of(&job).0, "Failed");
    }

    #[test]
    fn started_job_without_pods_is_running() {
        let job = job(json!({}), json!({"startTime": "2026-01-01T00:00:00Z"}));

        assert_eq!(status_of(&job).0, "Running");
    }
}
