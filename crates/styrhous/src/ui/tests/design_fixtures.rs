//! The Harbour design dataset (boards A01–A16), shared by the screen oracle
//! tests: the four kubeconfig contexts, the `kind-kind` cluster with its
//! `kube-system` workloads and nodes, and a fixed clock so ages read as the
//! boards print them ("220d", "3h", "14m").
//!
//! Everything reaches the app the way a session would: the seeded state is
//! what the app holds after connecting and discovering the cluster, and the
//! objects arrive as worker results built from real Kubernetes objects by the
//! app's own resource handlers.

use super::super::clock::WallClock;
use super::super::state::{ClusterConnectionState, ClusterState, SelectedNamespaces, UiState};
use super::fixtures::{AppHarness, application_harness};
use super::harness::MockUiHarnessExt;
use crate::api_resource::ApiResource;
use crate::helm_release::{HelmRelease, StorageDriver};
use crate::minimal_namespace::MinimalNamespace;
use crate::minimal_resource::MinimalResource;
use crate::pod_metrics::{NodeUsage, PodUsage};
use crate::resource_catalog::build_resource_navigation;
use crate::worker::{
    CronJobRunFailed, HelmReleasesReplaced, KubernetesResourcesReplaced, MockWorker,
    NodeMetricsUpdated, NodePodCountsUpdated, PodMetricsUpdated, ResourceScaleUpdated,
};
use k8s_openapi::api::core::v1::{Node, Pod};
use k8s_openapi::serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use time::{Duration, OffsetDateTime};

/// "Now" on every board: 1 June 2026, 12:00 UTC.
pub(in crate::ui) fn design_now() -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(1_780_315_200).expect("a valid timestamp")
}

/// The four kubeconfig contexts, in the order the cluster card counts them.
pub(super) const DESIGN_CONTEXTS: [&str; 4] = ["dev-eu", "kind-kind", "kind-legacy", "prod-eu"];
/// The key of the selected `kind-kind` context.
pub(super) const KIND_KIND: i32 = 2;
/// The namespace the boards show.
pub(super) const KUBE_SYSTEM: &str = "kube-system";
/// The namespaces of `kind-kind` (the sidebar's "Namespaces 5").
const NAMESPACES: [&str; 5] = [
    "default",
    "kube-node-lease",
    "kube-public",
    "kube-system",
    "local-path-storage",
];

fn api_resource(
    group: &str,
    version: &str,
    kind: &str,
    name: &str,
    namespaced: bool,
) -> ApiResource {
    ApiResource {
        group: group.into(),
        version: version.into(),
        kind: kind.into(),
        name: name.into(),
        namespaced,
    }
}

pub(super) fn pods() -> ApiResource {
    api_resource("core", "v1", "Pod", "pods", true)
}

pub(super) fn nodes() -> ApiResource {
    api_resource("core", "v1", "Node", "nodes", false)
}

fn deployments() -> ApiResource {
    api_resource("apps", "v1", "Deployment", "deployments", true)
}

