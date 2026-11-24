use super::*;

pub(crate) fn resource_event_from_kubernetes(event: KubernetesEvent) -> ResourceEvent {
    let last_timestamp = if let Some(timestamp) = event.event_time.as_ref() {
        OffsetDateTime::parse(
            &timestamp.0.to_string(),
            &time::format_description::well_known::Rfc3339,
        )
        .ok()
    } else {
        event.last_timestamp.as_ref().and_then(|timestamp| {
            OffsetDateTime::parse(
                &timestamp.0.to_string(),
                &time::format_description::well_known::Rfc3339,
            )
            .ok()
        })
    };
    ResourceEvent {
        uid: get_resource_uid(&event),
        type_: event.type_.unwrap_or_else(|| "Normal".to_owned()),
        reason: event.reason.unwrap_or_else(|| "Unknown".to_owned()),
        message: event.message.unwrap_or_default(),
        source: event.source.and_then(|source| source.component),
        count: event.count.unwrap_or(1),
        last_timestamp,
    }
}

pub(crate) fn supports_scale_subresource(
    resources: &[k8s_openapi::apimachinery::pkg::apis::meta::v1::APIResource],
    resource_name: &str,
) -> bool {
    let scale_name = format!("{resource_name}/scale");
    resources
        .iter()
        .any(|resource| resource.name == scale_name && allows_scaling(&resource.verbs))
}

/// Whether a scale subresource's verbs allow reading and changing the replica count.
pub(crate) fn allows_scaling(verbs: &[String]) -> bool {
    verbs.iter().any(|verb| verb == "get") && verbs.iter().any(|verb| verb == "patch")
}

/// Fetch the desired replica count through a dynamically discovered Scale subresource.
pub(crate) async fn get_resource_scale(
    cluster_key: i32,
    request_id: u64,
    client: kube::Client,
    api_resource: ApiResource,
    namespace: Option<String>,
    resource_name: String,
) -> Result<ResourceScaleFetched> {
    let api = dynamic_api::create(&client, &api_resource, namespace.as_deref())?;
    let scale = api.get_scale(&resource_name).await?;
    let replicas = scale
        .spec
        .context("Scale endpoint returned no desired replica count")?
        .replicas
        .context("Scale endpoint returned no desired replica count")?;

    Ok(ResourceScaleFetched {
        cluster_key,
        request_id,
        api_resource,
        namespace,
        resource_name,
        replicas,
    })
}

/// Update the desired replica count through a dynamically discovered Scale subresource.
pub(crate) async fn update_resource_scale(
    cluster_key: i32,
    client: kube::Client,
    api_resource: ApiResource,
    namespace: Option<String>,
    resource_name: String,
    replicas: i32,
) -> Result<ResourceScaleUpdated> {
    let api = dynamic_api::create(&client, &api_resource, namespace.as_deref())?;
    let patch: serde_yaml::Value =
        serde_yaml::from_str(&format!("spec:\n  replicas: {replicas}\n"))?;
    api.patch_scale(
        &resource_name,
        &kube::api::PatchParams::default(),
        &kube::api::Patch::Merge(&patch),
    )
    .await?;

    Ok(ResourceScaleUpdated {
        cluster_key,
        api_resource,
        namespace,
        resource_name,
        replicas,
    })
}

/// Fetch a resource's full YAML representation
pub(crate) async fn get_resource_yaml(
    editor_id: u64,
    cluster_key: i32,
    client: kube::Client,
    api_resource: ApiResource,
    namespace: Option<String>,
    resource_name: String,
) -> Result<ResourceYamlFetched> {
    info!(
        "Getting YAML for {}/{} {} in {}",
        api_resource.group,
        api_resource.name,
        resource_name,
        namespace.as_deref().unwrap_or("cluster-wide scope")
    );

    let started = std::time::Instant::now();
    let api = dynamic_api::create(&client, &api_resource, namespace.as_deref())?;
    let mut obj = api.get(&resource_name).await?;
    info!(
        "Got YAML for {}/{} {resource_name} after {:?}",
        api_resource.group,
        api_resource.name,
        started.elapsed()
    );

    let (yaml, resource_version, resource_uid) = resource_yaml::editor_yaml(&mut obj)?;

    Ok(ResourceYamlFetched {
        editor_id,
        cluster_key,
        api_resource,
        namespace,
        resource_name,
        yaml,
        resource_version,
        resource_uid,
    })
}

