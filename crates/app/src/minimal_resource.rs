use crate::resource_detail::ResourceOwner;
use crate::resource_table::{CellValue, ContainerKind};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::Resource;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use time::OffsetDateTime;

const DELETING_CELL: &str = "__resource-deleting";
const FINALIZERS_CELL: &str = "__resource-finalizers";

/// A lightweight representation of any Kubernetes resource for UI display.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MinimalResource {
    /// Unique identifier (metadata.uid or fallback to namespace/name)
    pub uid: String,
    /// Resource name
    pub name: String,
    /// Namespace (None for cluster-scoped resources)
    pub namespace: Option<String>,
    /// Creation timestamp
    pub creation_timestamp: Option<OffsetDateTime>,
    /// The Kubernetes controller owner, if this resource has one.
    pub controller_owner: Option<ResourceOwner>,
    /// Type-specific values keyed by the selected resource table definition.
    pub cells: BTreeMap<String, CellValue>,
    /// Declared Pod containers that can be selected for log streaming. This is
    /// empty for all non-Pod resources.
    pub log_containers: Vec<PodLogContainer>,
}

/// A declared Pod container available as a log-stream target.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct PodLogContainer {
    pub(crate) name: String,
    pub(crate) kind: ContainerKind,
}

impl Ord for MinimalResource {
    fn cmp(&self, other: &Self) -> Ordering {
        self.name.to_lowercase().cmp(&other.name.to_lowercase())
    }
}

impl PartialOrd for MinimalResource {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl MinimalResource {
    pub(crate) fn with_lifecycle_metadata(
        mut self,
        is_deleting: bool,
        finalizers: Vec<String>,
    ) -> Self {
        if is_deleting {
            self.cells
                .insert(DELETING_CELL.into(), CellValue::Text("true".into()));
        }
        if !finalizers.is_empty() {
            self.cells
                .insert(FINALIZERS_CELL.into(), CellValue::List(finalizers));
        }
        self
    }

    /// Whether Kubernetes has accepted deletion but still retains the object.
    pub(crate) fn is_deleting(&self) -> bool {
        matches!(self.cells.get(DELETING_CELL), Some(CellValue::Text(value)) if value == "true")
    }

    /// Finalizers blocking deletion, when this resource is pending deletion.
    pub(crate) fn finalizers(&self) -> &[String] {
        match self.cells.get(FINALIZERS_CELL) {
            Some(CellValue::List(finalizers)) => finalizers,
            _ => &[],
        }
    }

    pub(crate) fn can_force_delete(&self) -> bool {
        self.is_deleting() && !self.finalizers().is_empty()
    }