/// What discovery reports for `kind-kind`: the design's taxonomy (Workloads
/// 6, Network 7, Config & secrets 4, Storage 5, Cluster 14, Access control 5,
/// Namespaces, Events) and six custom resource API groups (`helm.sh`,
/// `metrics.k8s.io` and four more): 54 kinds in all, as the boards count them.
fn discovered_api_resources() -> Vec<ApiResource> {
    let namespaced =
        |group: &str, kind: &str, name: &str| api_resource(group, "v1", kind, name, true);
    let cluster =
        |group: &str, kind: &str, name: &str| api_resource(group, "v1", kind, name, false);
    vec![
        pods(),
        deployments(),
        namespaced("apps", "StatefulSet", "statefulsets"),
        namespaced("apps", "DaemonSet", "daemonsets"),
        namespaced("batch", "Job", "jobs"),
        namespaced("batch", "CronJob", "cronjobs"),
        namespaced("core", "Service", "services"),
        namespaced("core", "Endpoints", "endpoints"),
        namespaced("discovery.k8s.io", "EndpointSlice", "endpointslices"),
        namespaced("networking.k8s.io", "Ingress", "ingresses"),
        cluster("networking.k8s.io", "IngressClass", "ingressclasses"),
        namespaced("networking.k8s.io", "NetworkPolicy", "networkpolicies"),
        cluster(
            "gateway.networking.k8s.io",
            "GatewayClass",
            "gatewayclasses",
        ),
        namespaced("core", "ConfigMap", "configmaps"),
        namespaced("core", "Secret", "secrets"),
        namespaced("core", "ResourceQuota", "resourcequotas"),
        namespaced("core", "LimitRange", "limitranges"),
        cluster("core", "PersistentVolume", "persistentvolumes"),
        namespaced("core", "PersistentVolumeClaim", "persistentvolumeclaims"),
        cluster("storage.k8s.io", "StorageClass", "storageclasses"),
        cluster("storage.k8s.io", "VolumeAttachment", "volumeattachments"),
        cluster("storage.k8s.io", "CSIDriver", "csidrivers"),
        nodes(),
        cluster("scheduling.k8s.io", "PriorityClass", "priorityclasses"),
        cluster("node.k8s.io", "RuntimeClass", "runtimeclasses"),
        namespaced("coordination.k8s.io", "Lease", "leases"),
        cluster("core", "ComponentStatus", "componentstatuses"),
        cluster("apiregistration.k8s.io", "APIService", "apiservices"),
        cluster(
            "apiextensions.k8s.io",
            "CustomResourceDefinition",
            "customresourcedefinitions",
        ),
        cluster(
            "admissionregistration.k8s.io",
            "MutatingWebhookConfiguration",
            "mutatingwebhookconfigurations",
        ),
        cluster(
            "admissionregistration.k8s.io",
            "ValidatingWebhookConfiguration",
            "validatingwebhookconfigurations",
        ),
        cluster(
            "admissionregistration.k8s.io",
            "ValidatingAdmissionPolicy",
            "validatingadmissionpolicies",
        ),
        cluster(
            "admissionregistration.k8s.io",
            "ValidatingAdmissionPolicyBinding",
            "validatingadmissionpolicybindings",
        ),
        cluster(
            "certificates.k8s.io",
            "CertificateSigningRequest",
            "certificatesigningrequests",
        ),
        cluster("flowcontrol.apiserver.k8s.io", "FlowSchema", "flowschemas"),
        cluster(
            "flowcontrol.apiserver.k8s.io",
            "PriorityLevelConfiguration",
            "prioritylevelconfigurations",
        ),
        namespaced("core", "ServiceAccount", "serviceaccounts"),
        namespaced("rbac.authorization.k8s.io", "Role", "roles"),
        namespaced("rbac.authorization.k8s.io", "RoleBinding", "rolebindings"),
        cluster("rbac.authorization.k8s.io", "ClusterRole", "clusterroles"),
        cluster(
            "rbac.authorization.k8s.io",
            "ClusterRoleBinding",
            "clusterrolebindings",
        ),
        cluster("core", "Namespace", "namespaces"),
        namespaced("core", "Event", "events"),
        namespaced("helm.sh", "HelmChart", "helmcharts"),
        namespaced("helm.sh", "HelmChartConfig", "helmchartconfigs"),
        namespaced("metrics.k8s.io", "PodMetrics", "pods"),
        cluster("metrics.k8s.io", "NodeMetrics", "nodes"),
        namespaced("monitoring.coreos.com", "PodMonitor", "podmonitors"),
        namespaced("monitoring.coreos.com", "ServiceMonitor", "servicemonitors"),
        namespaced(
            "snapshot.storage.k8s.io",
            "VolumeSnapshot",
            "volumesnapshots",
        ),
        cluster(
            "snapshot.storage.k8s.io",
            "VolumeSnapshotClass",
            "volumesnapshotclasses",
        ),
        namespaced("traefik.io", "IngressRoute", "ingressroutes"),
        namespaced("upgrade.cattle.io", "Plan", "plans"),
    ]
}

/// The app's state after connecting to `kind-kind` and opening Pods in
/// `kube-system`, before any object arrives: four contexts, the namespaces,
/// discovery, the running Pods watch and the design clock.
pub(super) fn design_state() -> UiState {
    let mut clusters = HashMap::new();
    for (index, name) in DESIGN_CONTEXTS.into_iter().enumerate() {
        let key = index as i32 + 1;
        let mut cluster = ClusterState::for_test(key, name);
        // The kind contexts serve from the local machine.
        if name.starts_with("kind-") {
            cluster.server = Some("https://127.0.0.1:41207".into());
        }
        clusters.insert(key, cluster);
    }
    let kind = clusters.get_mut(&KIND_KIND).expect("kind-kind exists");
    kind.connection = ClusterConnectionState::Connected;
    for namespace in NAMESPACES {
        kind.namespaces.insert(
            namespace.into(),
            MinimalNamespace {
                name: namespace.into(),
                labels: Default::default(),
                annotations: Default::default(),
            },
        );
    }
    kind.selected_namespaces = SelectedNamespaces::from([KUBE_SYSTEM]);
    kind.resource_navigation = build_resource_navigation(discovered_api_resources());
    kind.selected_api_resource = Some(pods());
    kind.active_watchers
        .insert((pods(), Some(KUBE_SYSTEM.to_owned())));
    kind.active_pod_metrics.insert(KUBE_SYSTEM.to_owned());
    UiState {
        clusters,
        next_cluster_key: DESIGN_CONTEXTS.len() as i32,
        selected_cluster: Some(KIND_KIND),
        clock: WallClock::Fixed(design_now()),
        ..Default::default()
    }
}

/// An RFC 3339 creation time `age` before the design's now.
fn created(age: Duration) -> String {
    (design_now() - age)
        .format(&time::format_description::well_known::Rfc3339)
        .expect("a formattable time")
}

/// One pod of A02's table.
struct DesignPod {
    name: &'static str,
    node: &'static str,
    age: Duration,
    phase: &'static str,
    /// (container, ready, restarts, state) of each container.
    containers: &'static [(&'static str, bool, i32, &'static str)],
    /// CPU millicores and memory MiB, when metrics-server reports them.
    usage: Option<(i64, i64)>,
}