/// Fetch the OpenAPI v3 group-version document and return the schema for one resource,
/// built in or custom. Schemas are only loaded here, when a YAML editor needs one.
pub(crate) async fn get_resource_schema(
    editor_id: u64,
    cluster_key: i32,
    client: kube::Client,
    api_resource: ApiResource,
) -> Result<ResourceSchemaLoaded> {
    let group_version = if api_resource.kube_group().is_empty() {
        format!("api/{}", api_resource.version)
    } else {
        format!("apis/{}/{}", api_resource.group, api_resource.version)
    };
    let index: k8s_openapi::serde_json::Value = client
        .request(Request::builder().uri("/openapi/v3").body(Vec::new())?)
        .await?;
    let path = index
        .get("paths")
        .and_then(|paths| {
            paths
                .get(&group_version)
                .or_else(|| paths.get(format!("/{group_version}")))
        })
        .and_then(|entry| entry.get("serverRelativeURL"))
        .and_then(k8s_openapi::serde_json::Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("No OpenAPI v3 schema is available for {group_version}"))?;
    let document: k8s_openapi::serde_json::Value = client
        .request(Request::builder().uri(path).body(Vec::new())?)
        .await?;
    let schema = ResourceSchema::from_openapi_document(document, &api_resource)
        .ok_or_else(|| anyhow::anyhow!("No OpenAPI schema matches {}", api_resource.kind))?;
    Ok(ResourceSchemaLoaded {
        editor_id,
        cluster_key,
        api_resource,
        schema,
    })
}

pub(crate) struct ResourceYamlValidationRequest {
    pub editor_id: u64,
    pub revision: u64,
    pub cluster_key: i32,
    pub client: kube::Client,
    pub api_resource: ApiResource,
    pub namespace: Option<String>,
    pub resource_name: String,
    pub original_yaml: String,
    pub yaml: String,
    pub resource_version: String,
    pub resource_uid: String,
}

/// Validate the same full-resource replacement request used by Save without persisting a change.
pub(crate) async fn validate_resource_yaml(
    request: ResourceYamlValidationRequest,
) -> Result<Result<ResourceYamlValidated, ResourceYamlValidationFailed>> {
    let ResourceYamlValidationRequest {
        editor_id,
        revision,
        cluster_key,
        client,
        api_resource,
        namespace,
        resource_name,
        original_yaml,
        yaml,
        resource_version,
        resource_uid,
    } = request;

    let api = dynamic_api::create(&client, &api_resource, namespace.as_deref())?;
    let document = resource_yaml::EditorDocument {
        original_yaml: &original_yaml,
        edited_yaml: &yaml,
        resource_version: &resource_version,
        resource_uid: &resource_uid,
    };
    let replacement =
        resource_yaml::send_editor_replacement(&api, &resource_name, &document, |object| {
            let request =
                strict_dry_run_replace_request(api.resource_url(), &resource_name, &object);
            let client = client.clone();
            async move { Ok(client.request::<DynamicObject>(request?).await?) }
        })
        .await?;
    let error = match replacement {
        resource_yaml::EditorReplacement::Sent(_) => {
            return Ok(Ok(ResourceYamlValidated {
                editor_id,
                revision,
                cluster_key,
                api_resource,
                namespace,
                resource_name,
            }));
        }
        resource_yaml::EditorReplacement::Conflict { .. } => resource_version_conflict_error(),
        resource_yaml::EditorReplacement::Rejected(status) => resource_api_error(&status),
    };
    Ok(Err(ResourceYamlValidationFailed {
        editor_id,
        revision,
        cluster_key,
        api_resource,
        namespace,
        resource_name,
        error,
    }))
}

fn strict_dry_run_replace_request(
    resource_url: &str,
    resource_name: &str,
    object: &DynamicObject,
) -> Result<Request<Vec<u8>>> {
    Request::put(format!(
        "{resource_url}/{resource_name}?dryRun=All&fieldValidation=Strict"
    ))
    .header(http::header::CONTENT_TYPE, "application/json")
    .body(k8s_openapi::serde_json::to_vec(object)?)
    .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_dry_run_replacement_uses_the_resource_update_endpoint() {
        let object: DynamicObject =
            k8s_openapi::serde_json::from_value(k8s_openapi::serde_json::json!({
                "apiVersion": "apps/v1",
                "kind": "Deployment",
                "metadata": { "name": "api" }
            }))
            .unwrap();

        let request = strict_dry_run_replace_request(
            "/apis/apps/v1/namespaces/default/deployments",
            "api",
            &object,
        )
        .unwrap();

        assert_eq!(request.method(), http::Method::PUT);
        assert_eq!(
            request.uri(),
            "/apis/apps/v1/namespaces/default/deployments/api?dryRun=All&fieldValidation=Strict"
        );
        assert_eq!(
            request.headers().get(http::header::CONTENT_TYPE).unwrap(),
            "application/json"
        );
    }
}
