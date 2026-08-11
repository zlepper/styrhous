use crate::api_resource::ApiResource;
use crate::minimal_resource::PodLogContainer;
use crate::resource_table::CellValue;
use std::collections::BTreeMap;
use time::OffsetDateTime;

/// Data shared by every resource detail renderer.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct ResourceDetail {
    pub(crate) api_resource: ApiResource,
    pub(crate) name: String,
    pub(crate) namespace: Option<String>,
    pub(crate) uid: String,
    /// Kubernetes' optimistic-concurrency token for the watched object.
    pub(crate) resource_version: String,
    /// Kubernetes has accepted a deletion request but retains the object.
    pub(crate) is_deleting: bool,
    /// Controller cleanup hooks that currently block deletion.
    pub(crate) finalizers: Vec<String>,
    pub(crate) creation_timestamp: Option<OffsetDateTime>,
    pub(crate) owner: Option<ResourceOwner>,
    pub(crate) labels: BTreeMap<String, String>,
    pub(crate) annotations: BTreeMap<String, String>,
    pub(crate) payload: ResourceDetailPayload,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct ResourceOwner {
    pub(crate) kind: String,
    pub(crate) name: String,
}

/// A resource related to the object currently shown in the inspector.
///
/// The relationship may be workload ownership or a Pod's node assignment;
/// type-specific cells let inspector tables match the main resource-list
/// presentation.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct ManagedResource {
    pub(crate) api_resource: ApiResource,
    pub(crate) name: String,
    pub(crate) namespace: Option<String>,
    pub(crate) uid: String,
    pub(crate) association: ManagedResourceAssociation,
    pub(crate) creation_timestamp: Option<OffsetDateTime>,
    /// Type-specific table values, extracted alongside the resource metadata.
    pub(crate) cells: BTreeMap<String, CellValue>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum ManagedResourceAssociation {
    ControllerOwnerUid(String),
    NodeName(String),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum ResourceDetailPayload {
    Generic,
    Pod(Box<PodDetail>),
    Node(NodeDetail),
    ConfigMap(ConfigMapDetail),
    Secret(SecretDetail),
}

#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub(crate) struct ConfigMapDetail {
    pub(crate) data: BTreeMap<String, String>,
    pub(crate) immutable: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub(crate) struct SecretDetail {
    pub(crate) data: BTreeMap<String, SecretDataDetail>,
    pub(crate) immutable: bool,
    pub(crate) type_: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct SecretDataDetail {
    pub(crate) byte_len: usize,
    /// Secret bytes which are not valid UTF-8 remain visible only as a length and
    /// cannot be edited as text.
    pub(crate) text: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub(crate) struct PodDetail {
    pub(crate) phase: String,
    pub(crate) conditions: Vec<PodConditionDetail>,
    pub(crate) node_name: Option<String>,
    pub(crate) pod_ip: Option<String>,
    pub(crate) host_ip: Option<String>,
    pub(crate) qos_class: Option<String>,
    pub(crate) restart_policy: Option<String>,
    pub(crate) service_account_name: Option<String>,
    pub(crate) dns_policy: Option<String>,
    pub(crate) containers: Vec<PodContainerDetail>,
    /// All declared containers, including init and ephemeral containers, used
    /// by the Pod log action picker.
    pub(crate) log_containers: Vec<PodLogContainer>,
    pub(crate) volumes: Vec<PodVolumeDetail>,
}

#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub(crate) struct NodeDetail {
    pub(crate) pod_cidrs: Vec<String>,
    pub(crate) provider_id: Option<String>,
    pub(crate) unschedulable: bool,
    pub(crate) taints: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct PodConditionDetail {
    pub(crate) type_: String,
    pub(crate) status: String,
    pub(crate) reason: Option<String>,
    pub(crate) message: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct PodVolumeDetail {
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) source: String,
    pub(crate) mount_path: Option<String>,
    pub(crate) read_only: bool,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct PodContainerDetail {
    pub(crate) name: String,
    pub(crate) image: String,
    pub(crate) ready: bool,
    pub(crate) restart_count: i32,
    pub(crate) state: String,
    pub(crate) reason: Option<String>,
    pub(crate) message: Option<String>,
    pub(crate) command: Vec<String>,
    pub(crate) args: Vec<String>,
    pub(crate) ports: Vec<String>,
    pub(crate) environment_variables: Vec<PodEnvironmentVariableDetail>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct PodEnvironmentVariableDetail {
    pub(crate) name: String,
    pub(crate) value: Option<String>,
    pub(crate) source: PodEnvironmentVariableSource,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum PodEnvironmentVariableSource {
    Literal,
    ConfigMapKey {
        name: String,
        key: String,
        optional: bool,
    },
    SecretKey {
        name: String,
        key: String,
        optional: bool,
    },
    Field {
        path: String,
    },
    ResourceField {
        resource: String,
        container_name: Option<String>,
    },
    ConfigMapImport {
        name: String,
        prefix: String,
        optional: bool,
    },
    SecretImport {
        name: String,
        prefix: String,
        optional: bool,
    },
    Unspecified,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct ResourceEvent {
    pub(crate) uid: String,
    pub(crate) type_: String,
    pub(crate) reason: String,
    pub(crate) message: String,
    pub(crate) source: Option<String>,
    pub(crate) count: i32,
    pub(crate) last_timestamp: Option<OffsetDateTime>,
}
