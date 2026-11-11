//! Watchers must survive transient stream errors: a failing stream is retried by
//! the backed-off watcher, short blips stay invisible, and only a streak that
//! outlasts the grace period is reported (and reported as recovered later).

use super::*;
use crate::cluster_connection_manager::HelmWatchContext;
use crate::resource_detail::{ResourceDetail, ResourceDetailPayload};
use futures_util::stream;
use k8s_openapi::api::core::v1::Namespace;
use std::time::Duration;
use tokio::sync::mpsc;

type Stream<T> = Vec<std::result::Result<Event<T>, &'static str>>;

/// Never reports within a test: errors are always "within the grace period".
fn patient() -> RetryTracker {
    RetryTracker::with_grace(Duration::from_secs(3600))
}

/// Reports on the first error, as if the grace period had already elapsed.
fn impatient() -> RetryTracker {
    RetryTracker::with_grace(Duration::ZERO)
}

fn channel() -> (WorkerResultSender, mpsc::Receiver<WorkerResultBox>) {
    let (sender, receiver) = mpsc::channel(256);
    (WorkerResultSender::new(sender, None), receiver)
}

fn drain(mut receiver: mpsc::Receiver<WorkerResultBox>) -> Vec<WorkerResultBox> {
    let mut results = Vec::new();
    while let Ok(result) = receiver.try_recv() {
        results.push(result);
    }
    results
}

fn is<T: 'static>(result: &WorkerResultBox) -> bool {
    result.as_ref().as_any().is::<T>()
}

fn get<T: 'static>(result: &WorkerResultBox) -> &T {
    result
        .as_ref()
        .as_any()
        .downcast_ref::<T>()
        .expect("result has the expected type")
}

fn count<T: 'static>(results: &[WorkerResultBox]) -> usize {
    results.iter().filter(|result| is::<T>(result)).count()
}

fn pod_api_resource() -> ApiResource {
    ApiResource {
        group: "core".to_owned(),
        version: "v1".to_owned(),
        kind: "Pod".to_owned(),
        name: "pods".to_owned(),
        namespaced: true,
    }
}