const RUNNING: &[(&str, bool, i32, &str)] = &[("main", true, 0, "running")];

const fn running(
    name: &'static str,
    node: &'static str,
    days: i64,
    usage: (i64, i64),
) -> DesignPod {
    DesignPod {
        name,
        node,
        age: Duration::days(days),
        phase: "Running",
        containers: RUNNING,
        usage: Some(usage),
    }
}

const CONTROL_PLANE: &str = "kind-control-plane";
const WORKER: &str = "kind-worker";

/// The twelve `kube-system` pods of A02, in the board's order.
fn design_pods() -> [DesignPod; 12] {
    [
        running("coredns-66bc5c9577-ffw2s", CONTROL_PLANE, 220, (15, 22)),
        running("coredns-66bc5c9577-z9gt9", CONTROL_PLANE, 220, (12, 20)),
        DesignPod {
            name: "api-7b948f-pod",
            node: WORKER,
            age: Duration::hours(3),
            phase: "Pending",
            containers: &[
                ("api", true, 0, "running"),
                ("migrate", false, 5, "waiting"),
            ],
            usage: Some((125, 96)),
        },
        running("etcd-kind-control-plane", CONTROL_PLANE, 220, (31, 58)),
        running("kindnet-9qrlh", CONTROL_PLANE, 220, (2, 11)),
        running(
            "kube-apiserver-kind-control-plane",
            CONTROL_PLANE,
            220,
            (88, 312),
        ),
        running(
            "kube-controller-manager-kind-control-plane",
            CONTROL_PLANE,
            220,
            (21, 54),
        ),
        running("kube-proxy-v86gd", CONTROL_PLANE, 220, (1, 18)),
        running(
            "kube-scheduler-kind-control-plane",
            CONTROL_PLANE,
            220,
            (4, 22),
        ),
        DesignPod {
            name: "job-nightly-report-28xk2",
            node: WORKER,
            age: Duration::minutes(14),
            phase: "Failed",
            containers: &[("report", false, 2, "terminated")],
            usage: None,
        },
        running("metrics-server-6d94bc8694-lk2qv", WORKER, 41, (3, 19)),
        running(
            "local-path-provisioner-7b8c8ddbd-8xnmp",
            CONTROL_PLANE,
            220,
            (1, 9),
        ),
    ]
}

fn container_state(state: &str) -> Value {
    match state {
        "running" => json!({"running": {"startedAt": created(Duration::days(1))}}),
        "waiting" => json!({"waiting": {"reason": "CrashLoopBackOff"}}),
        _ => json!({"terminated": {"exitCode": 1, "reason": "Error"}}),
    }
}

impl DesignPod {
    /// The pod as the API server returns it, in `kube-system`.
    fn object(&self) -> Pod {
        self.object_in(KUBE_SYSTEM)
    }

    /// The pod as the API server returns it, in `namespace`.
    fn object_in(&self, namespace: &str) -> Pod {
        k8s_openapi::serde_json::from_value(json!({
            "metadata": {
                "name": self.name,
                "namespace": namespace,
                "uid": format!("uid-{}", self.name),
                "creationTimestamp": created(self.age),
            },
            "spec": {
                "nodeName": self.node,
                "containers": self.containers.iter().map(|(name, ..)| json!({
                    "name": name,
                    "image": format!("registry.k8s.io/{name}:v1"),
                })).collect::<Vec<_>>(),
            },
            "status": {
                "phase": self.phase,
                "containerStatuses": self.containers.iter().map(|(name, ready, restarts, state)| json!({
                    "name": name,
                    "ready": ready,
                    "restartCount": restarts,
                    "image": format!("registry.k8s.io/{name}:v1"),
                    "imageID": "",
                    "state": container_state(state),
                })).collect::<Vec<_>>(),
            },
        }))
        .expect("a valid Pod")
    }
}

/// The three nodes of A04: the control plane and a worker Ready, a second
/// worker NotReady and younger.
fn design_nodes() -> Vec<Node> {
    [
        (CONTROL_PLANE, true, Some("control-plane"), 220),
        (WORKER, true, None, 220),
        ("kind-worker2", false, None, 12),
    ]
    .into_iter()
    .map(|(name, ready, role, age_days)| {
        let mut labels = BTreeMap::from([("kubernetes.io/hostname".to_owned(), name.to_owned())]);
        if let Some(role) = role {
            labels.insert(format!("node-role.kubernetes.io/{role}"), String::new());
        }
        k8s_openapi::serde_json::from_value(json!({
            "metadata": {
                "name": name,
                "uid": format!("uid-{name}"),
                "labels": labels,
                "creationTimestamp": created(Duration::days(age_days)),
            },
            "status": {
                "conditions": [{"type": "Ready", "status": if ready { "True" } else { "False" }}],
                "nodeInfo": {
                    "kubeletVersion": "v1.31.0",
                    "architecture": "amd64",
                    "bootID": "",
                    "containerRuntimeVersion": "containerd://1.7.18",
                    "kernelVersion": "6.10.0",
                    "kubeProxyVersion": "v1.31.0",
                    "machineID": "",
                    "operatingSystem": "linux",
                    "osImage": "Debian GNU/Linux 12 (bookworm)",
                    "systemUUID": "",
                },
                "capacity": {"cpu": "4", "memory": "7987Mi", "pods": "110"},
                "allocatable": {"cpu": "4", "memory": "7987Mi", "pods": "110"},
            },
        }))
        .expect("a valid Node")
    })
    .collect()
}

