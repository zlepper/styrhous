use crate::resource_detail::ResourceOwner;
use crate::resource_table::{CellValue, ContainerKind, Tone};
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
    /// Resource metadata available for user-configured table columns.
    pub labels: BTreeMap<String, String>,
    /// Resource metadata available for user-configured table columns.
    pub annotations: BTreeMap<String, String>,
    /// Type-specific values keyed by the selected resource table definition.
    pub cells: BTreeMap<String, CellValue>,
    /// Declared Pod containers that can be selected for log streaming. This is
    /// empty for all non-Pod resources.
    pub log_containers: Vec<PodLogContainer>,
}

/// A declared Pod container available as a log-stream or debug-shell target.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct PodLogContainer {
    pub(crate) name: String,
    pub(crate) kind: ContainerKind,
    /// The declared container image, when Kubernetes supplied one. Debug-shell
    /// menus use this to offer images already used by the Pod.
    pub(crate) image: Option<String>,
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
    /// How badly the resource is doing, when it is not well: the worst tone
    /// among its status cells, if that is a warning or danger (the problem
    /// rows of the tables, S17).
    pub(crate) fn problem_tone(&self) -> Option<Tone> {
        self.cells
            .values()
            .filter_map(|cell| match cell {
                CellValue::Status { tone, .. } => Some(*tone),
                _ => None,
            })
            .filter(|tone| matches!(tone, Tone::Warning | Tone::Danger))
            .max_by_key(|tone| matches!(tone, Tone::Danger))
    }

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
}

/// Projects metadata plus resource-specific table cells into the worker-to-UI
/// resource model. Keeping this beside `MinimalResource` lets typed handlers
/// stay independent of connection and watcher orchestration.
pub(crate) fn from_kubernetes_resource<T: Resource>(
    resource: &T,
    cells: BTreeMap<String, CellValue>,
) -> MinimalResource {
    let metadata = resource.meta();
    let creation_timestamp = creation_time(metadata);
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
        labels: metadata.labels.clone().unwrap_or_default(),
        annotations: metadata.annotations.clone().unwrap_or_default(),
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

/// The object's creation timestamp, when it is set and valid RFC 3339.
pub(crate) fn creation_time(metadata: &ObjectMeta) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(
        &metadata.creation_timestamp.as_ref()?.0.to_string(),
        &time::format_description::well_known::Rfc3339,
    )
    .ok()
}

/// How long before `now` an object was created ("220d", "3h"), or "Unknown".
pub(crate) fn format_age(
    creation_timestamp: Option<OffsetDateTime>,
    now: OffsetDateTime,
) -> String {
    match creation_timestamp {
        Some(ts) => format_duration(now - ts),
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
    use k8s_openapi::api::core::v1::Pod;

    fn resource_with_statuses(tones: &[Tone]) -> MinimalResource {
        MinimalResource {
            uid: "uid".into(),
            name: "pod".into(),
            namespace: Some("default".into()),
            creation_timestamp: None,
            controller_owner: None,
            labels: BTreeMap::new(),
            annotations: BTreeMap::new(),
            cells: tones
                .iter()
                .enumerate()
                .map(|(index, tone)| {
                    (
                        format!("status-{index}"),
                        CellValue::Status {
                            label: "Status".into(),
                            tone: *tone,
                        },
                    )
                })
                .collect(),
            log_containers: Vec::new(),
        }
    }

    #[test]
    fn a_resource_has_a_problem_only_when_a_status_warns_and_the_worst_wins() {
        assert_eq!(resource_with_statuses(&[]).problem_tone(), None);
        assert_eq!(
            resource_with_statuses(&[Tone::Success, Tone::Neutral]).problem_tone(),
            None
        );
        assert_eq!(
            resource_with_statuses(&[Tone::Warning]).problem_tone(),
            Some(Tone::Warning)
        );
        assert_eq!(
            resource_with_statuses(&[Tone::Warning, Tone::Danger, Tone::Success]).problem_tone(),
            Some(Tone::Danger)
        );
    }

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
            labels: BTreeMap::new(),
            annotations: BTreeMap::new(),
            cells: BTreeMap::new(),
            log_containers: Vec::new(),
        };
        assert_eq!(format_age(resource.creation_timestamp, now), "2h");

        // 3 days ago
        let resource = MinimalResource {
            uid: "test".to_string(),
            name: "test-pod".to_string(),
            namespace: Some("default".to_string()),
            creation_timestamp: Some(now - time::Duration::days(3)),
            controller_owner: None,
            labels: BTreeMap::new(),
            annotations: BTreeMap::new(),
            cells: BTreeMap::new(),
            log_containers: Vec::new(),
        };
        assert_eq!(format_age(resource.creation_timestamp, now), "3d");

        // 45 minutes ago
        let resource = MinimalResource {
            uid: "test".to_string(),
            name: "test-pod".to_string(),
            namespace: Some("default".to_string()),
            creation_timestamp: Some(now - time::Duration::minutes(45)),
            controller_owner: None,
            labels: BTreeMap::new(),
            annotations: BTreeMap::new(),
            cells: BTreeMap::new(),
            log_containers: Vec::new(),
        };
        assert_eq!(format_age(resource.creation_timestamp, now), "45m");

        // 30 seconds ago
        let resource = MinimalResource {
            uid: "test".to_string(),
            name: "test-pod".to_string(),
            namespace: Some("default".to_string()),
            creation_timestamp: Some(now - time::Duration::seconds(30)),
            controller_owner: None,
            labels: BTreeMap::new(),
            annotations: BTreeMap::new(),
            cells: BTreeMap::new(),
            log_containers: Vec::new(),
        };
        assert_eq!(format_age(resource.creation_timestamp, now), "30s");
    }

    #[test]
    fn test_unknown_age() {
        let resource = MinimalResource {
            uid: "test".to_string(),
            name: "test-pod".to_string(),
            namespace: None,
            creation_timestamp: None,
            controller_owner: None,
            labels: BTreeMap::new(),
            annotations: BTreeMap::new(),
            cells: BTreeMap::new(),
            log_containers: Vec::new(),
        };
        assert_eq!(
            format_age(resource.creation_timestamp, OffsetDateTime::now_utc()),
            "Unknown"
        );
    }

    #[test]
    fn force_delete_requires_a_deleting_resource_with_finalizers() {
        let resource = MinimalResource {
            uid: "test".to_string(),
            name: "test-resource".to_string(),
            namespace: None,
            creation_timestamp: None,
            controller_owner: None,
            labels: BTreeMap::new(),
            annotations: BTreeMap::new(),
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

    #[test]
    fn extraction_preserves_labels_and_annotations_for_table_columns() {
        let pod = Pod {
            metadata: ObjectMeta {
                name: Some("api".into()),
                labels: Some(BTreeMap::from([("app".into(), "api".into())])),
                annotations: Some(BTreeMap::from([(
                    "example.com/team".into(),
                    "platform".into(),
                )])),
                ..Default::default()
            },
            ..Default::default()
        };

        let resource = from_kubernetes_resource(&pod, BTreeMap::new());

        assert_eq!(resource.labels["app"], "api");
        assert_eq!(resource.annotations["example.com/team"], "platform");
    }
}
