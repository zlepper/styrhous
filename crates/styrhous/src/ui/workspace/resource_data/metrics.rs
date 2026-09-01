use super::ResourceMetrics;
use crate::minimal_resource::MinimalResource;
use crate::pod_metrics::{format_cpu_cores, format_memory};
use crate::resource_table::{CPU_COLUMN, CellValue, MEMORY_COLUMN};

pub(in crate::ui::workspace) fn resolved_resource_cell(
    resource: &MinimalResource,
    column_id: &str,
    metrics: ResourceMetrics<'_>,
    api_resource: &crate::api_resource::ApiResource,
) -> Option<CellValue> {
    if column_id != CPU_COLUMN && column_id != MEMORY_COLUMN {
        return None;
    }
    let is_cpu = column_id == CPU_COLUMN;
    if api_resource.group == "core" && api_resource.kind == "Node" {
        if !metrics.node_metrics_api_available || metrics.node_metrics.error.is_some() {
            return Some(CellValue::Text("Unavailable".into()));
        }
        return metrics
            .node_metrics
            .usages
            .get(&resource.name)
            .map(|usage| usage_cell(is_cpu, usage.cpu_nanocores, usage.memory_bytes));
    }
    if api_resource.group != "core" || api_resource.kind != "Pod" {
        return None;
    }
    let namespace = resource.namespace.as_deref()?;
    let namespace_metrics = metrics.pod_metrics.get(namespace);
    if !metrics.pod_metrics_api_available
        || namespace_metrics.is_some_and(|metrics| metrics.error.is_some())
    {
        return Some(CellValue::Text("Unavailable".into()));
    }
    namespace_metrics
        .and_then(|metrics| metrics.usages.get(&resource.name))
        .map(|usage| usage_cell(is_cpu, usage.cpu_nanocores, usage.memory_bytes))
}

fn usage_cell(is_cpu: bool, cpu_nanocores: i64, memory_bytes: i64) -> CellValue {
    CellValue::Usage {
        label: if is_cpu {
            format_cpu_cores(cpu_nanocores)
        } else {
            format_memory(memory_bytes)
        },
        value: if is_cpu { cpu_nanocores } else { memory_bytes },
    }
}
