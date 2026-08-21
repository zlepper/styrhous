use super::*;

/// Delete a resource
pub(crate) async fn delete_resource(
    cluster_key: i32,
    client: kube::Client,
    api_resource: ApiResource,
    namespace: Option<String>,
    resource_name: String,
    resource_uid: Option<String>,
    bulk_delete_id: Option<u64>,
) -> Result<ResourceDeleteCompleted> {
    info!(
        "Deleting {}/{} {} in {}",
        api_resource.group,
        api_resource.name,
        resource_name,
        namespace.as_deref().unwrap_or("cluster-wide scope")
    );

    let api = dynamic_api::create(&client, &api_resource, namespace.as_deref()).await?;
    let delete_params = resource_uid.map(|uid| {
        DeleteParams::default().preconditions(Preconditions {
            uid: Some(uid),
            resource_version: None,
        })
    });
    api.delete(&resource_name, &delete_params.unwrap_or_default())
        .await?;

    Ok(ResourceDeleteCompleted {
        cluster_key,
        api_resource,
        namespace,
        resource_name,
        bulk_delete_id,
    })
}

/// Remove finalizers from a resource Kubernetes is already deleting.
pub(crate) async fn force_delete_resource(
    cluster_key: i32,
    client: kube::Client,
    api_resource: ApiResource,
    namespace: Option<String>,
    resource_name: String,
    resource_uid: String,
) -> Result<ResourceForceDeleteCompleted> {
    let api = dynamic_api::create(&client, &api_resource, namespace.as_deref()).await?;
    let resource = api.get(&resource_name).await?;
    let metadata = resource.meta();
    if metadata.uid.as_deref() != Some(&resource_uid) {
        anyhow::bail!(
            "Resource was replaced while awaiting confirmation; finalizers were not removed"
        );
    }
    if metadata.deletion_timestamp.is_none() {
        anyhow::bail!("Resource is no longer deleting; finalizers were not removed");
    }
    if metadata.finalizers.as_ref().is_none_or(Vec::is_empty) {
        anyhow::bail!("Resource no longer has finalizers; nothing was removed");
    }
    let resource_version = metadata
        .resource_version
        .as_deref()
        .context("Deleting resource did not include a resource version")?;
    info!(
        "Removing finalizers from {}/{} {} in {}",
        api_resource.group,
        api_resource.name,
        resource_name,
        namespace.as_deref().unwrap_or("cluster-wide scope")
    );
    let patch = k8s_openapi::serde_json::json!({
        "metadata": { "resourceVersion": resource_version, "finalizers": [] }
    });
    api.patch(
        &resource_name,
        &kube::api::PatchParams::default(),
        &kube::api::Patch::Merge(&patch),
    )
    .await?;
    Ok(ResourceForceDeleteCompleted {
        cluster_key,
        resource_name,
    })
}

/// Trigger a Deployment rollout the same way `kubectl rollout restart` does.
pub(crate) async fn restart_deployment(
    client: kube::Client,
    namespace: String,
    resource_name: String,
) -> Result<DeploymentRestartCompleted> {
    let restarted_at = OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .context("Formatting Deployment restart timestamp")?;
    info!(
        "Restarting rollout for Deployment {} in {}",
        resource_name, namespace
    );

    let api: Api<Deployment> = Api::namespaced(client, &namespace);
    let patch: serde_yaml::Value = serde_yaml::from_str(&format!(
        "spec:\n  template:\n    metadata:\n      annotations:\n        kubectl.kubernetes.io/restartedAt: \"{restarted_at}\"\n"
    ))?;
    api.patch(
        &resource_name,
        &kube::api::PatchParams::default(),
        &kube::api::Patch::Merge(&patch),
    )
    .await?;

    Ok(DeploymentRestartCompleted {
        namespace,
        resource_name,
    })
}

/// Create a one-off Job from a CronJob's current job template.
pub(crate) async fn run_cron_job(
    client: kube::Client,
    namespace: String,
    resource_name: String,
) -> Result<CronJobRunCompleted> {
    let cron_jobs: Api<CronJob> = Api::namespaced(client.clone(), &namespace);
    let cron_job = cron_jobs
        .get(&resource_name)
        .await
        .with_context(|| format!("Fetching CronJob {resource_name} in {namespace}"))?;
    let job = job_from_cron_job(&cron_job)?;

    info!("Creating one-off Job from CronJob {resource_name} in {namespace}");
    let jobs: Api<Job> = Api::namespaced(client, &namespace);
    let created = jobs
        .create(&Default::default(), &job)
        .await
        .with_context(|| format!("Creating Job from CronJob {resource_name} in {namespace}"))?;
    let job_name = created
        .metadata
        .name
        .context("Kubernetes created a Job without a name")?;

    Ok(CronJobRunCompleted {
        namespace,
        cron_job_name: resource_name,
        job_name,
    })
}

