//! A Kubernetes API that accepts a request and never answers must still end in a result.

use super::*;
use std::net::SocketAddr;

/// Accepts connections and swallows requests without ever responding, like an API
/// server whose connection has stalled.
async fn stalled_api_server() -> (SocketAddr, JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("the fake API server binds");
    let address = listener.local_addr().expect("the listener has an address");
    let server = tokio::spawn(async move {
        let mut held_open = Vec::new();
        while let Ok((socket, _)) = listener.accept().await {
            held_open.push(socket);
        }
    });
    (address, server)
}

/// A worker state connected as cluster 1 to `address`, plus the receiving end of its results.
async fn connected_state(
    address: SocketAddr,
    request_deadline: Duration,
) -> (WorkerState, mpsc::Receiver<WorkerResultBox>) {
    let (sender, receiver) = mpsc::channel(8);
    let mut state = WorkerState::new(WorkerResultSender::new(sender, None), None);
    state.request_deadline = request_deadline;
    let config = kube::Config::new(format!("http://{address}").parse().expect("valid URL"));
    state
        .connections
        .lock()
        .await
        .insert(1, ClusterConnection::for_test(1, config));
    (state, receiver)
}

/// The one result `command` produces, which must arrive within `patience`.
async fn result_of(
    command: WorkerCommandBox,
    state: WorkerState,
    mut receiver: mpsc::Receiver<WorkerResultBox>,
    patience: Duration,
) -> WorkerResultBox {
    tokio::spawn(dispatch_command(command, Arc::new(state)));
    tokio::time::timeout(patience, receiver.recv())
        .await
        .expect("the command must report a result instead of waiting for the API forever")
        .expect("the result channel stays open")
}

fn failure_of<S: 'static, E: 'static + std::fmt::Debug>(result: &WorkerResultBox) -> &E {
    match result
        .as_ref()
        .as_any()
        .downcast_ref::<Result<S, E>>()
        .expect("the command's result type")
    {
        Ok(_) => panic!("a stalled API cannot succeed"),
        Err(failure) => failure,
    }
}

const MUTATION_NOTE: &str = "may or may not have been applied";

/// One wrapped command, and what its failure must identify.
struct Case {
    command: WorkerCommandBox,
    /// Whether giving up leaves the outcome unknown.
    mutates: bool,
    /// Checks the failure's type and identifying fields, and returns its message.
    failure: fn(&WorkerResultBox) -> String,
}

fn case<C: WorkerCommand>(
    command: C,
    mutates: bool,
    failure: fn(&WorkerResultBox) -> String,
) -> Case {
    Case {
        command: Box::new(command),
        mutates,
        failure,
    }
}

