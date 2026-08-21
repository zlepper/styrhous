use super::*;

#[test]
fn replacing_a_resource_watch_aborts_the_previous_task() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime initializes");
    runtime.block_on(async {
        let state = worker_state();
        let aborted = Arc::new(AtomicUsize::new(0));
        let key = ResourceScope {
            cluster_key: 1,
            api_resource: pod_resource(),
            namespace: Some("default".to_owned()),
        };

        state
            .replace_resource_watch(key.clone(), tokio::spawn(AbortProbe(aborted.clone())))
            .await;
        state
            .replace_resource_watch(key, tokio::spawn(std::future::pending()))
            .await;
        tokio::task::yield_now().await;

        assert_eq!(aborted.load(Ordering::Relaxed), 1);
    });
}

#[test]
fn reconciliation_or_cluster_teardown_prevents_a_queued_start_from_installing() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime initializes");
    runtime.block_on(async {
        let state = worker_state();
        let resource = pod_resource();
        let obsolete_generation = state.replace_resource_watch_sources(1, &resource).await;
        let current_generation = state.replace_resource_watch_sources(1, &resource).await;
        let session = state.resource_watch_session(1).await;
        let aborted = Arc::new(AtomicUsize::new(0));

        assert!(
            !state
                .install_resource_watch_if_current(
                    ResourceScope {
                        cluster_key: 1,
                        api_resource: resource,
                        namespace: Some("default".to_owned()),
                    },
                    obsolete_generation,
                    session,
                    tokio::spawn(AbortProbe(aborted.clone())),
                )
                .await
        );
        tokio::task::yield_now().await;

        assert_eq!(current_generation, obsolete_generation + 1);
        assert_eq!(aborted.load(Ordering::Relaxed), 1);
        assert!(state.resource_watches.lock().await.watches.is_empty());

        state.invalidate_cluster_resource_watches(1).await;
        assert!(
            !state
                .resource_watch_generation_is_current(
                    1,
                    &pod_resource(),
                    current_generation,
                    session
                )
                .await
        );
    });
}

#[test]
fn replacing_a_pod_metrics_watch_aborts_the_previous_task() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime initializes");
    runtime.block_on(async {
        let state = worker_state();
        let aborted = Arc::new(AtomicUsize::new(0));
        let key = (1, "default".to_owned());

        state
            .replace_pod_metrics_watch(key.clone(), tokio::spawn(AbortProbe(aborted.clone())))
            .await;
        state
            .replace_pod_metrics_watch(key, tokio::spawn(std::future::pending()))
            .await;
        tokio::task::yield_now().await;

        assert_eq!(aborted.load(Ordering::Relaxed), 1);
    });
}

#[test]
fn replacing_a_node_metrics_watch_aborts_the_previous_task() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime initializes");
    runtime.block_on(async {
        let state = worker_state();
        let aborted = Arc::new(AtomicUsize::new(0));

        state
            .replace_node_metrics_watch(1, tokio::spawn(AbortProbe(aborted.clone())))
            .await;
        state
            .replace_node_metrics_watch(1, tokio::spawn(std::future::pending()))
            .await;
        tokio::task::yield_now().await;

        assert_eq!(aborted.load(Ordering::Relaxed), 1);
    });
}

#[test]
fn stopping_all_clusters_aborts_supervised_tasks() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime initializes");
    runtime.block_on(async {
        let state = worker_state();
        let resource_aborted = Arc::new(AtomicUsize::new(0));
        let detail_aborted = Arc::new(AtomicUsize::new(0));
        let metrics_aborted = Arc::new(AtomicUsize::new(0));
        let node_metrics_aborted = Arc::new(AtomicUsize::new(0));
        let log_aborted = Arc::new(AtomicUsize::new(0));

        state
            .replace_resource_watch(
                ResourceScope {
                    cluster_key: 1,
                    api_resource: pod_resource(),
                    namespace: Some("default".to_owned()),
                },
                tokio::spawn(AbortProbe(resource_aborted.clone())),
            )
            .await;
        state
            .detail_watches
            .replace((1, 3), tokio::spawn(AbortProbe(detail_aborted.clone())))
            .await;
        state
            .pod_metrics_watches
            .replace(
                (1, "default".to_owned()),
                tokio::spawn(AbortProbe(metrics_aborted.clone())),
            )
            .await;
        state
            .node_metrics_watches
            .replace(1, tokio::spawn(AbortProbe(node_metrics_aborted.clone())))
            .await;
        state
            .log_streams
            .replace((1, 4), tokio::spawn(AbortProbe(log_aborted.clone())))
            .await;

        state.stop_all_clusters().await;
        tokio::task::yield_now().await;

        assert_eq!(resource_aborted.load(Ordering::Relaxed), 1);
        assert_eq!(detail_aborted.load(Ordering::Relaxed), 1);
        assert_eq!(metrics_aborted.load(Ordering::Relaxed), 1);
        assert_eq!(node_metrics_aborted.load(Ordering::Relaxed), 1);
        assert_eq!(log_aborted.load(Ordering::Relaxed), 1);
        assert!(state.resource_watches.lock().await.watches.is_empty());
        assert!(state.detail_watches.is_empty().await);
        assert!(state.pod_metrics_watches.is_empty().await);
        assert!(state.node_metrics_watches.is_empty().await);
        assert!(state.log_streams.is_empty().await);
    });
}