#[derive(Clone)]
struct Item(&'static str);

fn resource_watch_context(sender: WorkerResultSender) -> ResourceWatchContext {
    ResourceWatchContext {
        event_sender: sender,
        cluster_key: 9,
        api_resource: pod_api_resource(),
        scope: ResourceWatchScope::Single(Some("apps".to_owned())),
    }
}

async fn run_items(
    stream: Stream<Item>,
    retry: RetryTracker,
    initialized: Option<oneshot::Sender<()>>,
) -> Vec<WorkerResultBox> {
    let (sender, receiver) = channel();
    run_resource_watch(
        stream::iter(stream),
        resource_watch_context(sender),
        initialized,
        |item: &Item| MinimalResource {
            uid: item.0.to_owned(),
            name: item.0.to_owned(),
            namespace: Some("apps".to_owned()),
            creation_timestamp: None,
            controller_owner: None,
            labels: BTreeMap::new(),
            annotations: BTreeMap::new(),
            cells: BTreeMap::new(),
            log_containers: Vec::new(),
        },
        |_| Some("apps"),
        |item| item.0.to_owned(),
        retry,
    )
    .await;
    drain(receiver)
}

fn relisted_after_error() -> Stream<Item> {
    vec![
        Ok(Event::Init),
        Ok(Event::InitApply(Item("a"))),
        Ok(Event::InitDone),
        Err("connection lost"),
        Ok(Event::Init),
        Ok(Event::InitApply(Item("b"))),
        Ok(Event::InitDone),
    ]
}

#[tokio::test]
async fn resource_watch_keeps_running_after_an_error_and_relist_replaces_stale_rows() {
    let results = run_items(relisted_after_error(), patient(), None).await;

    let replaced = results
        .iter()
        .filter(|result| is::<KubernetesResourcesReplaced>(result))
        .map(|result| {
            get::<KubernetesResourcesReplaced>(result)
                .resources
                .iter()
                .map(|resource| resource.uid.clone())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    assert_eq!(replaced, [vec!["a".to_owned()], vec!["b".to_owned()]]);
    assert_eq!(count::<KubernetesResourceWatchReconnecting>(&results), 0);
    assert_eq!(count::<KubernetesResourceWatchRecovered>(&results), 0);
    assert_eq!(count::<KubernetesResourceWatchFailed>(&results), 0);
}

#[tokio::test]
async fn resource_watch_reports_a_long_streak_once_and_then_recovery() {
    let mut stream = relisted_after_error();
    stream.insert(4, Err("still lost"));
    let results = run_items(stream, impatient(), None).await;

    let reconnecting = results
        .iter()
        .filter(|result| is::<KubernetesResourceWatchReconnecting>(result))
        .collect::<Vec<_>>();
    assert_eq!(reconnecting.len(), 1, "one notice per failure streak");
    let notice = get::<KubernetesResourceWatchReconnecting>(reconnecting[0]);
    assert_eq!(notice.cluster_key, 9);
    assert_eq!(notice.namespace.as_deref(), Some("apps"));
    assert!(notice.error.contains("connection lost"));

    let kinds = results
        .iter()
        .map(|result| {
            if is::<KubernetesResourceWatchReconnecting>(result) {
                "reconnecting"
            } else if is::<KubernetesResourceWatchRecovered>(result) {
                "recovered"
            } else {
                "data"
            }
        })
        .collect::<Vec<_>>();
    let reconnecting_at = kinds.iter().position(|kind| *kind == "reconnecting");
    let recovered_at = kinds.iter().position(|kind| *kind == "recovered");
    assert!(reconnecting_at.is_some(), "the streak is reported");
    assert!(recovered_at.is_some(), "the streak recovery is reported");
    assert!(reconnecting_at < recovered_at);
}

#[tokio::test]
async fn resource_watch_releases_startup_when_the_first_list_fails() {
    let (initialized, initialized_receiver) = oneshot::channel();
    let never_ending =
        stream::iter(vec![Err::<Event<Item>, _>("connection lost")]).chain(stream::pending());
    let (sender, _receiver) = channel();
    let task = tokio::spawn(run_resource_watch(
        never_ending,
        resource_watch_context(sender),
        Some(initialized),
        |_| unreachable!("no resources are listed"),
        |_| None,
        |_| unreachable!("no resources are listed"),
        patient(),
    ));

    assert!(
        initialized_receiver.await.is_err(),
        "the startup slot is dropped instead of being held while retrying"
    );
    assert!(!task.is_finished(), "the watcher keeps retrying");
    task.abort();
}

fn namespace(name: Option<&str>) -> Namespace {
    Namespace {
        metadata: ObjectMeta {
            name: name.map(str::to_owned),
            ..Default::default()
        },
        ..Default::default()
    }
}

#[tokio::test]
async fn namespace_watch_survives_errors_nameless_deletes_and_relists() {
    let (sender, receiver) = channel();
    let events: Stream<Namespace> = vec![
        Ok(Event::Init),
        Ok(Event::InitApply(namespace(Some("old")))),
        Ok(Event::InitDone),
        Err("connection lost"),
        Err("connection lost"),
        Ok(Event::Delete(namespace(None))),
        Ok(Event::Init),
        Ok(Event::InitApply(namespace(Some("new")))),
        Ok(Event::InitDone),
    ];
    consume_namespace_stream(stream::iter(events), &sender, 3, impatient()).await;
    let results = drain(receiver);

    assert_eq!(count::<KubernetesNamespacesReconnecting>(&results), 1);
    assert_eq!(count::<KubernetesNamespacesRecovered>(&results), 1);
    assert_eq!(count::<KubernetesNamespacesDeleted>(&results), 0);
    let replaced = results
        .iter()
        .filter(|result| is::<KubernetesNamespacesReplaced>(result))
        .map(|result| {
            get::<KubernetesNamespacesReplaced>(result)
                .namespaces
                .iter()
                .map(|namespace| namespace.name.clone())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    assert_eq!(replaced, [vec!["old".to_owned()], vec!["new".to_owned()]]);
}

#[tokio::test]
async fn namespace_watch_is_silent_for_short_blips() {
    let (sender, receiver) = channel();
    let events: Stream<Namespace> = vec![
        Ok(Event::InitDone),
        Err("blip"),
        Ok(Event::Init),
        Ok(Event::InitDone),
    ];
    consume_namespace_stream(stream::iter(events), &sender, 3, patient()).await;
    let results = drain(receiver);

    assert_eq!(count::<KubernetesNamespacesReconnecting>(&results), 0);
    assert_eq!(count::<KubernetesNamespacesRecovered>(&results), 0);
    assert_eq!(count::<KubernetesNamespacesReplaced>(&results), 2);
}

fn dynamic_object(name: &str) -> DynamicObject {
    DynamicObject {
        types: None,
        metadata: ObjectMeta {
            name: Some(name.to_owned()),
            ..Default::default()
        },
        data: serde_json::json!({}),
    }
}

async fn detail_for(object: DynamicObject) -> ResourceDetail {
    ResourceDetail {
        api_resource: pod_api_resource(),
        name: object.metadata.name.unwrap_or_default(),
        namespace: None,
        uid: String::new(),
        resource_version: String::new(),
        is_deleting: false,
        finalizers: Vec::new(),
        creation_timestamp: None,
        owners: Vec::new(),
        labels: BTreeMap::new(),
        annotations: BTreeMap::new(),
        payload: ResourceDetailPayload::Generic,
    }
}

#[tokio::test]
async fn detail_object_watch_retries_reports_after_grace_and_detects_deletion_on_relist() {
    let events: Stream<DynamicObject> = vec![
        Ok(Event::Init),
        Ok(Event::InitApply(dynamic_object("web"))),
        Ok(Event::InitDone),
        Err("connection lost"),
        Ok(Event::Init),
        Ok(Event::InitDone),
    ];
    let (sender, receiver) = channel();
    consume_detail_object_stream(stream::iter(events), detail_for, 1, 7, &sender, impatient())
        .await;
    let results = drain(receiver);

    assert_eq!(count::<ResourceDetailUpdated>(&results), 1);
    let failed = results
        .iter()
        .filter(|result| is::<ResourceDetailWatchFailed>(result))
        .collect::<Vec<_>>();
    assert_eq!(failed.len(), 1);
    assert_eq!(
        get::<ResourceDetailWatchFailed>(failed[0]).stream,
        DetailStream::Object
    );
    assert_eq!(count::<ResourceDetailWatchRecovered>(&results), 1);
    assert_eq!(
        count::<ResourceDetailDeleted>(&results),
        1,
        "an object missing from the relist is reported as deleted"
    );
}

#[tokio::test]
async fn detail_object_watch_ignores_short_blips() {
    let events: Stream<DynamicObject> = vec![
        Ok(Event::InitApply(dynamic_object("web"))),
        Err("blip"),
        Ok(Event::Apply(dynamic_object("web"))),
    ];
    let (sender, receiver) = channel();
    consume_detail_object_stream(stream::iter(events), detail_for, 1, 7, &sender, patient()).await;
    let results = drain(receiver);

    assert_eq!(count::<ResourceDetailUpdated>(&results), 2);
    assert_eq!(count::<ResourceDetailWatchFailed>(&results), 0);
    assert_eq!(count::<ResourceDetailWatchRecovered>(&results), 0);
}

fn kubernetes_event(uid: &str) -> KubernetesEvent {
    KubernetesEvent {
        metadata: ObjectMeta {
            uid: Some(uid.to_owned()),
            name: Some(uid.to_owned()),
            ..Default::default()
        },
        ..Default::default()
    }
}

#[tokio::test]
async fn detail_events_watch_retries_and_relist_drops_stale_events() {
    let events: Stream<KubernetesEvent> = vec![
        Ok(Event::Init),
        Ok(Event::InitApply(kubernetes_event("old"))),
        Ok(Event::InitDone),
        Err("connection lost"),
        Ok(Event::Init),
        Ok(Event::InitApply(kubernetes_event("new"))),
        Ok(Event::InitDone),
    ];
    let (sender, receiver) = channel();
    consume_detail_events_stream(stream::iter(events), 1, 7, &sender, impatient()).await;
    let results = drain(receiver);

    let last = results
        .iter()
        .rev()
        .find(|result| is::<ResourceEventsReplaced>(result))
        .map(get::<ResourceEventsReplaced>)
        .expect("events were published");
    assert_eq!(last.events.len(), 1);
    let failed = results
        .iter()
        .find(|result| is::<ResourceDetailWatchFailed>(result))
        .map(get::<ResourceDetailWatchFailed>)
        .expect("a long streak is reported");
    assert_eq!(failed.stream, DetailStream::Events);
    assert_eq!(count::<ResourceDetailWatchRecovered>(&results), 1);
}

fn owned_pod(uid: &str) -> Pod {
    Pod {
        metadata: ObjectMeta {
            uid: Some(uid.to_owned()),
            name: Some(uid.to_owned()),
            namespace: Some("apps".to_owned()),
            owner_references: Some(vec![OwnerReference {
                controller: Some(true),
                uid: "owner".to_owned(),
                ..Default::default()
            }]),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn drain_updates(
    mut receiver: tokio::sync::mpsc::UnboundedReceiver<ManagedResourceUpdate>,
) -> Vec<ManagedResourceUpdate> {
    let mut updates = Vec::new();
    while let Ok(update) = receiver.try_recv() {
        updates.push(update);
    }
    updates
}

#[tokio::test]
async fn managed_watch_publishes_only_complete_lists_and_reports_streaks() {
    let events: Stream<Pod> = vec![
        Ok(Event::Init),
        Ok(Event::InitApply(owned_pod("old"))),
        Ok(Event::InitDone),
        Err("connection lost"),
        Ok(Event::Init),
        Ok(Event::InitApply(owned_pod("new"))),
        Ok(Event::InitDone),
    ];
    let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
    consume_managed_type_stream(
        stream::iter(events),
        &sender,
        resource_handlers::pod::extract,
        impatient(),
    )
    .await;
    drop(sender);
    let updates = drain_updates(receiver);

    let published = updates
        .iter()
        .filter_map(|update| match update {
            ManagedResourceUpdate::Replaced { resources, .. } => {
                Some(resources.iter().map(|r| r.uid.clone()).collect::<Vec<_>>())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        published,
        [vec!["old".to_owned()], vec!["new".to_owned()]],
        "Init and InitApply must not publish partial or empty lists"
    );
    assert_eq!(
        updates
            .iter()
            .filter(|update| matches!(update, ManagedResourceUpdate::Failed { .. }))
            .count(),
        1
    );
    assert_eq!(
        updates
            .iter()
            .filter(|update| matches!(update, ManagedResourceUpdate::Recovered { .. }))
            .count(),
        1
    );
}

#[tokio::test]
async fn managed_watch_is_silent_for_short_blips() {
    let events: Stream<Pod> = vec![Ok(Event::InitDone), Err("blip"), Ok(Event::InitDone)];
    let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
    consume_managed_type_stream(
        stream::iter(events),
        &sender,
        resource_handlers::pod::extract,
        patient(),
    )
    .await;
    drop(sender);
    let updates = drain_updates(receiver);

    assert_eq!(updates.len(), 2);
    assert!(
        updates
            .iter()
            .all(|update| matches!(update, ManagedResourceUpdate::Replaced { .. }))
    );
}

#[tokio::test]
async fn node_pod_watch_retries_and_publishes_only_after_initial_sync() {
    let events: Stream<Pod> = vec![
        Ok(Event::Init),
        Ok(Event::InitDone),
        Err("connection lost"),
        Ok(Event::Init),
        Ok(Event::InitDone),
    ];
    let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
    consume_node_pod_stream(
        stream::iter(events),
        "node-1".to_owned(),
        &sender,
        impatient(),
    )
    .await;
    drop(sender);
    let updates = drain_updates(receiver);

    let kinds = updates
        .iter()
        .map(|update| match update {
            ManagedResourceUpdate::Replaced { .. } => "replaced",
            ManagedResourceUpdate::Failed { .. } => "failed",
            ManagedResourceUpdate::Recovered { .. } => "recovered",
        })
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        ["replaced", "failed", "recovered", "replaced"],
        "the relist's Init does not publish an empty node"
    );
}

/// Resource stream that fails on every list attempt: kube-runtime yields
/// `Init` before each list, so an outage looks like `Err, Init, Err, Init, ...`.
fn outage<T: 'static>(attempts: usize) -> Stream<T> {
    (0..attempts)
        .flat_map(|_| [Err("list failed"), Ok(Event::Init)])
        .collect()
}

#[tokio::test]
async fn resource_watch_reports_an_outage_that_alternates_errors_and_init() {
    let mut stream = outage::<Item>(3);
    stream.push(Ok(Event::InitApply(Item("a"))));
    stream.push(Ok(Event::InitDone));
    let results = run_items(stream, impatient(), None).await;

    assert_eq!(count::<KubernetesResourceWatchReconnecting>(&results), 1);
    let recovered_after_data = results
        .iter()
        .position(is::<KubernetesResourceWatchRecovered>)
        .zip(results.iter().position(is::<KubernetesResourcesReplaced>));
    assert!(
        recovered_after_data.is_some_and(|(recovered, replaced)| recovered < replaced),
        "recovery is reported only once real data arrives"
    );
}

#[tokio::test]
async fn namespace_watch_reports_an_outage_that_alternates_errors_and_init() {
    let (sender, receiver) = channel();
    let mut events = outage::<Namespace>(3);
    events.push(Ok(Event::InitDone));
    consume_namespace_stream(stream::iter(events), &sender, 3, impatient()).await;
    let results = drain(receiver);

    assert_eq!(count::<KubernetesNamespacesReconnecting>(&results), 1);
    assert_eq!(count::<KubernetesNamespacesRecovered>(&results), 1);
}

#[tokio::test]
async fn detail_and_managed_watches_report_outages_that_alternate_errors_and_init() {
    let (sender, receiver) = channel();
    consume_detail_object_stream(
        stream::iter(outage::<DynamicObject>(3)),
        detail_for,
        1,
        7,
        &sender,
        impatient(),
    )
    .await;
    consume_detail_events_stream(
        stream::iter(outage::<KubernetesEvent>(3)),
        1,
        7,
        &sender,
        impatient(),
    )
    .await;
    let results = drain(receiver);
    assert_eq!(count::<ResourceDetailWatchFailed>(&results), 2);

    let (updates_sender, updates_receiver) = tokio::sync::mpsc::unbounded_channel();
    consume_managed_type_stream(
        stream::iter(outage::<Pod>(3)),
        &updates_sender,
        resource_handlers::pod::extract,
        impatient(),
    )
    .await;
    consume_node_pod_stream(
        stream::iter(outage::<Pod>(3)),
        "node-1".to_owned(),
        &updates_sender,
        impatient(),
    )
    .await;
    drop(updates_sender);
    let failures = drain_updates(updates_receiver)
        .iter()
        .filter(|update| matches!(update, ManagedResourceUpdate::Failed { .. }))
        .count();
    assert_eq!(failures, 2);
}

fn helm_context(sender: WorkerResultSender) -> HelmWatchContext {
    HelmWatchContext {
        cluster_key: 2,
        namespace: "apps".to_owned(),
        event_sender: sender,
    }
}

fn helm_config_map(release: &str) -> ConfigMap {
    use base64::Engine as _;
    use std::io::Write as _;
    let json = format!(
        r#"{{"name":"{release}","namespace":"apps","version":1,"info":{{"status":"deployed"}},"chart":{{"metadata":{{"name":"nginx","version":"1"}}}},"manifest":""}}"#
    );
    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gzip.write_all(json.as_bytes()).expect("gzip writes");
    let encoded = base64::engine::general_purpose::STANDARD.encode(gzip.finish().expect("gzip"));
    ConfigMap {
        metadata: ObjectMeta {
            name: Some(format!("sh.helm.release.v1.{release}.v1")),
            ..Default::default()
        },
        data: Some(BTreeMap::from([("release".to_owned(), encoded)])),
        ..Default::default()
    }
}

fn release_names(result: &WorkerResultBox) -> Vec<String> {
    get::<HelmReleasesReplaced>(result)
        .releases
        .iter()
        .map(|release| release.name.clone())
        .collect()
}

#[tokio::test]
async fn helm_watch_keeps_both_backends_running_and_clears_a_recovered_backend() {
    let (sender, receiver) = channel();
    let secrets: Stream<Secret> = vec![
        Err("secrets unavailable"),
        Ok(Event::Init),
        Ok(Event::InitDone),
    ];
    let config_maps: Stream<ConfigMap> = vec![Ok(Event::Init), Ok(Event::InitDone)];
    let (initialized, initialized_receiver) = oneshot::channel();
    consume_helm_streams(
        stream::iter(secrets),
        stream::iter(config_maps),
        &helm_context(sender),
        Some(initialized),
        [impatient(), impatient()],
    )
    .await;
    let results = drain(receiver);

    let failed = results
        .iter()
        .filter(|result| is::<HelmReleaseBackendFailed>(result))
        .map(get::<HelmReleaseBackendFailed>)
        .collect::<Vec<_>>();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].backend, "Secrets");
    assert_eq!(count::<HelmReleaseBackendRecovered>(&results), 1);
    assert!(count::<HelmReleasesReplaced>(&results) >= 1);
    assert!(
        initialized_receiver.await.is_err(),
        "startup is released once both backends have synced or failed"
    );
}

#[tokio::test]
async fn helm_watch_with_both_backends_failing_initially_still_releases_startup() {
    let (sender, receiver) = channel();
    let (initialized, initialized_receiver) = oneshot::channel();
    consume_helm_streams(
        stream::iter(outage::<Secret>(2)),
        stream::iter(outage::<ConfigMap>(2)),
        &helm_context(sender),
        Some(initialized),
        [impatient(), impatient()],
    )
    .await;
    let results = drain(receiver);

    let mut backends = results
        .iter()
        .filter(|result| is::<HelmReleaseBackendFailed>(result))
        .map(|result| get::<HelmReleaseBackendFailed>(result).backend)
        .collect::<Vec<_>>();
    backends.sort_unstable();
    assert_eq!(backends, ["ConfigMaps", "Secrets"]);
    assert!(initialized_receiver.await.is_err());
    assert!(
        results.iter().any(is::<HelmReleasesReplaced>),
        "an empty list is published so the UI stops loading"
    );
}

#[tokio::test]
async fn helm_relist_keeps_releases_visible_until_it_completes() {
    let (sender, receiver) = channel();
    let secrets: Stream<Secret> = vec![Ok(Event::Init), Ok(Event::InitDone)];
    let config_maps: Stream<ConfigMap> = vec![
        Ok(Event::Init),
        Ok(Event::InitApply(helm_config_map("keep"))),
        Ok(Event::InitApply(helm_config_map("gone"))),
        Ok(Event::InitDone),
        Err("connection lost"),
        Ok(Event::Init),
        Ok(Event::InitApply(helm_config_map("keep"))),
        Ok(Event::InitApply(helm_config_map("fresh"))),
        Ok(Event::InitDone),
    ];
    consume_helm_streams(
        stream::iter(secrets),
        stream::iter(config_maps),
        &helm_context(sender),
        None,
        [patient(), patient()],
    )
    .await;
    let results = drain(receiver);

    let lists = results
        .iter()
        .filter(|result| is::<HelmReleasesReplaced>(result))
        .map(release_names)
        .collect::<Vec<_>>();
    let last = lists.last().expect("releases were published");
    assert_eq!(last, &["fresh".to_owned(), "keep".to_owned()]);
    for list in &lists {
        assert!(
            list.contains(&"keep".to_owned()),
            "a release that survives the relist is never dropped mid-relist: {list:?}"
        );
    }
    assert!(
        lists.iter().any(|list| list.contains(&"gone".to_owned())),
        "the stale release was visible before the relist"
    );
}

#[tokio::test]
async fn helm_watch_is_silent_for_short_blips() {
    let (sender, receiver) = channel();
    let secrets: Stream<Secret> = vec![Err("blip"), Ok(Event::Init), Ok(Event::InitDone)];
    let config_maps: Stream<ConfigMap> = vec![Ok(Event::Init), Ok(Event::InitDone)];
    consume_helm_streams(
        stream::iter(secrets),
        stream::iter(config_maps),
        &helm_context(sender),
        None,
        [patient(), patient()],
    )
    .await;
    let results = drain(receiver);

    assert_eq!(count::<HelmReleaseBackendFailed>(&results), 0);
    assert_eq!(count::<HelmReleaseBackendRecovered>(&results), 0);
}

fn helm_secret(release: &str) -> Secret {
    let config_map = helm_config_map(release);
    let encoded = config_map
        .data
        .and_then(|mut data| data.remove("release"))
        .expect("the release payload exists");
    Secret {
        metadata: ObjectMeta {
            name: Some(format!("sh.helm.release.v1.{release}.v1")),
            ..Default::default()
        },
        data: Some(BTreeMap::from([(
            "release".to_owned(),
            k8s_openapi::ByteString(encoded.into_bytes()),
        )])),
        ..Default::default()
    }
}

fn from_channel<T>(receiver: mpsc::UnboundedReceiver<T>) -> impl futures_util::Stream<Item = T> {
    stream::unfold(receiver, |mut receiver| async move {
        receiver.recv().await.map(|item| (item, receiver))
    })
}

#[tokio::test]
async fn helm_backend_blip_does_not_publish_a_partial_list_before_it_relists() {
    let (sender, receiver) = channel();
    let (secret_tx, secret_rx) =
        mpsc::unbounded_channel::<std::result::Result<Event<Secret>, &str>>();
    let (config_map_tx, config_map_rx) =
        mpsc::unbounded_channel::<std::result::Result<Event<ConfigMap>, &str>>();
    let (initialized, initialized_receiver) = oneshot::channel();
    let context = helm_context(sender);
    let consumer = tokio::spawn(async move {
        consume_helm_streams(
            from_channel(secret_rx),
            from_channel(config_map_rx),
            &context,
            Some(initialized),
            [patient(), patient()],
        )
        .await;
    });

    secret_tx.send(Err("blip")).unwrap();
    assert!(
        initialized_receiver.await.is_err(),
        "startup is released on the first error"
    );
    for event in [
        Event::Init,
        Event::InitApply(helm_config_map("from-config-map")),
        Event::InitDone,
    ] {
        config_map_tx.send(Ok(event)).unwrap();
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
    for event in [
        Event::Init,
        Event::InitApply(helm_secret("from-secret")),
        Event::InitDone,
    ] {
        secret_tx.send(Ok(event)).unwrap();
    }
    drop(secret_tx);
    drop(config_map_tx);
    consumer.await.expect("the consumer finishes");

    let lists = drain(receiver)
        .iter()
        .filter(|result| is::<HelmReleasesReplaced>(result))
        .map(release_names)
        .collect::<Vec<_>>();
    assert!(!lists.is_empty());
    for list in lists {
        assert_eq!(
            list,
            ["from-config-map".to_owned(), "from-secret".to_owned()],
            "the first published list contains both backends"
        );
    }
}