fn cases() -> Vec<Case> {
    let namespace = || Some("default".to_owned());
    let name = || "target".to_owned();
    vec![
        case(
            GetResourceYaml {
                editor_id: 3,
                cluster_key: 1,
                api_resource: pod_resource(),
                namespace: namespace(),
                resource_name: name(),
            },
            false,
            |result| {
                let failure = failure_of::<ResourceYamlFetched, ResourceYamlFetchFailed>(result);
                assert_eq!(failure.editor_id, 3);
                failure.error.clone()
            },
        ),
        case(
            LoadResourceSchema {
                editor_id: 4,
                cluster_key: 1,
                api_resource: pod_resource(),
            },
            false,
            |result| {
                let failure = failure_of::<ResourceSchemaLoaded, ResourceSchemaLoadFailed>(result);
                assert_eq!(failure.editor_id, 4);
                failure.error.clone()
            },
        ),
        case(
            DeleteResource {
                cluster_key: 1,
                api_resource: pod_resource(),
                namespace: namespace(),
                resource_name: name(),
                resource_uid: Some("uid".to_owned()),
                bulk_delete_id: Some(8),
            },
            true,
            |result| {
                let failure = failure_of::<ResourceDeleteCompleted, ResourceDeleteFailed>(result);
                assert_eq!(failure.bulk_delete_id, Some(8));
                assert_eq!(failure.resource_name, "target");
                failure.error.clone()
            },
        ),
        case(
            ForceDeleteResource {
                cluster_key: 1,
                api_resource: pod_resource(),
                namespace: namespace(),
                resource_name: name(),
                resource_uid: "uid".to_owned(),
            },
            true,
            |result| {
                let failure =
                    failure_of::<ResourceForceDeleteCompleted, ResourceForceDeleteFailed>(result);
                assert_eq!(failure.resource_name, "target");
                failure.error.clone()
            },
        ),
        case(
            RestartDeployment {
                cluster_key: 1,
                namespace: "default".to_owned(),
                resource_name: name(),
            },
            true,
            |result| {
                let failure =
                    failure_of::<DeploymentRestartCompleted, DeploymentRestartFailed>(result);
                assert_eq!(failure.resource_name, "target");
                failure.error.clone()
            },
        ),
        case(
            RunCronJob {
                cluster_key: 1,
                operation_id: 5,
                namespace: "default".to_owned(),
                resource_name: name(),
            },
            true,
            |result| {
                let failure = failure_of::<CronJobRunCompleted, CronJobRunFailed>(result);
                assert_eq!(
                    (failure.operation_id, failure.cron_job_name.as_str()),
                    (5, "target")
                );
                failure.error.clone()
            },
        ),
        case(
            GetDeploymentRevisions {
                cluster_key: 1,
                request_id: 9,
                namespace: "default".to_owned(),
                resource_name: name(),
            },
            false,
            |result| {
                let failure = failure_of::<
                    DeploymentRevisionsFetched,
                    DeploymentRevisionsFetchFailed,
                >(result);
                assert_eq!((failure.cluster_key, failure.request_id), (1, 9));
                failure.error.clone()
            },
        ),
        case(
            RollbackDeployment {
                cluster_key: 1,
                operation_id: 6,
                namespace: "default".to_owned(),
                resource_name: name(),
                revision: 2,
            },
            true,
            |result| {
                let failure = failure_of::<DeploymentRolledBack, DeploymentRollbackFailed>(result);
                assert_eq!((failure.operation_id, failure.revision), (6, 2));
                failure.error.clone()
            },
        ),
        case(
            GetResourceScale {
                cluster_key: 1,
                request_id: 11,
                api_resource: pod_resource(),
                namespace: namespace(),
                resource_name: name(),
            },
            false,
            |result| {
                let failure = failure_of::<ResourceScaleFetched, ResourceScaleFetchFailed>(result);
                assert_eq!(failure.request_id, 11);
                failure.error.clone()
            },
        ),
        case(
            UpdateResourceScale {
                cluster_key: 1,
                api_resource: pod_resource(),
                namespace: namespace(),
                resource_name: name(),
                replicas: 3,
            },
            true,
            |result| {
                let failure = failure_of::<ResourceScaleUpdated, ResourceScaleUpdateFailed>(result);
                assert_eq!(failure.resource_name, "target");
                failure.error.clone()
            },
        ),
        case(
            ApplyResourceYaml {
                editor_id: 12,
                cluster_key: 1,
                api_resource: pod_resource(),
                namespace: namespace(),
                resource_name: name(),
                original_yaml: String::new(),
                yaml: String::new(),
                resource_version: "1".to_owned(),
                resource_uid: "uid".to_owned(),
            },
            true,
            |result| {
                let ResourceYamlApplyFailure::Command(failure) =
                    failure_of::<ResourceApplyCompleted, ResourceYamlApplyFailure>(result)
                else {
                    panic!("a timeout is a command failure, not an API status");
                };
                assert_eq!(
                    (failure.editor_id, failure.resource_name.as_str()),
                    (12, "target")
                );
                failure.error.clone()
            },
        ),
        case(
            ValidateResourceYaml {
                editor_id: 13,
                revision: 4,
                cluster_key: 1,
                api_resource: pod_resource(),
                namespace: namespace(),
                resource_name: name(),
                original_yaml: String::new(),
                yaml: String::new(),
                resource_version: "1".to_owned(),
                resource_uid: "uid".to_owned(),
            },
            false,
            |result| {
                let ResourceYamlValidationFailure::Command(failure) =
                    failure_of::<ResourceYamlValidated, ResourceYamlValidationFailure>(result)
                else {
                    panic!("a timeout is a command failure, not an API status");
                };
                assert_eq!((failure.editor_id, failure.revision), (13, 4));
                failure.error.clone()
            },
        ),
        case(
            UpdateResourceData {
                cluster_key: 1,
                history_entry_id: 14,
                request_id: 15,
                api_resource: ApiResource {
                    group: "core".to_owned(),
                    version: "v1".to_owned(),
                    kind: "ConfigMap".to_owned(),
                    name: "configmaps".to_owned(),
                    namespaced: true,
                },
                namespace: "default".to_owned(),
                resource_name: name(),
                update: ResourceDataUpdate {
                    expected_resource_version: "1".to_owned(),
                    expected_values: BTreeMap::from([("key".to_owned(), "old".to_owned())]),
                    updated_values: BTreeMap::from([("key".to_owned(), "new".to_owned())]),
                },
            },
            true,
            |result| {
                let failure =
                    failure_of::<ResourceDataUpdateCompleted, ResourceDataUpdateFailed>(result);
                assert_eq!((failure.history_entry_id, failure.request_id), (14, 15));
                failure.error.clone()
            },
        ),
        case(
            CordonNode {
                cluster_key: 1,
                operation_id: 16,
                node_name: "node-1".to_owned(),
                cordon: true,
            },
            true,
            |result| {
                let failure = failure_of::<NodeCordonCompleted, NodeCordonFailed>(result);
                assert_eq!(
                    (failure.operation_id, failure.node_name.as_str()),
                    (16, "node-1")
                );
                assert!(failure.cordon);
                failure.error.clone()
            },
        ),
    ]
}

