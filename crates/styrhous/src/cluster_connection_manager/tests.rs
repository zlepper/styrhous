use super::*;
use crate::resource_table::{
    AVAILABLE_COLUMN, READY_COLUMN, RESTARTS_COLUMN, STATUS_COLUMN, UP_TO_DATE_COLUMN,
};
use k8s_openapi::api::apps::v1::{Deployment, DeploymentStatus};
use k8s_openapi::api::batch::v1::{CronJobSpec, JobSpec, JobTemplateSpec};
use k8s_openapi::api::core::v1::{ContainerStatus, Pod, PodStatus};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{APIResource, ObjectMeta, OwnerReference};

mod capabilities;
mod extractors;
mod helm_and_jobs;
mod watches;