/// `count` named objects of a kind the board only counts.
fn counted(prefix: &str, count: usize) -> Vec<MinimalResource> {
    (0..count)
        .map(|index| MinimalResource {
            uid: format!("uid-{prefix}-{index}"),
            name: format!("{prefix}-{index}"),
            namespace: Some(KUBE_SYSTEM.into()),
            creation_timestamp: Some(design_now() - Duration::days(220)),
            controller_owner: None,
            labels: BTreeMap::new(),
            annotations: BTreeMap::new(),
            cells: BTreeMap::new(),
            log_containers: Vec::new(),
        })
        .collect()
}

fn helm_release(name: &str, chart: &str) -> HelmRelease {
    HelmRelease {
        storage: StorageDriver::Secret,
        storage_name: format!("sh.helm.release.v1.{name}.v1"),
        name: name.into(),
        namespace: KUBE_SYSTEM.into(),
        revision: 1,
        status: "deployed".into(),
        description: "Install complete".into(),
        notes: String::new(),
        chart: chart.into(),
        chart_version: "1.0.0".into(),
        app_version: "1.0.0".into(),
        first_deployed: created(Duration::days(41)),
        last_deployed: created(Duration::days(41)),
        values: json!({}),
        manifest: String::new(),
        storage_labels: BTreeMap::new(),
        storage_annotations: BTreeMap::new(),
    }
}

/// Deliver the objects of `kind-kind` as its watches list them: the twelve
/// pods and their metrics, the nodes, and the kinds the sidebar counts
/// (opened earlier in the session, so their watches still run).
pub(super) fn deliver_design_objects(harness: &mut AppHarness) {
    let design_pods = design_pods();
    harness.deliver_worker_result(KubernetesResourcesReplaced {
        cluster_key: KIND_KIND,
        api_resource: pods(),
        namespace: Some(KUBE_SYSTEM.into()),
        resources: design_pods
            .iter()
            .map(|pod| crate::resource_handlers::pod::extract(&pod.object()))
            .collect(),
    });
    harness.deliver_worker_result(PodMetricsUpdated {
        cluster_key: KIND_KIND,
        namespace: KUBE_SYSTEM.into(),
        usages: design_pods
            .iter()
            .filter_map(|pod| {
                let (cpu_millicores, memory_mebibytes) = pod.usage?;
                Some((
                    pod.name.to_owned(),
                    PodUsage {
                        timestamp: design_now(),
                        cpu_nanocores: cpu_millicores * 1_000_000,
                        memory_bytes: memory_mebibytes * 1024 * 1024,
                        containers: BTreeMap::new(),
                    },
                ))
            })
            .collect(),
    });
    harness.deliver_worker_result(KubernetesResourcesReplaced {
        cluster_key: KIND_KIND,
        api_resource: nodes(),
        namespace: None,
        resources: design_nodes()
            .iter()
            .map(crate::resource_handlers::node::extract)
            .collect(),
    });
    let navigation = harness.cluster(KIND_KIND).resource_navigation.clone();
    for (name, count) in [
        ("deployments", 4),
        ("statefulsets", 1),
        ("daemonsets", 2),
        ("jobs", 1),
        ("cronjobs", 1),
        ("events", 42),
    ] {
        let api_resource = navigation
            .all_api_resources()
            .find(|api_resource| {
                api_resource.name == name && api_resource.group != "metrics.k8s.io"
            })
            .expect("discovery reports the kind")
            .clone();
        harness.deliver_worker_result(KubernetesResourcesReplaced {
            cluster_key: KIND_KIND,
            api_resource,
            namespace: Some(KUBE_SYSTEM.into()),
            resources: counted(name, count),
        });
    }
    harness.deliver_worker_result(HelmReleasesReplaced {
        cluster_key: KIND_KIND,
        namespace: KUBE_SYSTEM.into(),
        releases: vec![
            helm_release("metrics-server", "metrics-server"),
            helm_release("local-path-provisioner", "local-path-provisioner"),
        ],
    });
}

/// The one failed CronJob run that A04 and A15 keep unread in Notifications
/// (their toasts are gone by then).
pub(super) fn deliver_design_unread_failure(harness: &mut AppHarness) {
    let operation_id = harness
        .state_mut()
        .ui_state
        .clusters
        .get_mut(&KIND_KIND)
        .expect("kind-kind exists")
        .cron_job_runs
        .start(());
    harness.deliver_worker_result(CronJobRunFailed {
        cluster_key: KIND_KIND,
        operation_id,
        namespace: KUBE_SYSTEM.into(),
        cron_job_name: "nightly-report".into(),
        error: "Forbidden: cannot create jobs.batch in kube-system".into(),
    });
}

