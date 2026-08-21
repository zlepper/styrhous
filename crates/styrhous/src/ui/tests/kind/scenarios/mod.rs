use super::super::super::state::ClusterConnectionState;
use super::support::{self, *};
use crate::pod_metrics::{format_cpu, format_memory};
use crate::resource_table::{READY_COLUMN, STATUS_COLUMN};
use crate::sorted_name::SortedName;
use egui_kittest::kittest::{NodeT, Queryable};
use k8s_openapi::api::apps::v1::{Deployment, DeploymentSpec};
use k8s_openapi::api::batch::v1::{CronJob, CronJobSpec, Job, JobSpec, JobTemplateSpec};
use k8s_openapi::api::core::v1::{ConfigMap, Secret};
use k8s_openapi::api::core::v1::{Container, Pod, PodSpec, PodTemplateSpec, ResourceRequirements};
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::LabelSelector;
use kube::api::Patch;
use kube::{Api, Client};
use std::collections::BTreeMap;
use std::time::Duration;

const WATCHER_CONFIGMAP_NAME: &str = "resource-watcher";
const ACTIONS_CONFIGMAP_NAME: &str = "resource-actions";
const ACTIONS_SECRET_NAME: &str = "resource-secret-actions";
const METRICS_LOAD_POD_NAME: &str = "metrics-load";
const TEST_FINALIZER: &str = "tests.styrhous/finalizer";

/// Updates an existing Secret value through the inspector without exposing its
/// plaintext until the test explicitly operates on the editor state.
fn yaml_mapping_key_positions(yaml: &str) -> Vec<(usize, String, usize)> {
    let mut line_start = 0;
    let mut positions = Vec::new();
    for (line_number, line) in yaml.lines().enumerate() {
        let leading_whitespace = line.len() - line.trim_start().len();
        let line_after_indent = &line[leading_whitespace..];
        let (dash_prefix, mapping) = line_after_indent
            .strip_prefix("- ")
            .map_or((0, line_after_indent), |mapping| (2, mapping));
        if let Some((key, _)) = mapping.split_once(':')
            && !key.is_empty()
            && key.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
            })
        {
            let cursor = line_start + leading_whitespace + dash_prefix + key.len();
            positions.push((line_number + 1, key.to_owned(), cursor));
        }
        line_start += line.len() + 1;
    }
    positions
}

mod connection_and_metrics;
mod deployment_actions;
mod inspectors_and_deletes;
mod resource_actions;
