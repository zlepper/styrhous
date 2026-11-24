use crate::api_resource::{ApiResource, ApiResourceSet, canonical_group};
use crate::resource_detail::ResourceOwner;
use std::collections::{BTreeMap, BTreeSet};

/// A fixed section in the primary resource navigator.
#[derive(Debug, Clone)]
pub(super) struct CuratedResourceSection {
    pub(super) name: &'static str,
    pub(super) api_resources: Vec<ApiResource>,
}

/// A primary resource or a fixed section in the resource navigator.
#[derive(Debug, Clone)]
pub(super) enum CuratedNavigationEntry {
    Resource(ApiResource),
    Section(CuratedResourceSection),
}

/// Resource navigation derived from Kubernetes discovery.
///
/// The section order and the resources assigned to it are intentionally static.
/// Discovery only decides which resources exist on the connected cluster and provides
/// the server-supported API version used by the watcher.
#[derive(Debug, Clone, Default)]
pub(super) struct ResourceNavigation {
    pub(super) curated_entries: Vec<CuratedNavigationEntry>,
    pub(super) other_api_groups: BTreeMap<String, Vec<ApiResource>>,
}

impl ResourceNavigation {
    pub(super) fn api_resource_for_owner(&self, owner: &ResourceOwner) -> Option<ApiResource> {
        let group = owner.api_group();
        self.all_api_resources()
            .find(|resource| resource.group == group && resource.kind == owner.kind)
            .cloned()
    }

    /// Every kind of the navigation: the curated ones in order, then the other groups'.
    pub(crate) fn all_api_resources(&self) -> impl Iterator<Item = &ApiResource> {
        self.curated_entries
            .iter()
            .flat_map(|entry| match entry {
                CuratedNavigationEntry::Resource(resource) => std::slice::from_ref(resource),
                CuratedNavigationEntry::Section(section) => section.api_resources.as_slice(),
            })
            .chain(self.other_api_groups.values().flatten())
    }
}

#[derive(Clone, Copy)]
struct CatalogEntry {
    group: &'static str,
    name: &'static str,
}

struct CatalogSection {
    name: &'static str,
    entries: &'static [CatalogEntry],
}

enum CatalogItem {
    Resource(CatalogEntry),
    Section(CatalogSection),
}

const CORE: &str = crate::api_resource::CORE_GROUP;

/// A catalog entry of `group` and resource `name`.
const fn entry(group: &'static str, name: &'static str) -> CatalogEntry {
    CatalogEntry { group, name }
}

const GATEWAY: &str = "gateway.networking.k8s.io";
const ADMISSION: &str = "admissionregistration.k8s.io";
const FLOW_CONTROL: &str = "flowcontrol.apiserver.k8s.io";
const RBAC: &str = "rbac.authorization.k8s.io";

/// The sidebar's Kubernetes taxonomy (S09): the groups in order, then the
/// kinds that stand on their own.
const CATALOG: &[CatalogItem] = &[
    CatalogItem::Section(CatalogSection {
        name: "Workloads",
        entries: &[
            entry(CORE, "pods"),
            entry("apps", "deployments"),
            entry("apps", "statefulsets"),
            entry("apps", "daemonsets"),
            entry("apps", "replicasets"),
            entry(CORE, "replicationcontrollers"),
            entry("batch", "jobs"),
            entry("batch", "cronjobs"),
            entry("autoscaling", "horizontalpodautoscalers"),
            entry("policy", "poddisruptionbudgets"),
        ],
    }),
    CatalogItem::Section(CatalogSection {
        name: "Network",
        entries: &[
            entry(CORE, "services"),
            entry(CORE, "endpoints"),
            entry("discovery.k8s.io", "endpointslices"),
            entry("networking.k8s.io", "ingresses"),
            entry("networking.k8s.io", "ingressclasses"),
            entry("networking.k8s.io", "networkpolicies"),
            entry(GATEWAY, "gatewayclasses"),
            entry(GATEWAY, "gateways"),
            entry(GATEWAY, "httproutes"),
            entry(GATEWAY, "grpcroutes"),
            entry(GATEWAY, "tlsroutes"),
            entry(GATEWAY, "referencegrants"),
            entry(GATEWAY, "backendtlspolicies"),
        ],
    }),
    CatalogItem::Section(CatalogSection {
        name: "Config & secrets",
        entries: &[
            entry(CORE, "configmaps"),
            entry(CORE, "secrets"),
            entry(CORE, "resourcequotas"),
            entry(CORE, "limitranges"),
        ],
    }),
    CatalogItem::Section(CatalogSection {
        name: "Storage",
        entries: &[
            entry(CORE, "persistentvolumes"),
            entry(CORE, "persistentvolumeclaims"),
            entry("storage.k8s.io", "storageclasses"),
            entry("storage.k8s.io", "volumeattachments"),
            entry("storage.k8s.io", "csidrivers"),
            entry("storage.k8s.io", "csinodes"),
            entry("storage.k8s.io", "csistoragecapacities"),
        ],
    }),
    CatalogItem::Section(CatalogSection {
        name: "Cluster",
        entries: &[
            entry(CORE, "nodes"),
            entry("scheduling.k8s.io", "priorityclasses"),
            entry("node.k8s.io", "runtimeclasses"),
            entry("coordination.k8s.io", "leases"),
            entry(CORE, "componentstatuses"),
            entry("apiregistration.k8s.io", "apiservices"),
            entry("apiextensions.k8s.io", "customresourcedefinitions"),
            entry(ADMISSION, "mutatingwebhookconfigurations"),
            entry(ADMISSION, "validatingwebhookconfigurations"),
            entry(ADMISSION, "validatingadmissionpolicies"),
            entry(ADMISSION, "validatingadmissionpolicybindings"),
            entry("certificates.k8s.io", "certificatesigningrequests"),
            entry(FLOW_CONTROL, "flowschemas"),
            entry(FLOW_CONTROL, "prioritylevelconfigurations"),
        ],
    }),
    CatalogItem::Section(CatalogSection {
        name: "Access control",
        entries: &[
            entry(CORE, "serviceaccounts"),
            entry(RBAC, "roles"),
            entry(RBAC, "rolebindings"),
            entry(RBAC, "clusterroles"),
            entry(RBAC, "clusterrolebindings"),
        ],
    }),
    CatalogItem::Resource(entry(CORE, "namespaces")),
    CatalogItem::Resource(entry(CORE, "events")),
];