/// The outcomes A02's toasts report: a Deployment scaled, a CronJob that
/// could not run.
pub(super) fn deliver_design_outcomes(harness: &mut AppHarness) {
    harness.deliver_worker_result(ResourceScaleUpdated {
        cluster_key: KIND_KIND,
        api_resource: deployments(),
        namespace: Some(KUBE_SYSTEM.into()),
        resource_name: "coredns".into(),
        replicas: 4,
    });
    // The scale was seen when it happened; only the failure is still unread.
    harness.state_mut().ui_state.mark_operation_history_read();
    let operation_id = harness
        .state_mut()
        .ui_state
        .clusters
        .get_mut(&KIND_KIND)
        .expect("kind-kind exists")
        .cron_job_runs
        .start(());
    harness.deliver_worker_result(CronJobRunFailed {
        cluster_key: KIND_KIND,
        operation_id,
        namespace: KUBE_SYSTEM.into(),
        cron_job_name: "nightly-report".into(),
        error: "Forbidden: cannot create jobs.batch in kube-system".into(),
    });
}

/// The application on `kind-kind`'s `kube-system` Pods with every design
/// object delivered, in a window `size` points large.
pub(super) fn design_harness(size: egui::Vec2) -> AppHarness {
    let mut harness = application_harness::<MockWorker>();
    harness.set_size(size);
    harness.seed_ui_state(design_state());
    deliver_design_objects(&mut harness);
    harness
}

/// A04's node usage: the Ready nodes report CPU and memory and count their
/// Pods; the NotReady one has no sample. Delivered once the Nodes table
/// watches node metrics.
pub(super) fn deliver_design_node_metrics(harness: &mut AppHarness) {
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    let generation = harness.cluster(KIND_KIND).node_pod_counts.generation;
    harness.deliver_worker_result(NodePodCountsUpdated {
        cluster_key: KIND_KIND,
        generation,
        counts: BTreeMap::from([(CONTROL_PLANE.to_owned(), 14), (WORKER.to_owned(), 31)]),
    });
    harness.deliver_worker_result(NodeMetricsUpdated {
        cluster_key: KIND_KIND,
        usages: [
            (CONTROL_PLANE, 820_000_000, 2.1),
            (WORKER, 3_100_000_000, 5.9),
        ]
        .into_iter()
        .map(|(name, cpu_nanocores, memory_gibibytes)| {
            (
                name.to_owned(),
                NodeUsage {
                    timestamp: design_now(),
                    cpu_nanocores,
                    memory_bytes: (memory_gibibytes * GIB) as i64,
                },
            )
        })
        .collect(),
    });
}

// A03: the same twelve pods spread over three namespaces.

/// The namespaces A03 selects.
const A03_NAMESPACES: [&str; 3] = [KUBE_SYSTEM, "monitoring", "payments"];
/// The namespaces the board's selector lists besides the cluster's own
/// five: nine in all.
const A03_EXTRA_NAMESPACES: [&str; 4] = [
    "monitoring",
    "payments",
    "moonbase-prod",
    "mongodb-operator",
];

/// The `kind-kind` state of A03: Pods of three namespaces selected, the
/// namespace identities of the selector configured.
fn namespaces_state() -> UiState {
    let mut state = design_state();
    let kind = state
        .clusters
        .get_mut(&KIND_KIND)
        .expect("kind-kind exists");
    for namespace in A03_EXTRA_NAMESPACES {
        let labels = if namespace == "moonbase-prod" {
            BTreeMap::from([("customer".to_owned(), "Moonbase".to_owned())])
        } else {
            BTreeMap::new()
        };
        kind.namespaces.insert(
            namespace.into(),
            MinimalNamespace {
                name: namespace.into(),
                labels,
                annotations: BTreeMap::new(),
            },
        );
    }
    kind.selected_namespaces = SelectedNamespaces::from(A03_NAMESPACES);
    for namespace in A03_NAMESPACES {
        kind.active_watchers
            .insert((pods(), Some(namespace.to_owned())));
        kind.active_pod_metrics.insert(namespace.to_owned());
    }
    state
}

/// A03's pods by namespace, as the indexes of [`design_pods`].
const A03_PODS: [(&str, [usize; 4]); 3] = [
    (KUBE_SYSTEM, [0, 3, 6, 9]),
    ("monitoring", [1, 4, 7, 10]),
    ("payments", [2, 5, 8, 11]),
];

