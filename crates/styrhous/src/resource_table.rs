use time::OffsetDateTime;

pub(crate) const READY_COLUMN: &str = "ready";
pub(crate) const CONTAINERS_COLUMN: &str = "containers";
pub(crate) const STATUS_COLUMN: &str = "status";
pub(crate) const RESTARTS_COLUMN: &str = "restarts";
pub(crate) const NODE_COLUMN: &str = "node";
pub(crate) const CPU_COLUMN: &str = "cpu";
pub(crate) const MEMORY_COLUMN: &str = "memory";
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
    /// A quantity whose rendered label differs from the normalized numeric sort value.
    Usage {
        label: String,
        value: i64,
    },
    Timestamp(OffsetDateTime),
    Status {
        label: String,
        tone: StatusTone,
    },
    ContainerIndicators(Vec<ContainerIndicator>),
    List(Vec<String>),
    Empty,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum SortValue {
    Empty,
    Number(i64),
    Text(String),
}

pub(crate) fn cell_sort_value(value: &CellValue) -> SortValue {
    match value {
        CellValue::Text(value) => SortValue::Text(value.clone()),
        CellValue::Number(value) => SortValue::Number(*value),
        CellValue::Usage { value, .. } => SortValue::Number(*value),
        CellValue::Timestamp(value) => SortValue::Number(value.unix_timestamp()),
        CellValue::Status { label, .. } => SortValue::Text(label.clone()),
        CellValue::ContainerIndicators(values) => SortValue::Text(
            values
                .iter()
                .map(|value| format!("{}:{}", value.name, value.state))
                .collect::<Vec<_>>()
                .join(","),
        ),
        CellValue::List(values) => SortValue::Text(values.join(",")),
        CellValue::Empty => SortValue::Empty,
    }
}

pub(crate) fn compare_sort_values(
    left: SortValue,
    right: SortValue,
    direction: components::SortDirection,
) -> std::cmp::Ordering {
    let ordering = match (&left, &right) {
        (SortValue::Empty, SortValue::Empty) => std::cmp::Ordering::Equal,
        (SortValue::Empty, _) => std::cmp::Ordering::Greater,
        (_, SortValue::Empty) => std::cmp::Ordering::Less,
        (SortValue::Number(left), SortValue::Number(right)) => left.cmp(right),
        (SortValue::Text(left), SortValue::Text(right)) => left.cmp(right),
        (SortValue::Number(left), SortValue::Text(right)) => left.to_string().cmp(right),
        (SortValue::Text(left), SortValue::Number(right)) => left.cmp(&right.to_string()),
    };
    if direction == components::SortDirection::Descending
        && !matches!(left, SortValue::Empty)
        && !matches!(right, SortValue::Empty)
    {
        ordering.reverse()
    } else {
        ordering
    }
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

    #[test]
    fn numeric_sort_values_sort_numerically() {
        assert_eq!(
            compare_sort_values(
                SortValue::Number(9),
                SortValue::Number(10),
                components::SortDirection::Ascending,
            ),
            std::cmp::Ordering::Less,
        );
    }
}
