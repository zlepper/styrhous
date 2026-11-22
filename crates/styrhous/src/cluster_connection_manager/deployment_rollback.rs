//! Deployment revision history and rollback, the way `kubectl rollout undo` does it.
//!
//! A Deployment's revisions are the ReplicaSets it controls, numbered by their
//! `deployment.kubernetes.io/revision` annotation. Rolling back copies the
//! chosen ReplicaSet's pod template back into the Deployment; the Deployment
//! controller then records it as a new revision.
//!
//! Objects are loaded untyped so that pod-template fields newer than this
//! client's Kubernetes types take part in comparisons and survive the rollback.
//! The typed form is only used to summarise images, environment and resources.

use crate::minimal_resource::creation_time;
use anyhow::{Context, Result, bail};
use k8s_openapi::api::apps::v1::{Deployment, ReplicaSet};
use k8s_openapi::api::core::v1::{Container, EnvVar, EnvVarSource, PodTemplateSpec};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::api::{DynamicObject, ListParams, PostParams};
use kube::{Api, Resource, ResourceExt};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use time::OffsetDateTime;

pub(crate) const REVISION_ANNOTATION: &str = "deployment.kubernetes.io/revision";
const POD_TEMPLATE_HASH_LABEL: &str = "pod-template-hash";
/// Status updates by the Deployment controller change the resourceVersion
/// constantly during a rollout, so a conflicting replace is retried.
const ROLLBACK_ATTEMPTS: usize = 5;

