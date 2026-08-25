//! Kubernetes Metrics API data and resource-quantity handling.

use anyhow::{Result, anyhow};
use k8s_openapi::serde_json;
use serde::Deserialize;
use std::collections::BTreeMap;
use time::OffsetDateTime;

/// The cadence offered by metrics-server. Polling more frequently does not produce fresher
/// samples, and this interval keeps the namespace table and open inspector in sync.
pub(crate) const POD_METRICS_POLL_INTERVAL: std::time::Duration =
    std::time::Duration::from_secs(15);
/// The bounded period retained for compact inspector charts.
pub(crate) const POD_USAGE_HISTORY_WINDOW: time::Duration = time::Duration::minutes(10);
const NANOCORES_PER_CORE: i64 = 1_000_000_000;

/// A normalized current resource-use sample. CPU is nanocores and memory is bytes so the
/// presentation layer can sort and aggregate without depending on quantity spelling.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct PodUsage {
    pub(crate) timestamp: OffsetDateTime,
    pub(crate) cpu_nanocores: i64,
    pub(crate) memory_bytes: i64,
    pub(crate) containers: BTreeMap<String, ContainerUsage>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct ContainerUsage {
    pub(crate) cpu_nanocores: i64,
    pub(crate) memory_bytes: i64,
}

/// A normalized current node-use sample. CPU is nanocores and memory is bytes.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct NodeUsage {
    pub(crate) timestamp: OffsetDateTime,
    pub(crate) cpu_nanocores: i64,
    pub(crate) memory_bytes: i64,
}

#[derive(Debug, Deserialize)]
struct PodMetrics {
    metadata: MetricsMetadata,
    timestamp: String,
    containers: Vec<ContainerMetrics>,
}

#[derive(Debug, Deserialize)]
struct MetricsMetadata {
    name: String,
}

#[derive(Debug, Deserialize)]
struct ContainerMetrics {
    name: String,
    usage: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct NodeMetrics {
    metadata: MetricsMetadata,
    timestamp: String,
    usage: BTreeMap<String, String>,
}

pub(crate) fn pod_usage_from_value(value: serde_json::Value) -> Result<(String, PodUsage)> {
    let metrics: PodMetrics = serde_json::from_value(value)?;
    let timestamp = OffsetDateTime::parse(
        &metrics.timestamp,
        &time::format_description::well_known::Rfc3339,
    )?;
    let mut cpu_nanocores = 0_i64;
    let mut memory_bytes = 0_i64;
    let mut containers = BTreeMap::new();
    for container in metrics.containers {
        let usage = ContainerUsage {
            cpu_nanocores: parse_cpu_nanocores(required_usage(&container.usage, "cpu")?)?,
            memory_bytes: parse_memory_bytes(required_usage(&container.usage, "memory")?)?,
        };
        cpu_nanocores = cpu_nanocores
            .checked_add(usage.cpu_nanocores)
            .ok_or_else(|| anyhow!("aggregated CPU usage overflowed"))?;
        memory_bytes = memory_bytes
            .checked_add(usage.memory_bytes)
            .ok_or_else(|| anyhow!("aggregated memory usage overflowed"))?;
        containers.insert(container.name, usage);
    }
    Ok((
        metrics.metadata.name,
        PodUsage {
            timestamp,
            cpu_nanocores,
            memory_bytes,
            containers,
        },
    ))
}

pub(crate) fn node_usage_from_value(value: serde_json::Value) -> Result<(String, NodeUsage)> {
    let metrics: NodeMetrics = serde_json::from_value(value)?;
    let timestamp = OffsetDateTime::parse(
        &metrics.timestamp,
        &time::format_description::well_known::Rfc3339,
    )?;
    Ok((
        metrics.metadata.name,
        NodeUsage {
            timestamp,
            cpu_nanocores: parse_cpu_nanocores(required_usage(&metrics.usage, "cpu")?)?,
            memory_bytes: parse_memory_bytes(required_usage(&metrics.usage, "memory")?)?,
        },
    ))
}

fn required_usage<'a>(usage: &'a BTreeMap<String, String>, name: &str) -> Result<&'a str> {
    usage
        .get(name)
        .map(String::as_str)
        .ok_or_else(|| anyhow!("metrics entry did not include {name} usage"))
}

pub(crate) fn parse_cpu_nanocores(value: &str) -> Result<i64> {
    parse_quantity(
        value,
        &[
            ("n", 1.0),
            ("u", 1_000.0),
            ("m", 1_000_000.0),
            ("k", 1_000_000_000_000.0),
            ("M", 1_000_000_000_000_000.0),
            ("G", 1_000_000_000_000_000_000.0),
        ],
        1_000_000_000.0,
    )
}

pub(crate) fn parse_memory_bytes(value: &str) -> Result<i64> {
    let binary = [
        ("Ki", 1024.0),
        ("Mi", 1024.0_f64.powi(2)),
        ("Gi", 1024.0_f64.powi(3)),
        ("Ti", 1024.0_f64.powi(4)),
        ("Pi", 1024.0_f64.powi(5)),
        ("Ei", 1024.0_f64.powi(6)),
    ];
    if let Some((number, multiplier)) = binary.iter().find_map(|(suffix, multiplier)| {
        value
            .strip_suffix(suffix)
            .map(|number| (number, multiplier))
    }) {
        return scaled_number(number, *multiplier, value);
    }
    parse_quantity(
        value,
        &[
            ("n", 0.000_000_001),
            ("u", 0.000_001),
            ("m", 0.001),
            ("k", 1_000.0),
            ("M", 1_000_000.0),
            ("G", 1_000_000_000.0),
            ("T", 1_000_000_000_000.0),
            ("P", 1_000_000_000_000_000.0),
            ("E", 1_000_000_000_000_000_000.0),
        ],
        1.0,
    )
}