pub(super) fn build_resource_navigation(api_resources: Vec<ApiResource>) -> ResourceNavigation {
    let mut discovered = BTreeMap::new();
    for resource in api_resources {
        let key = (
            canonical_group(&resource.group).to_owned(),
            resource.name.clone(),
        );
        discovered.entry(key).or_insert(resource);
    }

    let mut categorized = BTreeSet::new();
    let curated_entries: Vec<_> = CATALOG
        .iter()
        .filter_map(|item| match item {
            CatalogItem::Resource(entry) => {
                discovered_resource(entry, &discovered, &mut categorized)
                    .map(CuratedNavigationEntry::Resource)
            }
            CatalogItem::Section(section) => {
                let api_resources = section
                    .entries
                    .iter()
                    .filter_map(|entry| discovered_resource(entry, &discovered, &mut categorized))
                    .collect::<Vec<_>>();

                (!api_resources.is_empty()).then_some(CuratedNavigationEntry::Section(
                    CuratedResourceSection {
                        name: section.name,
                        api_resources,
                    },
                ))
            }
        })
        .collect();

    let mut other_api_groups: BTreeMap<String, Vec<ApiResource>> = BTreeMap::new();
    for ((group, name), resource) in discovered {
        if !categorized.contains(&(group.clone(), name)) {
            other_api_groups.entry(group).or_default().push(resource);
        }
    }
    for resources in other_api_groups.values_mut() {
        resources.sort_by(|left, right| left.name.cmp(&right.name));
    }

    let mut curated_entries = curated_entries;
    curated_entries.push(CuratedNavigationEntry::Resource(
        ApiResource::helm_releases(),
    ));

    ResourceNavigation {
        curated_entries,
        other_api_groups,
    }
}

/// The category colour (`theme.category.nth`) marking a kind in the sidebar,
/// as the design colours them: Pods coral, controllers teal, Nodes sky, Helm
/// releases violet, Namespaces green and Events amber. Other kinds take
/// their group's colour; custom resources the first.
pub(crate) fn kind_category(api_resource: &ApiResource) -> usize {
    const VIOLET: usize = 0;
    const TEAL: usize = 1;
    const AMBER: usize = 2;
    const CORAL: usize = 3;
    const SKY: usize = 4;
    const GREEN: usize = 5;
    if api_resource.is_helm_releases() {
        return VIOLET;
    }
    match api_resource.identity() {
        (CORE, "pods") => CORAL,
        (CORE, "nodes") => SKY,
        (CORE, "namespaces") => GREEN,
        (CORE, "events") => AMBER,
        identity => CATALOG
            .iter()
            .find_map(|item| match item {
                CatalogItem::Section(section)
                    if section
                        .entries
                        .iter()
                        .any(|entry| (entry.group, entry.name) == identity) =>
                {
                    Some(section.name)
                }
                _ => None,
            })
            .map_or(VIOLET, |section| match section {
                "Workloads" => TEAL,
                "Network" | "Cluster" => SKY,
                "Config & secrets" => AMBER,
                "Storage" => GREEN,
                _ => VIOLET,
            }),
    }
}