/// Annotations kubectl keeps from the Deployment instead of copying them from
/// the ReplicaSet, because they describe the Deployment's own rollout state.
const ANNOTATIONS_KEPT_FROM_DEPLOYMENT: [&str; 6] = [
    "kubectl.kubernetes.io/last-applied-configuration",
    REVISION_ANNOTATION,
    "deployment.kubernetes.io/revision-history",
    "deployment.kubernetes.io/desired-replicas",
    "deployment.kubernetes.io/max-replicas",
    "deprecated.deployment.rollback.to",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeploymentRevisionHistory {
    pub(crate) paused: bool,
    /// Newest first, one entry per revision number.
    pub(crate) revisions: Vec<DeploymentRevision>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeploymentRevision {
    pub(crate) revision: i64,
    pub(crate) replica_set: String,
    pub(crate) created: Option<OffsetDateTime>,
    pub(crate) images: Vec<String>,
    pub(crate) is_current: bool,
    /// What rolling back to this revision changes in the current pod template.
    pub(crate) changes: Vec<TemplateChange>,
    /// The templates also differ in ways `changes` does not show.
    pub(crate) other_changes: bool,
}

/// One summarised difference between the current and a previous pod template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TemplateChange {
    pub(crate) container: String,
    /// `image`, `env NAME`, `requests.cpu`, `limits.memory` or `container`.
    pub(crate) field: String,
    pub(crate) current: Option<String>,
    pub(crate) target: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeploymentRollbackOutcome {
    RolledBack,
    /// The Deployment's template already matched the revision (kubectl's "skipped rollback").
    AlreadyAtRevision,
}

fn revision_of(metadata: &ObjectMeta) -> Option<i64> {
    metadata
        .annotations
        .as_ref()?
        .get(REVISION_ANNOTATION)?
        .parse()
        .ok()
}

/// The ReplicaSets `deployment_uid` controls, newest revision first. Should two
/// claim the same revision, the most recently created one wins.
fn owned_revisions(
    deployment_uid: &str,
    mut replica_sets: Vec<DynamicObject>,
) -> Vec<(i64, DynamicObject)> {
    replica_sets.sort_by_key(|replica_set| std::cmp::Reverse(creation_time(&replica_set.metadata)));
    let mut seen = BTreeSet::new();
    let mut revisions = replica_sets
        .into_iter()
        .filter(|replica_set| {
            replica_set
                .metadata
                .owner_references
                .iter()
                .flatten()
                .any(|owner| owner.controller == Some(true) && owner.uid == deployment_uid)
        })
        .filter_map(|replica_set| Some((revision_of(&replica_set.metadata)?, replica_set)))
        .filter(|(revision, _)| seen.insert(*revision))
        .collect::<Vec<_>>();
    revisions.sort_by_key(|(revision, _)| std::cmp::Reverse(*revision));
    revisions
}

fn find_revision(
    deployment: &DynamicObject,
    replica_sets: Vec<DynamicObject>,
    revision: i64,
) -> Result<DynamicObject> {
    owned_revisions(
        deployment.metadata.uid.as_deref().unwrap_or_default(),
        replica_sets,
    )
    .into_iter()
    .find_map(|(candidate, replica_set)| (candidate == revision).then_some(replica_set))
    .with_context(|| {
        format!(
            "Revision {revision} of Deployment {} no longer exists",
            deployment.name_any()
        )
    })
}

fn erased<K: Resource<DynamicType = ()>>() -> kube::discovery::ApiResource {
    kube::discovery::ApiResource::erase::<K>(&())
}

/// The Deployment and every ReplicaSet in its namespace that may belong to it.
async fn load(
    client: kube::Client,
    namespace: &str,
    name: &str,
) -> Result<(DynamicObject, Vec<DynamicObject>)> {
    let deployment =
        Api::<DynamicObject>::namespaced_with(client.clone(), namespace, &erased::<Deployment>())
            .get(name)
            .await
            .with_context(|| format!("Loading Deployment {name}"))?;
    // The selector's labels narrow the list; ownership is checked exactly
    // afterwards, so `matchExpressions` may safely be ignored here.
    let selector = deployment.data["spec"]["selector"]["matchLabels"]
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(key, value)| Some(format!("{key}={}", value.as_str()?)))
        .collect::<Vec<_>>()
        .join(",");
    let replica_sets =
        Api::<DynamicObject>::namespaced_with(client, namespace, &erased::<ReplicaSet>())
            .list(&ListParams::default().labels(&selector))
            .await
            .with_context(|| format!("Listing the ReplicaSets of Deployment {name}"))?
            .items;
    Ok((deployment, replica_sets))
}

pub(crate) async fn deployment_revisions(
    client: kube::Client,
    namespace: &str,
    name: &str,
) -> Result<DeploymentRevisionHistory> {
    let (deployment, replica_sets) = load(client, namespace, name).await?;
    Ok(revision_history(&deployment, replica_sets))
}

/// A pod template as raw JSON, without the label the controller adds per ReplicaSet.
fn template_value(object: &DynamicObject) -> Value {
    let mut template = object.data["spec"]["template"].clone();
    if let Some(labels) = template["metadata"]["labels"].as_object_mut() {
        labels.remove(POD_TEMPLATE_HASH_LABEL);
    }
    template
}

fn typed_template(template: &Value) -> PodTemplateSpec {
    serde_json::from_value(template.clone()).unwrap_or_default()
}

/// The template with the summarised container fields removed, to detect
/// differences the summary does not show.
fn without_summarised_fields(template: &Value) -> Value {
    let mut template = template.clone();
    for list in ["initContainers", "containers"] {
        for container in template["spec"][list].as_array_mut().into_iter().flatten() {
            if let Some(container) = container.as_object_mut() {
                for field in ["image", "env", "resources"] {
                    container.remove(field);
                }
            }
        }
    }
    template
}

fn revision_history(
    deployment: &DynamicObject,
    replica_sets: Vec<DynamicObject>,
) -> DeploymentRevisionHistory {
    let current = template_value(deployment);
    let current_typed = typed_template(&current);
    let current_revision = revision_of(&deployment.metadata);
    let revisions = owned_revisions(
        deployment.metadata.uid.as_deref().unwrap_or_default(),
        replica_sets,
    )
    .into_iter()
    .map(|(revision, replica_set)| {
        let template = template_value(&replica_set);
        let typed = typed_template(&template);
        let changes = template_changes(&current_typed, &typed);
        let other_changes = without_summarised_fields(&current)
            != without_summarised_fields(&template)
            || (changes.is_empty() && current != template);
        DeploymentRevision {
            revision,
            replica_set: replica_set.name_any(),
            created: creation_time(&replica_set.metadata),
            images: containers(&typed)
                .filter_map(|container| container.image.clone())
                .collect(),
            is_current: current_revision.map_or(current == template, |current| current == revision),
            changes,
            other_changes,
        }
    })
    .collect();
    DeploymentRevisionHistory {
        paused: deployment.data["spec"]["paused"].as_bool() == Some(true),
        revisions,
    }
}

fn containers(template: &PodTemplateSpec) -> impl Iterator<Item = &Container> {
    template.spec.iter().flat_map(|spec| {
        spec.init_containers
            .iter()
            .flatten()
            .chain(spec.containers.iter())
    })
}

/// Describes an environment variable; references name their source, not its value.
fn env_value(env: &EnvVar) -> String {
    if let Some(value) = &env.value {
        return value.clone();
    }
    match &env.value_from {
        None => String::new(),
        Some(EnvVarSource {
            secret_key_ref: Some(key),
            ..
        }) => format!("from Secret {}/{}", key.name, key.key),
        Some(EnvVarSource {
            config_map_key_ref: Some(key),
            ..
        }) => format!("from ConfigMap {}/{}", key.name, key.key),
        Some(EnvVarSource {
            field_ref: Some(field),
            ..
        }) => format!("from field {}", field.field_path),
        Some(EnvVarSource {
            resource_field_ref: Some(resource),
            ..
        }) => format!("from resource {}", resource.resource),
        Some(_) => "from an unknown source".to_owned(),
    }
}

fn container_values(container: &Container) -> BTreeMap<String, String> {
    let mut values = BTreeMap::new();
    if let Some(image) = &container.image {
        values.insert("image".to_owned(), image.clone());
    }
    for env in container.env.iter().flatten() {
        values.insert(format!("env {}", env.name), env_value(env));
    }
    let resources = container.resources.as_ref();
    for (kind, quantities) in [
        ("requests", resources.and_then(|r| r.requests.as_ref())),
        ("limits", resources.and_then(|r| r.limits.as_ref())),
    ] {
        for (resource, quantity) in quantities.into_iter().flatten() {
            values.insert(format!("{kind}.{resource}"), quantity.0.clone());
        }
    }
    values
}

/// Images, environment and resources that differ per container, from the
/// `current` template to the `target` one.
pub(crate) fn template_changes(
    current: &PodTemplateSpec,
    target: &PodTemplateSpec,
) -> Vec<TemplateChange> {
    let by_name = |template| {
        containers(template)
            .map(|container| (container.name.as_str(), container))
            .collect::<BTreeMap<_, _>>()
    };
    let current_containers = by_name(current);
    let target_containers = by_name(target);
    let names = current_containers
        .keys()
        .chain(target_containers.keys())
        .collect::<BTreeSet<_>>();

    let mut changes = Vec::new();
    for name in names {
        match (current_containers.get(name), target_containers.get(name)) {
            (Some(current), Some(target)) => {
                let current_values = container_values(current);
                let target_values = container_values(target);
                let fields = current_values
                    .keys()
                    .chain(target_values.keys())
                    .collect::<BTreeSet<_>>();
                changes.extend(fields.into_iter().filter_map(|field| {
                    let (current, target) = (current_values.get(field), target_values.get(field));
                    (current != target).then(|| TemplateChange {
                        container: (*name).to_owned(),
                        field: field.clone(),
                        current: current.cloned(),
                        target: target.cloned(),
                    })
                }));
            }
            (current, target) => changes.push(TemplateChange {
                container: (*name).to_owned(),
                field: "container".to_owned(),
                current: current.map(|container| container.image.clone().unwrap_or_default()),
                target: target.map(|container| container.image.clone().unwrap_or_default()),
            }),
        }
    }
    changes
}

fn is_conflict(error: &kube::Error) -> bool {
    matches!(error, kube::Error::Api(status) if status.code == 409)
}

/// Rolls the Deployment back to `revision` like `kubectl rollout undo --to-revision`.
///
/// The Deployment is replaced with the resourceVersion it was read at, so a
/// concurrent change is never overwritten; a conflict re-reads and retries.
pub(crate) async fn rollback_deployment(
    client: kube::Client,
    namespace: &str,
    name: &str,
    revision: i64,
) -> Result<DeploymentRollbackOutcome> {
    let deployments: Api<DynamicObject> =
        Api::namespaced_with(client.clone(), namespace, &erased::<Deployment>());
    for attempt in 1..=ROLLBACK_ATTEMPTS {
        let (deployment, replica_sets) = load(client.clone(), namespace, name).await?;
        let replica_set = find_revision(&deployment, replica_sets, revision)?;
        let Some(rolled_back) = rolled_back_deployment(deployment, &replica_set)? else {
            return Ok(DeploymentRollbackOutcome::AlreadyAtRevision);
        };
        match deployments
            .replace(name, &PostParams::default(), &rolled_back)
            .await
        {
            Ok(_) => return Ok(DeploymentRollbackOutcome::RolledBack),
            Err(error) if is_conflict(&error) && attempt < ROLLBACK_ATTEMPTS => continue,
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("Rolling back Deployment {name} to revision {revision}")
                });
            }
        }
    }
    unreachable!("the last attempt returns")
}