#[test]
fn stopping_one_cluster_preserves_other_cluster_tasks() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime initializes");
    runtime.block_on(async {
        let state = worker_state();
        let first_aborted = Arc::new(AtomicUsize::new(0));
        let second_aborted = Arc::new(AtomicUsize::new(0));
        let first_metrics_aborted = Arc::new(AtomicUsize::new(0));
        let second_metrics_aborted = Arc::new(AtomicUsize::new(0));
        let first_node_metrics_aborted = Arc::new(AtomicUsize::new(0));
        let second_node_metrics_aborted = Arc::new(AtomicUsize::new(0));
        let first_detail_aborted = Arc::new(AtomicUsize::new(0));
        let second_detail_aborted = Arc::new(AtomicUsize::new(0));
        let first_log_aborted = Arc::new(AtomicUsize::new(0));
        let second_log_aborted = Arc::new(AtomicUsize::new(0));
        state
            .replace_resource_watch(
                ResourceScope {
                    cluster_key: 1,
                    api_resource: pod_resource(),
                    namespace: Some("default".to_owned()),
                },
                tokio::spawn(AbortProbe(first_aborted.clone())),
            )
            .await;
        state
            .replace_resource_watch(
                ResourceScope {
                    cluster_key: 2,
                    api_resource: pod_resource(),
                    namespace: Some("default".to_owned()),
                },
                tokio::spawn(AbortProbe(second_aborted.clone())),
            )
            .await;

        state
            .replace_pod_metrics_watch(
                (1, "default".to_owned()),
                tokio::spawn(AbortProbe(first_metrics_aborted.clone())),
            )
            .await;
        state
            .replace_node_metrics_watch(
                1,
                tokio::spawn(AbortProbe(first_node_metrics_aborted.clone())),
            )
            .await;
        state
            .replace_node_metrics_watch(
                2,
                tokio::spawn(AbortProbe(second_node_metrics_aborted.clone())),
            )
            .await;
        state
            .replace_pod_metrics_watch(
                (2, "default".to_owned()),
                tokio::spawn(AbortProbe(second_metrics_aborted.clone())),
            )
            .await;

        state
            .detail_watches
            .replace(
                (1, 10),
                tokio::spawn(AbortProbe(first_detail_aborted.clone())),
            )
            .await;
        state
            .detail_watches
            .replace(
                (2, 10),
                tokio::spawn(AbortProbe(second_detail_aborted.clone())),
            )
            .await;
        state
            .log_streams
            .replace((1, 11), tokio::spawn(AbortProbe(first_log_aborted.clone())))
            .await;
        state
            .log_streams
            .replace(
                (2, 11),
                tokio::spawn(AbortProbe(second_log_aborted.clone())),
            )
            .await;

        state.stop_cluster(1).await;

        assert_eq!(first_aborted.load(Ordering::Relaxed), 1);
        assert_eq!(second_aborted.load(Ordering::Relaxed), 0);
        assert_eq!(first_metrics_aborted.load(Ordering::Relaxed), 1);
        assert_eq!(second_metrics_aborted.load(Ordering::Relaxed), 0);
        assert_eq!(first_node_metrics_aborted.load(Ordering::Relaxed), 1);
        assert_eq!(second_node_metrics_aborted.load(Ordering::Relaxed), 0);
        assert_eq!(first_detail_aborted.load(Ordering::Relaxed), 1);
        assert_eq!(second_detail_aborted.load(Ordering::Relaxed), 0);
        assert_eq!(first_log_aborted.load(Ordering::Relaxed), 1);
        assert_eq!(second_log_aborted.load(Ordering::Relaxed), 0);
        assert!(
            state
                .resource_watches
                .lock()
                .await
                .watches
                .contains_key(&ResourceScope {
                    cluster_key: 2,
                    api_resource: pod_resource(),
                    namespace: Some("default".to_owned()),
                })
        );
        assert!(
            state
                .pod_metrics_watches
                .contains_key(&(2, "default".to_owned()))
                .await
        );
        assert!(state.node_metrics_watches.contains_key(&2).await);
        assert!(state.detail_watches.contains_key(&(2, 10)).await);
        assert!(state.log_streams.contains_key(&(2, 11)).await);
    });
}