fn discovered_resource(
    entry: &CatalogEntry,
    discovered: &BTreeMap<(String, String), ApiResource>,
    categorized: &mut BTreeSet<(String, String)>,
) -> Option<ApiResource> {
    let key = (entry.group.to_owned(), entry.name.to_owned());
    discovered.get(&key).cloned().inspect(|_| {
        categorized.insert(key);
    })
}

/// The built-in Kubernetes resources of the curated navigation, assumed to exist before
/// discovery has answered so a first connection can show them at once. Version, kind and
/// scope come from the Kubernetes API definitions; discovery replaces them if the cluster
/// serves something else.
pub(crate) fn builtin_api_resources() -> ApiResourceSet {
    use k8s_openapi::api::{
        admissionregistration::v1 as admission, apps::v1 as apps, autoscaling::v2 as autoscaling,
        batch::v1 as batch, certificates::v1 as certificates, coordination::v1 as coordination,
        core::v1 as core, discovery::v1 as discovery, flowcontrol::v1 as flowcontrol,
        networking::v1 as networking, node::v1 as node, policy::v1 as policy, rbac::v1 as rbac,
        scheduling::v1 as scheduling, storage::v1 as storage,
    };
    use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1 as apiextensions;
    use k8s_openapi::kube_aggregator::pkg::apis::apiregistration::v1 as apiregistration;

    let mut resources = ApiResourceSet::default();
    for (api_resource, scalable) in [
        (ApiResource::of::<core::Node>(), false),
        (ApiResource::of::<core::Pod>(), false),
        (ApiResource::of::<apps::Deployment>(), true),
        (ApiResource::of::<apps::StatefulSet>(), true),
        (ApiResource::of::<apps::DaemonSet>(), false),
        (ApiResource::of::<apps::ReplicaSet>(), true),
        (ApiResource::of::<core::ReplicationController>(), true),
        (ApiResource::of::<batch::Job>(), false),
        (ApiResource::of::<batch::CronJob>(), false),
        (ApiResource::of::<core::ConfigMap>(), false),
        (ApiResource::of::<core::Secret>(), false),
        (ApiResource::of::<core::ResourceQuota>(), false),
        (ApiResource::of::<core::LimitRange>(), false),
        (
            ApiResource::of::<autoscaling::HorizontalPodAutoscaler>(),
            false,
        ),
        (ApiResource::of::<policy::PodDisruptionBudget>(), false),
        (ApiResource::of::<scheduling::PriorityClass>(), false),
        (ApiResource::of::<node::RuntimeClass>(), false),
        (ApiResource::of::<coordination::Lease>(), false),
        (ApiResource::of::<core::Service>(), false),
        (ApiResource::of::<core::Endpoints>(), false),
        (ApiResource::of::<discovery::EndpointSlice>(), false),
        (ApiResource::of::<networking::Ingress>(), false),
        (ApiResource::of::<networking::IngressClass>(), false),
        (ApiResource::of::<networking::NetworkPolicy>(), false),
        (ApiResource::of::<core::PersistentVolume>(), false),
        (ApiResource::of::<core::PersistentVolumeClaim>(), false),
        (ApiResource::of::<storage::StorageClass>(), false),
        (ApiResource::of::<storage::VolumeAttachment>(), false),
        (ApiResource::of::<storage::CSIDriver>(), false),
        (ApiResource::of::<storage::CSINode>(), false),
        (ApiResource::of::<storage::CSIStorageCapacity>(), false),
        (ApiResource::of::<core::ComponentStatus>(), false),
        (ApiResource::of::<apiregistration::APIService>(), false),
        (
            ApiResource::of::<apiextensions::CustomResourceDefinition>(),
            false,
        ),
        (
            ApiResource::of::<admission::MutatingWebhookConfiguration>(),
            false,
        ),
        (
            ApiResource::of::<admission::ValidatingWebhookConfiguration>(),
            false,
        ),
        (
            ApiResource::of::<admission::ValidatingAdmissionPolicy>(),
            false,
        ),
        (
            ApiResource::of::<admission::ValidatingAdmissionPolicyBinding>(),
            false,
        ),
        (
            ApiResource::of::<certificates::CertificateSigningRequest>(),
            false,
        ),
        (ApiResource::of::<flowcontrol::FlowSchema>(), false),
        (
            ApiResource::of::<flowcontrol::PriorityLevelConfiguration>(),
            false,
        ),
        (ApiResource::of::<core::Namespace>(), false),
        (ApiResource::of::<core::Event>(), false),
        (ApiResource::of::<core::ServiceAccount>(), false),
        (ApiResource::of::<rbac::Role>(), false),
        (ApiResource::of::<rbac::RoleBinding>(), false),
        (ApiResource::of::<rbac::ClusterRole>(), false),
        (ApiResource::of::<rbac::ClusterRoleBinding>(), false),
    ] {
        resources.push(api_resource, scalable);
    }
    resources
}

#[cfg(test)]
mod tests;