/// The Deployment with the ReplicaSet's template and annotations, or `None`
/// when it already runs that template.
fn rolled_back_deployment(
    mut deployment: DynamicObject,
    replica_set: &DynamicObject,
) -> Result<Option<DynamicObject>> {
    if deployment.data["spec"]["paused"].as_bool() == Some(true) {
        bail!("A paused Deployment cannot be rolled back. Resume its rollout first.");
    }
    let template = template_value(replica_set);
    if template == template_value(&deployment) {
        return Ok(None);
    }
    deployment.data["spec"]["template"] = template;

    let annotations = deployment.annotations_mut();
    annotations.retain(|key, _| ANNOTATIONS_KEPT_FROM_DEPLOYMENT.contains(&key.as_str()));
    annotations.extend(
        replica_set
            .annotations()
            .iter()
            .filter(|(key, _)| !ANNOTATIONS_KEPT_FROM_DEPLOYMENT.contains(&key.as_str()))
            .map(|(key, value)| (key.clone(), value.clone())),
    );
    Ok(Some(deployment))
}

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::api::apps::v1::{DeploymentSpec, ReplicaSetSpec};
    use k8s_openapi::api::core::v1::{
        ConfigMapKeySelector, ObjectFieldSelector, PodSpec, ResourceFieldSelector,
        ResourceRequirements, SecretKeySelector,
    };
    use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::{OwnerReference, Time};
    use serde_json::json;

    const DEPLOYMENT_UID: &str = "deployment-uid";

    fn container(name: &str, image: &str, env: &[(&str, &str)], cpu: Option<&str>) -> Container {
        Container {
            name: name.to_owned(),
            image: Some(image.to_owned()),
            env: (!env.is_empty()).then(|| {
                env.iter()
                    .map(|(name, value)| EnvVar {
                        name: (*name).to_owned(),
                        value: Some((*value).to_owned()),
                        ..Default::default()
                    })
                    .collect()
            }),
            resources: cpu.map(|cpu| ResourceRequirements {
                requests: Some(BTreeMap::from([(
                    "cpu".to_owned(),
                    Quantity(cpu.to_owned()),
                )])),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    fn template(containers: Vec<Container>, hash: Option<&str>) -> PodTemplateSpec {
        let mut labels = BTreeMap::from([("app".to_owned(), "web".to_owned())]);
        if let Some(hash) = hash {
            labels.insert(POD_TEMPLATE_HASH_LABEL.to_owned(), hash.to_owned());
        }
        PodTemplateSpec {
            metadata: Some(ObjectMeta {
                labels: Some(labels),
                ..Default::default()
            }),
            spec: Some(PodSpec {
                containers,
                ..Default::default()
            }),
        }
    }

    fn dynamic(object: impl serde::Serialize) -> DynamicObject {
        serde_json::from_value(serde_json::to_value(object).expect("serializable"))
            .expect("valid object")
    }

    fn replica_set(
        name: &str,
        revision: Option<i64>,
        owner: &str,
        created_seconds: i64,
        template: PodTemplateSpec,
    ) -> DynamicObject {
        dynamic(ReplicaSet {
            metadata: ObjectMeta {
                name: Some(name.to_owned()),
                creation_timestamp: Some(Time(
                    k8s_openapi::jiff::Timestamp::from_second(created_seconds)
                        .expect("valid timestamp"),
                )),
                annotations: revision.map(|revision| {
                    BTreeMap::from([(REVISION_ANNOTATION.to_owned(), revision.to_string())])
                }),
                owner_references: Some(vec![OwnerReference {
                    controller: Some(true),
                    uid: owner.to_owned(),
                    ..Default::default()
                }]),
                ..Default::default()
            },
            spec: Some(ReplicaSetSpec {
                template: Some(template),
                ..Default::default()
            }),
            ..Default::default()
        })
    }

    fn deployment(
        current_revision: Option<i64>,
        paused: bool,
        template: PodTemplateSpec,
    ) -> DynamicObject {
        dynamic(Deployment {
            metadata: ObjectMeta {
                name: Some("web".to_owned()),
                uid: Some(DEPLOYMENT_UID.to_owned()),
                annotations: current_revision.map(|revision| {
                    BTreeMap::from([(REVISION_ANNOTATION.to_owned(), revision.to_string())])
                }),
                ..Default::default()
            },
            spec: Some(DeploymentSpec {
                paused: Some(paused),
                template,
                ..Default::default()
            }),
            ..Default::default()
        })
    }

    fn change(
        container: &str,
        field: &str,
        current: Option<&str>,
        target: Option<&str>,
    ) -> TemplateChange {
        TemplateChange {
            container: container.to_owned(),
            field: field.to_owned(),
            current: current.map(str::to_owned),
            target: target.map(str::to_owned),
        }
    }

    #[test]
    fn template_changes_summarise_images_environment_resources_and_containers() {
        let current = template(
            vec![
                container(
                    "web",
                    "nginx:1.27",
                    &[("MODE", "fast"), ("NEW", "1")],
                    Some("200m"),
                ),
                container("sidecar", "envoy:1", &[], None),
            ],
            None,
        );
        let mut target = template(
            vec![container(
                "web",
                "nginx:1.25",
                &[("MODE", "safe"), ("OLD", "1")],
                Some("100m"),
            )],
            None,
        );
        target.spec.as_mut().unwrap().init_containers =
            Some(vec![container("migrate", "migrator:3", &[], None)]);

        assert_eq!(
            template_changes(&current, &target),
            vec![
                change("migrate", "container", None, Some("migrator:3")),
                change("sidecar", "container", Some("envoy:1"), None),
                change("web", "env MODE", Some("fast"), Some("safe")),
                change("web", "env NEW", Some("1"), None),
                change("web", "env OLD", None, Some("1")),
                change("web", "image", Some("nginx:1.27"), Some("nginx:1.25")),
                change("web", "requests.cpu", Some("200m"), Some("100m")),
            ]
        );
        assert!(template_changes(&current, &current).is_empty());
    }

    #[test]
    fn environment_references_name_their_source_without_values() {
        let reference = |value_from: EnvVarSource| EnvVar {
            name: "VALUE".to_owned(),
            value_from: Some(value_from),
            ..Default::default()
        };
        assert_eq!(
            env_value(&reference(EnvVarSource {
                secret_key_ref: Some(SecretKeySelector {
                    name: "api".to_owned(),
                    key: "token".to_owned(),
                    ..Default::default()
                }),
                ..Default::default()
            })),
            "from Secret api/token"
        );
        assert_eq!(
            env_value(&reference(EnvVarSource {
                config_map_key_ref: Some(ConfigMapKeySelector {
                    name: "settings".to_owned(),
                    key: "mode".to_owned(),
                    ..Default::default()
                }),
                ..Default::default()
            })),
            "from ConfigMap settings/mode"
        );
        assert_eq!(
            env_value(&reference(EnvVarSource {
                field_ref: Some(ObjectFieldSelector {
                    field_path: "metadata.name".to_owned(),
                    ..Default::default()
                }),
                ..Default::default()
            })),
            "from field metadata.name"
        );
        assert_eq!(
            env_value(&reference(EnvVarSource {
                resource_field_ref: Some(ResourceFieldSelector {
                    resource: "limits.cpu".to_owned(),
                    ..Default::default()
                }),
                ..Default::default()
            })),
            "from resource limits.cpu"
        );
        assert_eq!(
            env_value(&EnvVar {
                name: "EMPTY".to_owned(),
                ..Default::default()
            }),
            ""
        );
    }

    #[test]
    fn history_lists_owned_revisions_newest_first_and_marks_the_current_one() {
        let v1 = template(vec![container("web", "nginx:1.25", &[], None)], Some("aaa"));
        let v2 = template(vec![container("web", "nginx:1.27", &[], None)], Some("bbb"));
        let current = template(vec![container("web", "nginx:1.27", &[], None)], None);

        let history = revision_history(
            &deployment(Some(2), false, current.clone()),
            vec![
                replica_set("web-aaa", Some(1), DEPLOYMENT_UID, 10, v1.clone()),
                replica_set("web-bbb", Some(2), DEPLOYMENT_UID, 20, v2),
                replica_set("other-ccc", Some(5), "another-deployment", 30, v1.clone()),
                replica_set("web-unnumbered", None, DEPLOYMENT_UID, 40, v1.clone()),
                // An adopted ReplicaSet claiming an existing revision: the newest wins.
                replica_set("web-adopted", Some(1), DEPLOYMENT_UID, 5, v1),
            ],
        );

        assert!(!history.paused);
        assert_eq!(
            history
                .revisions
                .iter()
                .map(|revision| (
                    revision.revision,
                    revision.replica_set.as_str(),
                    revision.is_current
                ))
                .collect::<Vec<_>>(),
            vec![(2, "web-bbb", true), (1, "web-aaa", false)]
        );
        let previous = &history.revisions[1];
        assert_eq!(previous.images, vec!["nginx:1.25".to_owned()]);
        assert_eq!(
            previous.changes,
            vec![change(
                "web",
                "image",
                Some("nginx:1.27"),
                Some("nginx:1.25")
            )]
        );
        assert!(!previous.other_changes);
        assert!(history.revisions[0].changes.is_empty());
        assert!(!history.revisions[0].other_changes);
        assert!(previous.created.is_some());
    }

    #[test]
    fn without_a_revision_annotation_the_current_revision_is_found_by_template() {
        let v1 = template(vec![container("web", "nginx:1.25", &[], None)], Some("aaa"));
        let v2 = template(vec![container("web", "nginx:1.27", &[], None)], Some("bbb"));
        let current = template(vec![container("web", "nginx:1.27", &[], None)], None);
        let history = revision_history(
            &deployment(None, true, current),
            vec![
                replica_set("web-aaa", Some(1), DEPLOYMENT_UID, 10, v1),
                replica_set("web-bbb", Some(2), DEPLOYMENT_UID, 20, v2),
            ],
        );
        assert!(history.paused);
        assert_eq!(
            history
                .revisions
                .iter()
                .map(|revision| (revision.revision, revision.is_current))
                .collect::<Vec<_>>(),
            vec![(2, true), (1, false)]
        );
    }

    #[test]
    fn differences_outside_the_summary_are_flagged() {
        let current = template(vec![container("web", "nginx:1.27", &[], None)], None);
        let mut command_changed = current.clone();
        command_changed.spec.as_mut().unwrap().containers[0].command =
            Some(vec!["serve".to_owned()]);
        let mut optional_secret = current.clone();
        optional_secret.spec.as_mut().unwrap().containers[0].env = Some(vec![EnvVar {
            name: "TOKEN".to_owned(),
            value_from: Some(EnvVarSource {
                secret_key_ref: Some(SecretKeySelector {
                    name: "api".to_owned(),
                    key: "token".to_owned(),
                    optional: Some(true),
                }),
                ..Default::default()
            }),
            ..Default::default()
        }]);
        let mut current_with_secret = optional_secret.clone();
        current_with_secret.spec.as_mut().unwrap().containers[0]
            .env
            .as_mut()
            .unwrap()[0]
            .value_from
            .as_mut()
            .unwrap()
            .secret_key_ref
            .as_mut()
            .unwrap()
            .optional = None;

        let history = revision_history(
            &deployment(Some(3), false, current),
            vec![replica_set(
                "web-aaa",
                Some(1),
                DEPLOYMENT_UID,
                10,
                command_changed,
            )],
        );
        assert!(history.revisions[0].changes.is_empty());
        assert!(history.revisions[0].other_changes);

        // Only `optional` differs, which the summary does not render.
        let history = revision_history(
            &deployment(Some(3), false, current_with_secret),
            vec![replica_set(
                "web-aaa",
                Some(1),
                DEPLOYMENT_UID,
                10,
                optional_secret,
            )],
        );
        assert!(history.revisions[0].changes.is_empty());
        assert!(
            history.revisions[0].other_changes,
            "a template that differs must never be reported as identical"
        );
    }

    #[test]
    fn rollback_targets_only_an_owned_existing_revision() {
        let v1 = template(vec![container("web", "nginx:1.25", &[], None)], Some("aaa"));
        let deployment = deployment(Some(2), false, template(vec![], None));
        let replica_sets = || {
            vec![
                replica_set("web-old", Some(1), DEPLOYMENT_UID, 10, v1.clone()),
                replica_set("web-new", Some(1), DEPLOYMENT_UID, 20, v1.clone()),
                replica_set("other", Some(4), "another-deployment", 30, v1.clone()),
            ]
        };
        assert_eq!(
            find_revision(&deployment, replica_sets(), 1)
                .unwrap()
                .name_any(),
            "web-new"
        );
        let error = find_revision(&deployment, replica_sets(), 4).unwrap_err();
        assert_eq!(
            error.to_string(),
            "Revision 4 of Deployment web no longer exists"
        );
    }

    #[test]
    fn rollback_copies_the_template_and_annotations_like_kubectl() {
        let deployment = dynamic(json!({
            "apiVersion": "apps/v1",
            "kind": "Deployment",
            "metadata": {
                "name": "web",
                "resourceVersion": "42",
                "annotations": {
                    "deployment.kubernetes.io/revision": "3",
                    "kubectl.kubernetes.io/last-applied-configuration": "{}",
                    "team": "payments"
                }
            },
            "spec": {
                "replicas": 2,
                "template": {
                    "metadata": { "labels": { "app": "web" } },
                    "spec": { "containers": [{ "name": "web", "image": "nginx:1.27" }] }
                }
            }
        }));
        let replica_set = dynamic(json!({
            "apiVersion": "apps/v1",
            "kind": "ReplicaSet",
            "metadata": {
                "name": "web-aaa",
                "annotations": {
                    "deployment.kubernetes.io/revision": "1",
                    "deployment.kubernetes.io/desired-replicas": "2",
                    "release-note": "first"
                }
            },
            "spec": {
                "template": {
                    "metadata": { "labels": { "app": "web", "pod-template-hash": "aaa" } },
                    "spec": {
                        "containers": [{ "name": "web", "image": "nginx:1.25" }],
                        "futureField": { "kept": true }
                    }
                }
            }
        }));

        let rolled_back = rolled_back_deployment(deployment, &replica_set)
            .unwrap()
            .expect("templates differ");

        assert_eq!(
            rolled_back.data["spec"]["template"],
            json!({
                "metadata": { "labels": { "app": "web" } },
                "spec": {
                    "containers": [{ "name": "web", "image": "nginx:1.25" }],
                    "futureField": { "kept": true }
                }
            }),
            "the hash label is dropped and fields unknown to this client survive"
        );
        assert_eq!(rolled_back.data["spec"]["replicas"], json!(2));
        assert_eq!(
            rolled_back.metadata.resource_version.as_deref(),
            Some("42"),
            "the replace is conditional on the version that was read"
        );
        assert_eq!(
            rolled_back.metadata.annotations,
            Some(BTreeMap::from([
                (
                    "deployment.kubernetes.io/revision".to_owned(),
                    "3".to_owned()
                ),
                (
                    "kubectl.kubernetes.io/last-applied-configuration".to_owned(),
                    "{}".to_owned()
                ),
                ("release-note".to_owned(), "first".to_owned()),
            ])),
            "rollout bookkeeping stays from the Deployment, the rest comes from the revision"
        );
    }

    #[test]
    fn rollback_is_skipped_when_already_at_the_revision_and_refused_while_paused() {
        let template = json!({
            "metadata": { "labels": { "app": "web" } },
            "spec": { "containers": [{ "name": "web", "image": "nginx:1.25" }] }
        });
        let mut replica_set_template = template.clone();
        replica_set_template["metadata"]["labels"]["pod-template-hash"] = json!("aaa");
        let replica_set = dynamic(json!({
            "metadata": { "name": "web-aaa" },
            "spec": { "template": replica_set_template }
        }));
        let deployment = |paused: bool| {
            dynamic(json!({
                "metadata": { "name": "web" },
                "spec": { "paused": paused, "template": template }
            }))
        };

        assert!(
            rolled_back_deployment(deployment(false), &replica_set)
                .unwrap()
                .is_none()
        );
        let error = rolled_back_deployment(deployment(true), &replica_set).unwrap_err();
        assert!(error.to_string().contains("paused"), "{error}");
    }
}