    /// Calculate age from creation_timestamp as human-readable string.
    pub fn age(&self) -> String {
        format_age(self.creation_timestamp)
    }
}

/// Projects metadata plus resource-specific table cells into the worker-to-UI
/// resource model. Keeping this beside `MinimalResource` lets typed handlers
/// stay independent of connection and watcher orchestration.
pub(crate) fn from_kubernetes_resource<T: Resource>(
    resource: &T,
    cells: BTreeMap<String, CellValue>,
) -> MinimalResource {
    let metadata = resource.meta();
    let creation_timestamp = metadata.creation_timestamp.as_ref().and_then(|timestamp| {
        OffsetDateTime::parse(
            &timestamp.0.to_string(),
            &time::format_description::well_known::Rfc3339,
        )
        .ok()
    });
    let uid = metadata.uid.clone().unwrap_or_else(|| {
        format!(
            "{}/{}",
            metadata.namespace.as_deref().unwrap_or_default(),
            metadata.name.as_deref().unwrap_or_default()
        )
    });

    MinimalResource {
        uid,
        name: metadata.name.clone().unwrap_or_default(),
        namespace: metadata.namespace.clone(),
        creation_timestamp,
        controller_owner: controller_owner(metadata),
        cells,
        log_containers: Vec::new(),
    }
    .with_lifecycle_metadata(
        metadata.deletion_timestamp.is_some(),
        metadata.finalizers.clone().unwrap_or_default(),
    )
}

fn controller_owner(metadata: &ObjectMeta) -> Option<ResourceOwner> {
    metadata
        .owner_references
        .as_deref()
        .unwrap_or_default()
        .iter()
        .find(|owner| owner.controller == Some(true))
        .map(|owner| ResourceOwner {
            api_version: owner.api_version.clone(),
            kind: owner.kind.clone(),
            name: owner.name.clone(),
            uid: owner.uid.clone(),
            controller: true,
        })
}

pub(crate) fn format_age(creation_timestamp: Option<OffsetDateTime>) -> String {
    match creation_timestamp {
        Some(ts) => {
            let now = OffsetDateTime::now_utc();
            let duration = now - ts;
            format_duration(duration)
        }
        None => "Unknown".to_string(),
    }
}

fn format_duration(duration: time::Duration) -> String {
    let seconds = duration.whole_seconds();
    if seconds < 0 {
        return "0s".to_string();
    }

    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;

    if days > 0 {
        format!("{}d", days)
    } else if hours > 0 {
        format!("{}h", hours)
    } else if minutes > 0 {
        format!("{}m", minutes)
    } else {
        format!("{}s", secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_age_formatting() {
        let now = OffsetDateTime::now_utc();

        // 2 hours ago
        let resource = MinimalResource {
            uid: "test".to_string(),
            name: "test-pod".to_string(),
            namespace: Some("default".to_string()),
            creation_timestamp: Some(now - time::Duration::hours(2)),
            controller_owner: None,
            cells: BTreeMap::new(),
            log_containers: Vec::new(),
        };
        assert_eq!(resource.age(), "2h");

        // 3 days ago
        let resource = MinimalResource {
            uid: "test".to_string(),
            name: "test-pod".to_string(),
            namespace: Some("default".to_string()),
            creation_timestamp: Some(now - time::Duration::days(3)),
            controller_owner: None,
            cells: BTreeMap::new(),
            log_containers: Vec::new(),
        };
        assert_eq!(resource.age(), "3d");

        // 45 minutes ago
        let resource = MinimalResource {
            uid: "test".to_string(),
            name: "test-pod".to_string(),
            namespace: Some("default".to_string()),
            creation_timestamp: Some(now - time::Duration::minutes(45)),
            controller_owner: None,
            cells: BTreeMap::new(),
            log_containers: Vec::new(),
        };
        assert_eq!(resource.age(), "45m");

        // 30 seconds ago
        let resource = MinimalResource {
            uid: "test".to_string(),
            name: "test-pod".to_string(),
            namespace: Some("default".to_string()),
            creation_timestamp: Some(now - time::Duration::seconds(30)),
            controller_owner: None,
            cells: BTreeMap::new(),
            log_containers: Vec::new(),
        };
        assert_eq!(resource.age(), "30s");
    }

    #[test]
    fn test_unknown_age() {
        let resource = MinimalResource {
            uid: "test".to_string(),
            name: "test-pod".to_string(),
            namespace: None,
            creation_timestamp: None,
            controller_owner: None,
            cells: BTreeMap::new(),
            log_containers: Vec::new(),
        };
        assert_eq!(resource.age(), "Unknown");
    }

    #[test]
    fn force_delete_requires_a_deleting_resource_with_finalizers() {
        let resource = MinimalResource {
            uid: "test".to_string(),
            name: "test-resource".to_string(),
            namespace: None,
            creation_timestamp: None,
            controller_owner: None,
            cells: BTreeMap::new(),
            log_containers: Vec::new(),
        };
        assert!(!resource.can_force_delete());
        assert!(
            !resource
                .clone()
                .with_lifecycle_metadata(false, vec!["example.com/cleanup".into()])
                .can_force_delete()
        );
        assert!(
            !resource
                .clone()
                .with_lifecycle_metadata(true, Vec::new())
                .can_force_delete()
        );
        assert!(
            resource
                .with_lifecycle_metadata(true, vec!["example.com/cleanup".into()])
                .can_force_delete()
        );
    }
}