/// The application on `kind-kind`'s Pods of `kube-system`, `monitoring`
/// and `payments`, four each, in a window `size` points large.
pub(super) fn design_namespaces_harness(size: egui::Vec2) -> AppHarness {
    let mut harness = application_harness::<MockWorker>();
    harness.set_size(size);
    harness.seed_ui_state(namespaces_state());
    harness.state_mut().namespace_selector_settings =
        crate::ui::namespace_selector::NamespaceSelectorSettings {
            fields: vec![crate::ui::namespace_selector::NamespaceMetadataField {
                alias: "customer".into(),
                source: crate::ui::table_preferences::MetadataColumnSource::Label,
                key: "customer".into(),
            }],
            templates: vec![crate::ui::namespace_selector::NamespaceIdentityTemplate {
                template: "Customer: {{customer}}".into(),
            }],
        };
    let design_pods = design_pods();
    for (namespace, indexes) in A03_PODS {
        let pods_here = indexes.map(|index| &design_pods[index]);
        harness.deliver_worker_result(KubernetesResourcesReplaced {
            cluster_key: KIND_KIND,
            api_resource: pods(),
            namespace: Some(namespace.into()),
            resources: pods_here
                .iter()
                .map(|pod| crate::resource_handlers::pod::extract(&pod.object_in(namespace)))
                .collect(),
        });
        harness.deliver_worker_result(PodMetricsUpdated {
            cluster_key: KIND_KIND,
            namespace: namespace.into(),
            usages: pods_here
                .iter()
                .filter_map(|pod| {
                    let (cpu_millicores, memory_mebibytes) = pod.usage?;
                    Some((
                        pod.name.to_owned(),
                        PodUsage {
                            timestamp: design_now(),
                            cpu_nanocores: cpu_millicores * 1_000_000,
                            memory_bytes: memory_mebibytes * 1024 * 1024,
                            containers: BTreeMap::new(),
                        },
                    ))
                })
                .collect(),
        });
    }
    harness
}

// ---- Resource inspectors (A05, WP3c) ----------------------------------------------

use crate::cluster_connection_manager::resource_detail;
use crate::resource_detail::{ResourceDetail, ResourceEvent};
use crate::worker::{ResourceDetailPodUsageUpdated, ResourceDetailUpdated, ResourceEventsReplaced};
use kube::api::DynamicObject;

pub(super) fn replica_sets() -> ApiResource {
    api_resource("apps", "v1", "ReplicaSet", "replicasets", true)
}

/// The uid of the inspected `coredns` pod, as A05 prints it.
pub(super) const COREDNS_POD_UID: &str = "d1f2c3a4-b5e6-47f8";
pub(super) const COREDNS_POD: &str = "coredns-66bc5c9577-ffw2s";
pub(super) const COREDNS_REPLICA_SET: &str = "coredns-66bc5c9577";

fn dynamic(value: Value) -> DynamicObject {
    k8s_openapi::serde_json::from_value(value).expect("a valid object")
}

/// The `coredns` Deployment, as the API server returns it.
pub(in crate::ui) fn coredns_deployment_detail() -> ResourceDetail {
    resource_detail(
        deployments(),
        &dynamic(json!({
            "apiVersion": "apps/v1", "kind": "Deployment",
            "metadata": {
                "name": "coredns", "namespace": KUBE_SYSTEM, "uid": "uid-coredns-deployment",
                "resourceVersion": "812", "creationTimestamp": created(Duration::days(220)),
                "labels": {"k8s-app": "kube-dns"},
            },
            "spec": {"replicas": 2},
        })),
    )
}

/// The `coredns-66bc5c9577` ReplicaSet, owned by the Deployment.
pub(super) fn coredns_replica_set_detail() -> ResourceDetail {
    resource_detail(
        replica_sets(),
        &dynamic(json!({
            "apiVersion": "apps/v1", "kind": "ReplicaSet",
            "metadata": {
                "name": COREDNS_REPLICA_SET, "namespace": KUBE_SYSTEM,
                "uid": "uid-coredns-replica-set", "resourceVersion": "813",
                "creationTimestamp": created(Duration::days(220)),
                "ownerReferences": [{
                    "apiVersion": "apps/v1", "kind": "Deployment", "name": "coredns",
                    "uid": "uid-coredns-deployment", "controller": true,
                }],
            },
            "spec": {"replicas": 2},
        })),
    )
}

/// A05's pod: one `coredns` container, owned by the ReplicaSet.
pub(in crate::ui) fn coredns_pod_detail() -> ResourceDetail {
    resource_detail(
        pods(),
        &dynamic(json!({
            "apiVersion": "v1", "kind": "Pod",
            "metadata": {
                "name": COREDNS_POD, "namespace": KUBE_SYSTEM, "uid": COREDNS_POD_UID,
                "resourceVersion": "9041", "creationTimestamp": created(Duration::days(220)),
                "labels": {
                    "k8s-app": "kube-dns",
                    "pod-template-hash": "66bc5c9577",
                    "tier": "control-plane",
                },
                "annotations": {"cluster-autoscaler.kubernetes.io/safe-to-evict": "true"},
                "ownerReferences": [{
                    "apiVersion": "apps/v1", "kind": "ReplicaSet", "name": COREDNS_REPLICA_SET,
                    "uid": "uid-coredns-replica-set", "controller": true,
                }],
            },
            "spec": {
                "nodeName": CONTROL_PLANE,
                "serviceAccountName": "coredns",
                "containers": [{
                    "name": "coredns",
                    "image": "coredns:v1.11.1",
                    "args": ["-conf", "/etc/coredns/Corefile"],
                    "ports": [
                        {"containerPort": 53, "protocol": "UDP"},
                        {"containerPort": 53, "protocol": "TCP"},
                        {"containerPort": 9153, "protocol": "TCP"},
                    ],
                    "resources": {
                        "limits": {"cpu": "100m", "memory": "170Mi"},
                        "requests": {"cpu": "100m", "memory": "70Mi"},
                    },
                }],
            },
            "status": {
                "phase": "Running",
                "qosClass": "Burstable",
                "podIP": "10.244.0.3",
                "hostIP": "172.18.0.2",
                "containerStatuses": [{
                    "name": "coredns", "ready": true, "restartCount": 0,
                    "image": "coredns:v1.11.1", "imageID": "",
                    "state": {"running": {"startedAt": created(Duration::days(1))}},
                }],
            },
        })),
    )
}