fn parse_quantity(value: &str, suffixes: &[(&str, f64)], default_multiplier: f64) -> Result<i64> {
    if let Some((suffix, multiplier)) = suffixes.iter().find_map(|(suffix, multiplier)| {
        value
            .strip_suffix(suffix)
            .map(|number| (number, multiplier))
    }) {
        scaled_number(suffix, *multiplier, value)
    } else {
        scaled_number(value, default_multiplier, value)
    }
}

fn scaled_number(number: &str, multiplier: f64, original: &str) -> Result<i64> {
    let number = number
        .parse::<f64>()
        .map_err(|_| anyhow!("invalid Kubernetes quantity {original:?}"))?;
    let scaled = number * multiplier;
    if !scaled.is_finite() || scaled < 0.0 || scaled > i64::MAX as f64 {
        return Err(anyhow!(
            "Kubernetes quantity {original:?} is outside supported range"
        ));
    }
    Ok(scaled.round() as i64)
}

pub(crate) fn format_cpu(cpu_nanocores: i64) -> String {
    if cpu_nanocores >= NANOCORES_PER_CORE {
        format_decimal(cpu_nanocores as f64 / NANOCORES_PER_CORE as f64, "")
    } else if cpu_nanocores >= 1_000_000 {
        format_decimal(cpu_nanocores as f64 / 1_000_000.0, "m")
    } else {
        format_decimal(cpu_nanocores as f64 / 1_000.0, "µ")
    }
}

pub(crate) fn format_cpu_cores(cpu_nanocores: i64) -> String {
    format!("{:.3}", cpu_nanocores as f64 / NANOCORES_PER_CORE as f64)
}

pub(crate) fn format_memory(memory_bytes: i64) -> String {
    const UNITS: [(&str, i64); 6] = [
        ("Ei", 1_i64 << 60),
        ("Pi", 1_i64 << 50),
        ("Ti", 1_i64 << 40),
        ("Gi", 1_i64 << 30),
        ("Mi", 1_i64 << 20),
        ("Ki", 1_i64 << 10),
    ];
    for (suffix, divisor) in UNITS {
        if memory_bytes >= divisor {
            return format_decimal(memory_bytes as f64 / divisor as f64, suffix);
        }
    }
    format!("{memory_bytes}B")
}

fn format_decimal(value: f64, suffix: &str) -> String {
    let value = (value * 10.0).round() / 10.0;
    if value.fract() == 0.0 {
        format!("{}{suffix}", value as i64)
    } else {
        format!("{value:.1}{suffix}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_aggregates_pod_metrics() {
        let (name, usage) = pod_usage_from_value(serde_json::json!({
            "metadata": { "name": "api" },
            "timestamp": "2026-08-14T09:00:00Z",
            "containers": [
                { "name": "app", "usage": { "cpu": "12m", "memory": "32Mi" } },
                { "name": "sidecar", "usage": { "cpu": "500u", "memory": "512Ki" } }
            ]
        }))
        .unwrap();
        assert_eq!(name, "api");
        assert_eq!(usage.cpu_nanocores, 12_500_000);
        assert_eq!(usage.memory_bytes, 32 * 1024 * 1024 + 512 * 1024);
        assert_eq!(format_cpu(usage.cpu_nanocores), "12.5m");
        assert_eq!(format_memory(usage.memory_bytes), "32.5Mi");
    }

    #[test]
    fn formats_cpu_cores_with_fixed_three_decimal_precision() {
        assert_eq!(format_cpu_cores(125_000_000), "0.125");
        assert_eq!(format_cpu_cores(1_000_000_000), "1.000");
        assert_eq!(format_cpu_cores(12_500_000_000), "12.500");
        assert_eq!(format_cpu_cores(999_500_000), "1.000");
    }

    #[test]
    fn rejects_missing_or_invalid_usage() {
        assert!(parse_cpu_nanocores("nope").is_err());
        assert!(
            pod_usage_from_value(serde_json::json!({
                "metadata": { "name": "api" },
                "timestamp": "2026-08-14T09:00:00Z",
                "containers": [{ "name": "app", "usage": { "cpu": "1m" } }]
            }))
            .is_err()
        );
    }

    #[test]
    fn parses_node_metrics() {
        let (name, usage) = node_usage_from_value(serde_json::json!({
            "metadata": { "name": "worker-a" },
            "timestamp": "2026-08-14T09:00:00Z",
            "usage": { "cpu": "1500m", "memory": "3Gi" }
        }))
        .unwrap();

        assert_eq!(name, "worker-a");
        assert_eq!(usage.cpu_nanocores, 1_500_000_000);
        assert_eq!(usage.memory_bytes, 3 * 1024 * 1024 * 1024);
    }
}
