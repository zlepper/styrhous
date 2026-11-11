//! A real resource watcher must resume by itself after its API disappears and
//! returns, without the user re-selecting anything.

use super::*;
use crate::api_resource::ApiResource;
use crate::cluster_connection_manager::start_resource_watcher;
use crate::worker::{
    KubernetesResourceAdded, KubernetesResourceWatchFailed, KubernetesResourceWatchReconnecting,
    KubernetesResourceWatchRecovered, KubernetesResourcesReplaced, WorkerResultBox,
    WorkerResultSender,
};
use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;
use kube::api::{ApiResource as KubeApiResource, DynamicObject, PostParams};
use std::collections::BTreeSet;
use std::time::Duration;
use tokio::sync::mpsc;

const RECOVERY_TIMEOUT: Duration = Duration::from_secs(90);

/// Removes the cluster-scoped test CRD even when an assertion fails.
struct CrdCleanup {
    runtime_handle: tokio::runtime::Handle,
    crds: Api<CustomResourceDefinition>,
    name: String,
}

impl Drop for CrdCleanup {
    fn drop(&mut self) {
        let _discovery_lock = KindDiscoveryLock::acquire();
        self.runtime_handle
            .block_on(delete_crd_and_wait(&self.crds, &self.name));
    }
}

/// Delete a CRD (if present) and wait until the API server no longer has it, so
/// recreating it cannot race with the deletion's finalizers.
async fn delete_crd_and_wait(crds: &Api<CustomResourceDefinition>, name: &str) {
    let _ = crds.delete(name, &Default::default()).await;
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    while crds.get(name).await.is_ok() {
        if std::time::Instant::now() >= deadline {
            eprintln!("Timed out waiting for CRD {name} to be deleted");
            return;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

fn crd(group: &str) -> CustomResourceDefinition {
    k8s_openapi::serde_json::from_value(k8s_openapi::serde_json::json!({
        "apiVersion": "apiextensions.k8s.io/v1",
        "kind": "CustomResourceDefinition",
        "metadata": { "name": format!("widgets.{group}") },
        "spec": {
            "group": group,
            "scope": "Namespaced",
            "names": { "plural": "widgets", "singular": "widget", "kind": "Widget" },
            "versions": [{
                "name": "v1",
                "served": true,
                "storage": true,
                "schema": { "openAPIV3Schema": {
                    "type": "object",
                    "x-kubernetes-preserve-unknown-fields": true
                } }
            }]
        }
    }))
    .expect("the test CRD is valid")
}

async fn create_crd(crds: &Api<CustomResourceDefinition>, group: &str) {
    // A CRD appearing or disappearing changes API discovery for every other
    // Kind test, so hold their discovery lock while it is being established.
    let _discovery_lock = KindDiscoveryLock::acquire();
    crds.create(&PostParams::default(), &crd(group))
        .await
        .expect("Failed to create the test CRD");
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        let established = crds
            .get(&format!("widgets.{group}"))
            .await
            .is_ok_and(|crd| {
                crd.status.is_some_and(|status| {
                    status
                        .conditions
                        .unwrap_or_default()
                        .iter()
                        .any(|condition| {
                            condition.type_ == "Established" && condition.status == "True"
                        })
                })
            });
        if established {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the test CRD was not established in time"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

async fn create_widget(client: &Client, namespace: &str, group: &str, name: &str) {
    let resource = KubeApiResource {
        group: group.to_owned(),
        version: "v1".to_owned(),
        api_version: format!("{group}/v1"),
        kind: "Widget".to_owned(),
        plural: "widgets".to_owned(),
    };
    let widgets: Api<DynamicObject> = Api::namespaced_with(client.clone(), namespace, &resource);
    // The CRD can be established before its endpoint serves the first request.
    let widget = DynamicObject::new(name, &resource).within(namespace);
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        match widgets.create(&PostParams::default(), &widget).await {
            Ok(_) => return,
            Err(error) if std::time::Instant::now() < deadline => {
                eprintln!("retrying widget creation: {error}");
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
            Err(error) => panic!("Failed to create the test widget: {error}"),
        }
    }
}

/// Collect results until `done` accepts one; returns everything seen so far.
async fn collect_until(
    receiver: &mut mpsc::Receiver<WorkerResultBox>,
    mut done: impl FnMut(&WorkerResultBox) -> bool,
) -> Vec<WorkerResultBox> {
    let mut seen = Vec::new();
    let deadline = tokio::time::Instant::now() + RECOVERY_TIMEOUT;
    loop {
        let result = tokio::time::timeout_at(deadline, receiver.recv())
            .await
            .expect("timed out waiting for the watcher")
            .expect("the watcher stopped sending results");
        let finished = done(&result);
        seen.push(result);
        if finished {
            return seen;
        }
    }
}

fn replaced_names(result: &WorkerResultBox) -> Option<Vec<String>> {
    result
        .as_ref()
        .as_any()
        .downcast_ref::<KubernetesResourcesReplaced>()
        .map(|replaced| {
            replaced
                .resources
                .iter()
                .map(|resource| resource.name.clone())
                .collect()
        })
}

#[test]
fn test_resource_watcher_resumes_after_its_api_returns() {
    let fixture = IntegrationNamespaceFixture::create("watch-recovery", "anchor", "unused");
    let group = format!("{}.example.com", fixture.namespace);
    let namespace = fixture.namespace.clone();
    let client = fixture
        .runtime
        .block_on(Client::try_default())
        .expect("Failed to create Kubernetes client");
    let crds: Api<CustomResourceDefinition> = Api::all(client.clone());
    // Declared after the fixture so the CRD is removed before the namespace.
    let _cleanup = CrdCleanup {
        runtime_handle: fixture.runtime.handle().clone(),
        crds: crds.clone(),
        name: format!("widgets.{group}"),
    };
    fixture.runtime.block_on(async {
        create_crd(&crds, &group).await;
        create_widget(&client, &namespace, &group, "first").await;

        let (sender, mut receiver) = mpsc::channel(1024);
        let api_resource = ApiResource {
            group: group.clone(),
            version: "v1".to_owned(),
            kind: "Widget".to_owned(),
            name: "widgets".to_owned(),
            namespaced: true,
        };
        let (_started, task) = start_resource_watcher(
            1,
            client.clone(),
            api_resource,
            Some(namespace.clone()),
            WorkerResultSender::new(sender, None),
            None,
        )
        .await
        .expect("the resource watcher starts");

        collect_until(&mut receiver, |result| {
            replaced_names(result).is_some_and(|names| names == ["first"])
        })
        .await;

        // The API goes away (its watch and every later list now fail) and comes
        // back with different contents, as after an API server outage.
        {
            let _discovery_lock = KindDiscoveryLock::acquire();
            delete_crd_and_wait(&crds, &format!("widgets.{group}")).await;
        }
        // Let the watcher observe a few failed attempts before the API returns.
        tokio::time::sleep(Duration::from_secs(2)).await;
        create_crd(&crds, &group).await;
        create_widget(&client, &namespace, &group, "second").await;

        // The relist may run after the CRD is established but before `second`
        // exists, so `second` can arrive as an add after an empty replacement.
        let mut names = BTreeSet::<String>::new();
        let seen = collect_until(&mut receiver, |result| {
            let any = result.as_ref().as_any();
            if let Some(replaced) = replaced_names(result) {
                names = replaced.into_iter().collect();
            } else if let Some(added) = any.downcast_ref::<KubernetesResourceAdded>() {
                names.insert(added.resource.name.clone());
            }
            names.contains("second") && !names.contains("first")
        })
        .await;
        assert!(!task.is_finished(), "the watcher keeps running");
        assert!(
            !seen.iter().any(|result| result
                .as_ref()
                .as_any()
                .is::<KubernetesResourceWatchFailed>()),
            "transient failures never terminate the watch"
        );

        // A notice, if the outage outlasted the grace period, must be cleared.
        let mut reconnecting = seen.iter().any(|result| {
            result
                .as_ref()
                .as_any()
                .is::<KubernetesResourceWatchReconnecting>()
        });
        let mut recovered = seen.iter().any(|result| {
            result
                .as_ref()
                .as_any()
                .is::<KubernetesResourceWatchRecovered>()
        });
        while let Ok(result) = receiver.try_recv() {
            let any = result.as_ref().as_any();
            reconnecting |= any.is::<KubernetesResourceWatchReconnecting>();
            recovered |= any.is::<KubernetesResourceWatchRecovered>();
        }
        assert!(
            !reconnecting || recovered,
            "a reconnecting notice is followed by a recovery"
        );
        task.abort();
    });
}
