use time::OffsetDateTime;

pub(crate) const READY_COLUMN: &str = "ready";
pub(crate) const CONTAINERS_COLUMN: &str = "containers";
pub(crate) const STATUS_COLUMN: &str = "status";
pub(crate) const RESTARTS_COLUMN: &str = "restarts";
pub(crate) const UP_TO_DATE_COLUMN: &str = "up-to-date";
pub(crate) const AVAILABLE_COLUMN: &str = "available";
pub(crate) const DESIRED_COLUMN: &str = "desired";
pub(crate) const CURRENT_COLUMN: &str = "current";
pub(crate) const COMPLETIONS_COLUMN: &str = "completions";
pub(crate) const TYPE_COLUMN: &str = "type";
pub(crate) const CLUSTER_IP_COLUMN: &str = "cluster-ip";
pub(crate) const PORTS_COLUMN: &str = "ports";
pub(crate) const SCHEDULE_COLUMN: &str = "schedule";
pub(crate) const SUSPEND_COLUMN: &str = "suspend";
pub(crate) const ACTIVE_COLUMN: &str = "active";
pub(crate) const ROLES_COLUMN: &str = "roles";
pub(crate) const VERSION_COLUMN: &str = "version";
pub(crate) const CAPACITY_COLUMN: &str = "capacity";
pub(crate) const ACCESS_MODES_COLUMN: &str = "access-modes";
pub(crate) const RECLAIM_POLICY_COLUMN: &str = "reclaim-policy";
pub(crate) const PROVISIONER_COLUMN: &str = "provisioner";
pub(crate) const BINDING_MODE_COLUMN: &str = "binding-mode";

/// A locally rendered CRD printer column. The worker evaluates its JSONPath against
/// each dynamic object instead of requesting Kubernetes' Table representation.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct CustomResourceColumn {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) json_path: String,
    pub(crate) type_: String,
    pub(crate) format: Option<String>,
}

/// A value transported from the Kubernetes worker to the UI for one table cell.
///
/// Values are semantic rather than egui-specific so the worker remains independent
/// from rendering concerns.
#[allow(dead_code)] // Timestamp and List support future resource definitions.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum CellValue {
    Text(String),
    Number(i64),
    Timestamp(OffsetDateTime),
    Status { label: String, tone: StatusTone },
    ContainerIndicators(Vec<ContainerIndicator>),
    List(Vec<String>),
    Empty,
}

/// A compact container state summary transported from the Kubernetes worker to
/// the Pod table. Rendering stays in the UI, while Kubernetes-specific state
/// interpretation remains in the worker-side extractor.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct ContainerIndicator {
    pub(crate) name: String,
    pub(crate) kind: ContainerKind,
    pub(crate) state: String,
    pub(crate) reason: Option<String>,
    pub(crate) message: Option<String>,
    pub(crate) ready: bool,
    pub(crate) restart_count: i32,
    pub(crate) tone: StatusTone,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum ContainerKind {
    Init,
    App,
    Ephemeral,
}

impl ContainerKind {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Init => "Init container",
            Self::App => "Container",
            Self::Ephemeral => "Ephemeral container",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum StatusTone {
    Neutral,
    Success,
    Warning,
    Danger,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ResourceColumn {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) initial_width: f32,
    pub(crate) sortable: bool,
}

/// The local, extensible definition of a resource's data columns.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct ResourceTableDefinition {
    pub(crate) columns: Vec<ResourceColumn>,
}

pub(crate) fn column(id: &str, label: &str, initial_width: f32) -> ResourceColumn {
    ResourceColumn {
        id: id.to_owned(),
        label: label.to_owned(),
        initial_width,
        sortable: false,
    }
}

pub(crate) fn custom_table_definition(
    custom_columns: &[CustomResourceColumn],
) -> ResourceTableDefinition {
    ResourceTableDefinition {
        columns: custom_columns
            .iter()
            .map(|column| ResourceColumn {
                id: column.id.clone(),
                label: column.label.clone(),
                initial_width: 120.0,
                sortable: false,
            })
            .collect(),
    }
}

pub(crate) fn status_tone(status: &str) -> StatusTone {
    match status {
        "Running" | "Succeeded" | "Active" | "Bound" | "Ready" => StatusTone::Success,
        "Pending" | "ContainerCreating" | "Terminating" => StatusTone::Warning,
        "Failed" | "Unknown" | "NotReady" => StatusTone::Danger,
        _ => StatusTone::Neutral,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_columns_replace_the_empty_dynamic_resource_definition() {
        let columns = vec![CustomResourceColumn {
            id: "crd-0".to_owned(),
            label: "State".to_owned(),
            json_path: ".status.state".to_owned(),
            type_: "string".to_owned(),
            format: None,
        }];

        let definition = custom_table_definition(&columns);

        assert_eq!(definition.columns[0].id, "crd-0");
        assert_eq!(definition.columns[0].label, "State");
    }
}
