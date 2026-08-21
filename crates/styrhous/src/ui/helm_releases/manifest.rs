use crate::api_resource::ApiResource;
use serde::Deserialize;

#[derive(Debug)]
pub(crate) struct ManifestResource {
    pub(crate) api_version: String,
    pub(crate) kind: String,
    pub(crate) name: String,
    pub(crate) namespace: Option<String>,
}

#[derive(Clone)]
pub(crate) struct ManifestInventoryRow<'a> {
    pub(crate) resource: &'a ManifestResource,
    pub(crate) api_resource: Option<ApiResource>,
    pub(crate) namespace: Option<String>,
    pub(crate) uid: Option<String>,
}

pub(crate) fn manifest_resource_namespace(
    resource: &ManifestResource,
    api_resource: Option<&ApiResource>,
) -> Option<String> {
    api_resource
        .is_none_or(|api_resource| api_resource.namespaced)
        .then(|| resource.namespace.clone())
        .flatten()
}

pub(crate) fn manifest_resources(manifest: &str, release_namespace: &str) -> Vec<ManifestResource> {
    serde_yaml::Deserializer::from_str(manifest)
        .filter_map(|document| serde_yaml::Value::deserialize(document).ok())
        .filter_map(|document| {
            let api_version = document.get("apiVersion")?.as_str()?.to_owned();
            let kind = document.get("kind")?.as_str()?.to_owned();
            let metadata = document.get("metadata")?;
            let name = metadata.get("name")?.as_str()?.to_owned();
            let namespace = metadata
                .get("namespace")
                .and_then(serde_yaml::Value::as_str)
                .map(ToOwned::to_owned)
                .or_else(|| Some(release_namespace.to_owned()));
            Some(ManifestResource {
                api_version,
                kind,
                name,
                namespace,
            })
        })
        .collect()
}