pub(crate) fn job_from_cron_job(cron_job: &CronJob) -> Result<Job> {
    let cron_job_name = cron_job
        .metadata
        .name
        .as_deref()
        .context("CronJob has no name")?;
    let cron_job_spec = cron_job.spec.as_ref().context("CronJob has no spec")?;
    let template = &cron_job_spec.job_template;
    let spec = template
        .spec
        .clone()
        .context("CronJob job template has no spec")?;
    let template_metadata = template.metadata.as_ref();
    let mut annotations = template_metadata
        .and_then(|metadata| metadata.annotations.clone())
        .unwrap_or_default();
    annotations.insert("cronjob.kubernetes.io/instantiate".into(), "manual".into());
    let owner_uid = cron_job
        .metadata
        .uid
        .clone()
        .context("CronJob has no UID")?;

    Ok(Job {
        metadata: ObjectMeta {
            generate_name: Some(format!("{cron_job_name}-manual-")),
            annotations: Some(annotations),
            labels: template_metadata.and_then(|metadata| metadata.labels.clone()),
            owner_references: Some(vec![OwnerReference {
                api_version: "batch/v1".into(),
                kind: "CronJob".into(),
                name: cron_job_name.into(),
                uid: owner_uid,
                controller: Some(true),
                block_owner_deletion: None,
            }]),
            ..Default::default()
        },
        spec: Some(spec),
        status: None,
    })
}

/// Apply (replace) a resource from YAML
pub(crate) async fn apply_resource_yaml(
    editor_id: u64,
    cluster_key: i32,
    client: kube::Client,
    api_resource: ApiResource,
    namespace: Option<String>,
    resource_name: String,
    yaml: String,
) -> Result<Result<ResourceApplyCompleted, ResourceApplyFailed>> {
    info!(
        "Applying YAML for {}/{} {} in {}",
        api_resource.group,
        api_resource.name,
        resource_name,
        namespace.as_deref().unwrap_or("cluster-wide scope")
    );

    let mut obj: DynamicObject = serde_yaml::from_str(&yaml)?;

    resource_yaml::strip_server_managed_metadata(&mut obj);

    let api = dynamic_api::create(&client, &api_resource, namespace.as_deref()).await?;

    // Use server-side apply with force to take ownership of fields
    let patch_params = kube::api::PatchParams::apply("styrhous").force();
    match api
        .patch(
            &resource_name,
            &patch_params,
            &kube::api::Patch::Apply(&obj),
        )
        .await
    {
        Ok(_) => Ok(Ok(ResourceApplyCompleted {
            editor_id,
            cluster_key,
            api_resource,
            namespace,
            resource_name,
        })),
        Err(kube::Error::Api(status)) => Ok(Err(ResourceApplyFailed {
            editor_id,
            cluster_key,
            api_resource,
            namespace,
            resource_name,
            error: resource_api_error(&status),
        })),
        Err(error) => Err(error.into()),
    }
}

pub(crate) struct ResourceDataUpdateRequest<'a> {
    pub cluster_key: i32,
    pub history_entry_id: u64,
    pub request_id: u64,
    pub client: kube::Client,
    pub api_resource: ApiResource,
    pub namespace: String,
    pub resource_name: String,
    pub expected_values: &'a BTreeMap<String, String>,
    pub updated_values: &'a BTreeMap<String, String>,
    pub expected_resource_version: &'a str,
}

/// Replace selected existing data values while preserving every other field. The
/// fetched object's resourceVersion makes a concurrent update fail rather than
/// silently overwriting it.
pub(crate) async fn update_resource_data(
    request: ResourceDataUpdateRequest<'_>,
) -> Result<ResourceDataUpdateCompleted> {
    let ResourceDataUpdateRequest {
        cluster_key,
        history_entry_id,
        request_id,
        client,
        api_resource,
        namespace,
        resource_name,
        expected_values,
        updated_values,
        expected_resource_version,
    } = request;
    resource_data::validate_update_request(
        expected_values,
        updated_values,
        expected_resource_version,
    )?;

    if resource_handlers::matches_namespaced_api_resource::<ConfigMap>(&api_resource) {
        let api: Api<ConfigMap> = Api::namespaced(client, &namespace);
        let mut config_map = api.get(&resource_name).await?;
        resource_data::validate_resource_version(
            config_map.metadata.resource_version.as_deref(),
            expected_resource_version,
            "ConfigMap",
        )?;
        let data = config_map
            .data
            .as_mut()
            .context("ConfigMap has no text data to update")?;
        for (key, expected) in expected_values {
            if data.get(key) != Some(expected) {
                bail!("ConfigMap data key '{key}' changed or was removed on the cluster");
            }
        }
        for (key, value) in updated_values {
            *data
                .get_mut(key)
                .expect("expected ConfigMap key was verified above") = value.clone();
        }
        api.replace(&resource_name, &Default::default(), &config_map)
            .await?;
    } else if resource_handlers::matches_namespaced_api_resource::<Secret>(&api_resource) {
        let api: Api<Secret> = Api::namespaced(client, &namespace);
        let mut secret = api.get(&resource_name).await?;
        resource_data::validate_resource_version(
            secret.metadata.resource_version.as_deref(),
            expected_resource_version,
            "Secret",
        )?;
        let data = secret
            .data
            .as_mut()
            .context("Secret has no data to update")?;
        for (key, expected) in expected_values {
            let Some(current) = data.get(key) else {
                bail!("Secret data key '{key}' was removed on the cluster");
            };
            if std::str::from_utf8(&current.0).ok() != Some(expected.as_str()) {
                bail!("Secret data key '{key}' changed on the cluster");
            }
        }
        for (key, value) in updated_values {
            *data
                .get_mut(key)
                .expect("expected Secret key was verified above") =
                k8s_openapi::ByteString(value.as_bytes().to_vec());
        }
        api.replace(&resource_name, &Default::default(), &secret)
            .await?;
    } else {
        bail!("Resource data updates are only supported for ConfigMaps and Secrets");
    }

    Ok(ResourceDataUpdateCompleted {
        cluster_key,
        history_entry_id,
        request_id,
    })
}
