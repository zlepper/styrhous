use super::*;

#[test]
fn resource_watch_scope_filters_all_namespaces_and_preserves_cluster_scope() {
    let all_namespaces = ResourceWatchScope::AllNamespaces(BTreeSet::from([
        "apps".to_owned(),
        "default".to_owned(),
    ]));
    assert_eq!(
        all_namespaces.event_namespace(Some("apps")),
        Some(Some("apps".to_owned()))
    );
    assert_eq!(all_namespaces.event_namespace(Some("kube-system")), None);
    assert_eq!(all_namespaces.event_namespace(None), None);

    let cluster = ResourceWatchScope::Single(None);
    assert_eq!(cluster.event_namespace(None), Some(None));
    assert_eq!(cluster.source_namespace(), None);
}

#[test]
fn resource_watch_runner_batches_filters_and_routes_events() {
    #[derive(Clone)]
    struct Item {
        uid: &'static str,
        namespace: Option<&'static str>,
    }

    let resource = |item: &Item| MinimalResource {
        uid: item.uid.to_owned(),
        name: item.uid.to_owned(),
        namespace: item.namespace.map(str::to_owned),
        creation_timestamp: None,
        controller_owner: None,
        labels: BTreeMap::new(),
        annotations: BTreeMap::new(),
        cells: BTreeMap::new(),
        log_containers: Vec::new(),
    };
    let api_resource = ApiResource {
        group: "core".to_owned(),
        version: "v1".to_owned(),
        kind: "Pod".to_owned(),
        name: "pods".to_owned(),
        namespaced: true,
    };
    let runtime = tokio::runtime::Runtime::new().expect("runtime initializes");
    runtime.block_on(async {
        let (sender, mut receiver) = tokio::sync::mpsc::channel(8);
        let (initialized, initialized_receiver) = oneshot::channel();
        run_resource_watch(
            futures_util::stream::iter(vec![
                Ok::<Event<Item>, &'static str>(Event::Init),
                Ok(Event::InitApply(Item {
                    uid: "apps-1",
                    namespace: Some("apps"),
                })),
                Ok(Event::InitApply(Item {
                    uid: "system-1",
                    namespace: Some("kube-system"),
                })),
                Ok(Event::InitDone),
                Ok(Event::Apply(Item {
                    uid: "apps-2",
                    namespace: Some("apps"),
                })),
                Ok(Event::Delete(Item {
                    uid: "apps-1",
                    namespace: Some("apps"),
                })),
            ]),
            ResourceWatchContext {
                event_sender: WorkerResultSender::new(sender, None),
                cluster_key: 4,
                api_resource: api_resource.clone(),
                scope: ResourceWatchScope::AllNamespaces(BTreeSet::from([
                    "apps".to_owned(),
                    "default".to_owned(),
                ])),
            },
            Some(initialized),
            resource,
            |item| item.namespace,
            |item| item.uid.to_owned(),
        )
        .await;

        assert!(initialized_receiver.await.is_err());
        let replaced_apps = receiver.recv().await.expect("apps replacement");
        let replaced_default = receiver.recv().await.expect("empty default replacement");
        let added = receiver.recv().await.expect("apply result");
        let deleted = receiver.recv().await.expect("delete result");

        assert!(
            replaced_apps
                .as_ref()
                .as_any()
                .downcast_ref::<KubernetesResourcesReplaced>()
                .is_some_and(|result| {
                    result.cluster_key == 4
                        && result.api_resource == api_resource
                        && result.namespace.as_deref() == Some("apps")
                        && result
                            .resources
                            .iter()
                            .map(|resource| resource.uid.as_str())
                            .eq(["apps-1"])
                })
        );
        assert!(
            replaced_default
                .as_ref()
                .as_any()
                .downcast_ref::<KubernetesResourcesReplaced>()
                .is_some_and(|result| {
                    result.namespace.as_deref() == Some("default") && result.resources.is_empty()
                })
        );
        assert!(
            added
                .as_ref()
                .as_any()
                .downcast_ref::<KubernetesResourceAdded>()
                .is_some_and(|result| result.namespace.as_deref() == Some("apps")
                    && result.resource.uid == "apps-2")
        );
        assert!(
            deleted
                .as_ref()
                .as_any()
                .downcast_ref::<KubernetesResourceDeleted>()
                .is_some_and(|result| result.namespace.as_deref() == Some("apps")
                    && result.resource_uid == "apps-1")
        );
    });
}

#[test]
fn resource_watch_runner_reports_the_source_scope_on_failure() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime initializes");
    runtime.block_on(async {
        let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
        let (initialized, initialized_receiver) = oneshot::channel();
        run_resource_watch(
            futures_util::stream::iter(vec![Err::<Event<()>, _>("connection lost")]),
            ResourceWatchContext {
                event_sender: WorkerResultSender::new(sender, None),
                cluster_key: 9,
                api_resource: ApiResource::helm_releases(),
                scope: ResourceWatchScope::Single(Some("apps".to_owned())),
            },
            Some(initialized),
            |_| unreachable!("the failed stream has no resource"),
            |_| unreachable!("the failed stream has no resource"),
            |_| unreachable!("the failed stream has no resource"),
        )
        .await;

        assert!(initialized_receiver.await.is_err());
        assert!(
            receiver
                .recv()
                .await
                .expect("watch failure result")
                .as_ref()
                .as_any()
                .downcast_ref::<KubernetesResourceWatchFailed>()
                .is_some_and(|result| {
                    result.cluster_key == 9
                        && result.namespace.as_deref() == Some("apps")
                        && result.error.contains("connection lost")
                })
        );
    });
}
