use super::ResourceMetrics;
use crate::minimal_resource::MinimalResource;
use crate::pod_metrics::{format_cpu, format_cpu_cores_of, format_memory, format_memory_of};
use crate::resource_table::{CPU_COLUMN, CellValue, MEMORY_COLUMN, PODS_COLUMN};

pub(in crate::ui::workspace) fn resolved_resource_cell(
    resource: &MinimalResource,
    column_id: &str,
    metrics: ResourceMetrics<'_>,
    api_resource: &crate::api_resource::ApiResource,
) -> Option<CellValue> {
    if api_resource.is_node() && column_id == PODS_COLUMN {
        return Some(match metrics.node_pod_counts.count(&resource.name) {
            Ok(Some(count)) => CellValue::Number(count as i64),
            Ok(None) => CellValue::Empty,
            Err(_) => CellValue::Unavailable,
        });
    }
    if column_id != CPU_COLUMN && column_id != MEMORY_COLUMN {
        return None;
    }
    let is_cpu = column_id == CPU_COLUMN;
    if api_resource.is_node() {
        if !metrics.node_metrics_api_available || metrics.node_metrics.error.is_some() {
            return Some(CellValue::Unavailable);
        }
        let allocatable = crate::resource_handlers::node::allocatable(resource);
        let allocatable = if is_cpu {
            allocatable.cpu_nanocores
        } else {
            allocatable.memory_bytes
        };
        return metrics
            .node_metrics
            .usages
            .get(&resource.name)
            .map(|usage| usage_cell(is_cpu, usage.cpu_nanocores, usage.memory_bytes, allocatable));
    }
    if !api_resource.is_pod() {
        return None;
    }
    let namespace = resource.namespace.as_deref()?;
    let namespace_metrics = metrics.pod_metrics.get(namespace);
    if !metrics.pod_metrics_api_available
        || namespace_metrics.is_some_and(|metrics| metrics.error.is_some())
    {
        return Some(CellValue::Unavailable);
    }
    namespace_metrics
        .and_then(|metrics| metrics.usages.get(&resource.name))
        .map(|usage| usage_cell(is_cpu, usage.cpu_nanocores, usage.memory_bytes, None))
}

/// The CPU or memory in use, of `allocatable` when that is known.
fn usage_cell(
    is_cpu: bool,
    cpu_nanocores: i64,
    memory_bytes: i64,
    allocatable: Option<i64>,
) -> CellValue {
    let value = if is_cpu { cpu_nanocores } else { memory_bytes };
    let label = match (is_cpu, allocatable) {
        (true, Some(allocatable)) => format_cpu_cores_of(value, allocatable),
        (false, Some(allocatable)) => format_memory_of(value, allocatable),
        (true, None) => format_cpu(value),
        (false, None) => format_memory(value),
    };
    let permille = allocatable
        .filter(|allocatable| *allocatable > 0)
        .map(|allocatable| (value.saturating_mul(1000) / allocatable).clamp(0, 1000) as u16);
    CellValue::Usage {
        label,
        value,
        permille,
    }
}