/// The two normal events of A05's pod.
pub(in crate::ui) fn coredns_pod_events() -> Vec<ResourceEvent> {
    [
        ("Scheduled", Duration::days(220)),
        ("Started", Duration::days(1)),
    ]
    .into_iter()
    .map(|(reason, age)| ResourceEvent {
        uid: format!("uid-event-{reason}"),
        type_: "Normal".into(),
        reason: reason.into(),
        message: format!("{reason} {COREDNS_POD}"),
        source: Some("kubelet".into()),
        count: 1,
        last_timestamp: Some(design_now() - age),
    })
    .collect()
}

/// Open Deployment `coredns`, its ReplicaSet and the pod, the way a reader
/// follows owners down to a pod, and deliver what each watch reports.
pub(super) fn open_coredns_inspector_stack(harness: &mut AppHarness) {
    // Discovery knows ReplicaSets, so the pod's owner is a link.
    let cluster = harness
        .state_mut()
        .ui_state
        .clusters
        .get_mut(&KIND_KIND)
        .expect("kind-kind exists");
    cluster.resource_navigation = build_resource_navigation(
        discovered_api_resources()
            .into_iter()
            .chain([replica_sets()])
            .collect(),
    );
    let mut commands = Vec::new();
    let state = &mut harness.state_mut().ui_state;
    state.open_resource_detail(
        KIND_KIND,
        deployments(),
        "coredns".into(),
        Some(KUBE_SYSTEM.into()),
        "uid-coredns-deployment".into(),
        &mut commands,
    );
    state.navigate_resource_detail(
        KIND_KIND,
        replica_sets(),
        COREDNS_REPLICA_SET.into(),
        Some(KUBE_SYSTEM.into()),
        "uid-coredns-replica-set".into(),
        &mut commands,
    );
    state.navigate_resource_detail(
        KIND_KIND,
        pods(),
        COREDNS_POD.into(),
        Some(KUBE_SYSTEM.into()),
        COREDNS_POD_UID.into(),
        &mut commands,
    );
    harness.run();
    for (history_entry_id, detail) in [
        (1, coredns_deployment_detail()),
        (2, coredns_replica_set_detail()),
        (3, coredns_pod_detail()),
    ] {
        harness.deliver_worker_result(ResourceDetailUpdated {
            cluster_key: KIND_KIND,
            history_entry_id,
            detail: Box::new(detail),
        });
    }
    harness.deliver_worker_result(ResourceEventsReplaced {
        cluster_key: KIND_KIND,
        history_entry_id: 3,
        events: coredns_pod_events(),
    });
    // Seven samples, 100 seconds apart, ending now.
    for (index, cpu_millicores) in [12, 13, 14, 12, 16, 15, 15].into_iter().enumerate() {
        harness.deliver_worker_result(ResourceDetailPodUsageUpdated {
            cluster_key: KIND_KIND,
            history_entry_id: 3,
            usage: PodUsage {
                timestamp: design_now() - Duration::seconds(100 * (6 - index as i64)),
                cpu_nanocores: cpu_millicores * 1_000_000,
                memory_bytes: 22 * 1024 * 1024,
                containers: BTreeMap::new(),
            },
        });
    }
}

/// A13's release: `demo` of chart nginx, installed in revision 1 and upgraded
/// in revision 2, newest first.
pub(super) fn demo_helm_releases() -> Vec<HelmRelease> {
    let upgrade = HelmRelease {
        storage: StorageDriver::Secret,
        storage_name: "sh.helm.release.v1.demo.v2".into(),
        name: "demo".into(),
        namespace: KUBE_SYSTEM.into(),
        revision: 2,
        status: "deployed".into(),
        description: "Upgrade complete".into(),
        notes: "Thank you for installing demo.\n\nYour release is ready. Get the URL with:\n  kubectl get svc demo -n kube-system".into(),
        chart: "nginx".into(),
        chart_version: "1.2.3".into(),
        app_version: "1.25.0".into(),
        first_deployed: "2026-08-01T12:00:00Z".into(),
        last_deployed: "2026-08-15T12:00:00Z".into(),
        values: json!({"replicaCount": 2, "service": {"type": "ClusterIP"}}),
        manifest: "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: demo\n---\napiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: demo\n".into(),
        storage_labels: BTreeMap::new(),
        storage_annotations: BTreeMap::new(),
    };
    let install = HelmRelease {
        storage_name: "sh.helm.release.v1.demo.v1".into(),
        revision: 1,
        status: "superseded".into(),
        description: "Install complete".into(),
        last_deployed: "2026-08-01T12:00:00Z".into(),
        ..upgrade.clone()
    };
    vec![upgrade, install]
}

// ---- A01 and A16: the clusters of the kubeconfig -------------------------------------------