#[tokio::test]
async fn every_wrapped_command_reports_a_typed_failure_when_the_api_never_answers() {
    let (address, server) = stalled_api_server().await;
    for case in cases() {
        let name = case.command.command_name();
        let (state, receiver) = connected_state(address, Duration::from_millis(100)).await;

        let result = result_of(case.command, state, receiver, Duration::from_secs(5)).await;

        let error = (case.failure)(&result);
        assert!(error.contains("no response"), "{name}: {error}");
        assert_eq!(
            error.contains(MUTATION_NOTE),
            case.mutates,
            "{name}: {error}"
        );
    }
    server.abort();
}

#[tokio::test]
async fn a_command_for_a_cluster_that_is_not_connected_reports_why() {
    let (sender, mut receiver) = mpsc::channel(1);
    let state = Arc::new(WorkerState::new(
        WorkerResultSender::new(sender, None),
        None,
    ));
    let command = GetResourceYaml {
        editor_id: 1,
        cluster_key: 99,
        api_resource: pod_resource(),
        namespace: None,
        resource_name: "pod".to_owned(),
    };

    dispatch_command(Box::new(command), state).await;

    let result = receiver.try_recv().expect("a failure is reported");
    let failure = failure_of::<ResourceYamlFetched, ResourceYamlFetchFailed>(&result);
    assert!(
        failure.error.contains("No client found"),
        "{}",
        failure.error
    );
}

#[tokio::test(start_paused = true)]
async fn the_real_deadline_allows_44_seconds_and_stops_waiting_after_45() {
    let state = worker_state();
    let answer_after = |seconds| async move {
        tokio::time::sleep(Duration::from_secs(seconds)).await;
        Ok::<_, anyhow::Error>(seconds)
    };

    let in_time = state
        .within_deadline(ApiCall::read("Getting YAML"), answer_after(44))
        .await;
    let too_late = state
        .within_deadline(ApiCall::read("Getting YAML"), answer_after(46))
        .await;

    assert_eq!(in_time.expect("44 s is within the deadline"), 44);
    let error = too_late.expect_err("46 s is not").to_string();
    assert!(error.contains("within 45s"), "{error}");
    assert!(!error.contains(MUTATION_NOTE), "{error}");
}

#[tokio::test(start_paused = true)]
async fn patient_calls_get_three_times_the_deadline_and_mutations_say_the_outcome_is_unknown() {
    let state = worker_state();
    let answer_after = |seconds| async move {
        tokio::time::sleep(Duration::from_secs(seconds)).await;
        Ok::<_, anyhow::Error>(())
    };

    state
        .within_deadline(ApiCall::read("Loading").patient(), answer_after(134))
        .await
        .expect("a patient call waits 135 s");
    let error = state
        .within_deadline(ApiCall::mutation("Deleting"), answer_after(46))
        .await
        .expect_err("a mutation gives up after 45 s")
        .to_string();

    assert!(error.starts_with("Deleting got no response"), "{error}");
    assert!(error.contains(MUTATION_NOTE), "{error}");
}

#[tokio::test(start_paused = true)]
async fn errors_pass_through_and_a_timed_out_request_is_dropped() {
    struct DropFlag(Arc<AtomicUsize>);
    impl Drop for DropFlag {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }
    let state = worker_state();

    let failed = state
        .within_deadline(ApiCall::read("Getting YAML"), async {
            anyhow::bail!("forbidden") as anyhow::Result<()>
        })
        .await
        .expect_err("the request's own error is kept")
        .to_string();
    let dropped = Arc::new(AtomicUsize::new(0));
    let flag = DropFlag(dropped.clone());
    let timed_out = state
        .within_deadline(ApiCall::read("Getting YAML"), async move {
            let _flag = flag;
            std::future::pending::<anyhow::Result<()>>().await
        })
        .await;

    assert_eq!(failed, "forbidden");
    assert!(timed_out.is_err());
    assert_eq!(
        dropped.load(Ordering::Relaxed),
        1,
        "the request was abandoned"
    );
}
