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
    resources.iter().any(|resource| {
        resource.name == scale_name
            && resource.verbs.iter().any(|verb| verb == "get")
            && resource.verbs.iter().any(|verb| verb == "patch")
    })
}

/// Fetch the desired replica count through a dynamically discovered Scale subresource.
pub(crate) async fn get_resource_scale(
    cluster_key: i32,
    client: kube::Client,
    api_resource: ApiResource,
    namespace: Option<String>,
    resource_name: String,
) -> Result<ResourceScaleFetched> {
    let api = dynamic_api::create(&client, &api_resource, namespace.as_deref()).await?;
    let scale = api.get_scale(&resource_name).await?;
    let replicas = scale
        .spec
        .context("Scale endpoint returned no desired replica count")?
        .replicas
        .context("Scale endpoint returned no desired replica count")?;

    Ok(ResourceScaleFetched {
        cluster_key,
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
    let api = dynamic_api::create(&client, &api_resource, namespace.as_deref()).await?;
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
        resource_name,
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

    let api = dynamic_api::create(&client, &api_resource, namespace.as_deref()).await?;
    let mut obj = api.get(&resource_name).await?;

    resource_yaml::strip_server_managed_metadata(&mut obj);

    let yaml = serde_yaml::to_string(&obj)?;

    Ok(ResourceYamlFetched {
        editor_id,
        cluster_key,
        api_resource,
        namespace,
        resource_name,
        yaml,
    })
}

/// Fetch the OpenAPI v3 group-version document and return the schema for one built-in resource.
/// CRD schemas are sent with API discovery, so this path is only used as a lazy fallback.
pub(crate) async fn get_resource_schema(
    editor_id: u64,
    cluster_key: i32,
    client: kube::Client,
    api_resource: ApiResource,
) -> Result<ResourceSchemaLoaded> {
    let group_version = if api_resource.group == "core" {
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
    pub yaml: String,
}

/// Validate the same server-side apply request used by Save without persisting a change.
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
        yaml,
    } = request;
    let mut obj: DynamicObject = serde_yaml::from_str(&yaml)?;
    resource_yaml::strip_server_managed_metadata(&mut obj);

    let api = dynamic_api::create(&client, &api_resource, namespace.as_deref()).await?;
    let params = kube::api::PatchParams::apply("styrhous")
        .force()
        .validation(kube::api::ValidationDirective::Strict)
        .dry_run();
    match api
        .patch(&resource_name, &params, &kube::api::Patch::Apply(&obj))
        .await
    {
        Ok(_) => Ok(Ok(ResourceYamlValidated {
            editor_id,
            revision,
            cluster_key,
            api_resource,
            namespace,
            resource_name,
        })),
        Err(kube::Error::Api(status)) => Ok(Err(ResourceYamlValidationFailed {
            editor_id,
            revision,
            cluster_key,
            api_resource,
            namespace,
            resource_name,
            error: resource_api_error(&status),
        })),
        Err(error) => Err(error.into()),
    }
}