/// What A01 and A16 know of the clusters besides `kind-kind`: where each
/// context runs (the servers its kubeconfig names), how each is connected,
/// the pins and visits the user made, the nodes last seen of the clusters
/// not connected, and the EKS cluster a scan found.
pub(super) fn seed_design_clusters(harness: &mut AppHarness) {
    use crate::cluster_connection_manager::{AvailableEksCluster, AwsProfile};
    let profile = AwsProfile::Named("production".into());
    let request_id = {
        let ui = &mut harness.state_mut().ui_state;
        for cluster in ui.clusters.values_mut() {
            match cluster.name.as_str() {
                "dev-eu" => {
                    cluster.server = Some("https://0A1B2C.gr7.eu-west-1.eks.amazonaws.com".into());
                    cluster.connection = ClusterConnectionState::Connected;
                }
                "prod-eu" => {
                    cluster.presentation.badge = Some("PR".into());
                    cluster.server =
                        Some("https://prod-eu-4f2a.hcp.westeurope.azmk8s.io:443".into());
                    cluster.connection =
                        ClusterConnectionState::Failed("connection refused".into());
                }
                _ => {}
            }
        }
        ui.cluster_roster.pinned = vec!["kind-kind".into(), "prod-eu".into()];
        ui.cluster_roster.recent = vec!["dev-eu".into(), "kind-legacy".into()];
        ui.kubeconfig_source = "~/.kube/config".into();
        ui.cluster_roster.known_nodes = BTreeMap::from([
            ("dev-eu".into(), 38),
            ("kind-legacy".into(), 1),
            ("prod-eu".into(), 62),
        ]);
        let aws = &mut ui.managed_cluster_discovery.aws;
        aws.begin_scan("222222222222".into(), profile.clone());
        aws.request_id
    };
    harness.deliver_worker_result(crate::worker::EksClustersUpdated {
        request_id,
        account_id: "222222222222".into(),
        profile: profile.clone(),
        clusters: vec![AvailableEksCluster {
            name: "payments".into(),
            region: "eu-west-1".into(),
            account_id: "222222222222".into(),
            partition: "aws".into(),
            profile,
            tags: BTreeMap::new(),
            configured: false,
        }],
        warning: None,
    });
}

// ---- Dialogs (A06, A07; WP3d) -------------------------------------------------------

/// The finalizers that keep A07's pod from going away.
pub(super) const STUCK_FINALIZERS: [&str; 2] =
    ["example.com/cleanup", "kubernetes.io/pvc-protection"];
/// The pod A07 force deletes.
pub(super) const STUCK_POD: &str = "job-nightly-report-28xk2";

/// A06's four Deployments of `kube-system`: (name, age, replicas, ready,
/// up to date, available). `api` is mid-rollout, three of four available.
const DESIGN_DEPLOYMENTS: [(&str, Duration, i32, i32, i32, i32); 4] = [
    ("coredns", Duration::days(220), 2, 2, 2, 2),
    ("metrics-server", Duration::days(41), 1, 1, 1, 1),
    ("api", Duration::hours(3), 4, 3, 4, 3),
    ("local-path-provisioner", Duration::days(220), 1, 1, 1, 1),
];

/// A06's Deployments as the API server returns them, in `kube-system`.
fn design_deployments() -> Vec<MinimalResource> {
    DESIGN_DEPLOYMENTS
        .into_iter()
        .map(|(name, age, replicas, ready, updated, available)| {
            let deployment = k8s_openapi::serde_json::from_value(json!({
                "metadata": {
                    "name": name,
                    "namespace": KUBE_SYSTEM,
                    "uid": format!("uid-deployment-{name}"),
                    "creationTimestamp": created(age),
                },
                "status": {
                    "replicas": replicas,
                    "readyReplicas": ready,
                    "updatedReplicas": updated,
                    "availableReplicas": available,
                },
            }))
            .expect("a valid Deployment");
            crate::resource_handlers::deployment::extract(&deployment)
        })
        .collect()
}

/// Deliver A06's Deployments of `kube-system`.
pub(super) fn deliver_design_deployments(harness: &mut AppHarness) {
    harness.deliver_worker_result(KubernetesResourcesReplaced {
        cluster_key: KIND_KIND,
        api_resource: deployments(),
        namespace: Some(KUBE_SYSTEM.into()),
        resources: design_deployments(),
    });
}

/// A07's pod: `job-nightly-report-28xk2` is being deleted and held back by two
/// finalizers. Replaces the pod the design objects delivered.
pub(super) fn deliver_design_stuck_pod(harness: &mut AppHarness) {
    let resources = design_pods()
        .iter()
        .map(|pod| {
            let resource = crate::resource_handlers::pod::extract(&pod.object());
            if pod.name == STUCK_POD {
                resource.with_lifecycle_metadata(true, STUCK_FINALIZERS.map(str::to_owned).to_vec())
            } else {
                resource
            }
        })
        .collect();
    harness.deliver_worker_result(KubernetesResourcesReplaced {
        cluster_key: KIND_KIND,
        api_resource: pods(),
        namespace: Some(KUBE_SYSTEM.into()),
        resources,
    });
}
