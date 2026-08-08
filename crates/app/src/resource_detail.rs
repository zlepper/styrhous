use crate::api_resource::ApiResource;
use std::collections::BTreeMap;
use time::OffsetDateTime;

/// Data shared by every resource detail renderer.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct ResourceDetail {
    pub(crate) api_resource: ApiResource,
    pub(crate) name: String,
    pub(crate) namespace: Option<String>,
    pub(crate) uid: String,
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

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum ResourceDetailPayload {
    Generic,
    Pod(PodDetail),
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
    pub(crate) volumes: Vec<PodVolumeDetail>,
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
