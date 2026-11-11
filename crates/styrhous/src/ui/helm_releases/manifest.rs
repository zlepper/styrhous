use crate::api_resource::ApiResource;
use serde::Deserialize;
use std::panic::{AssertUnwindSafe, catch_unwind};

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
    // Duplicate keys only fail their own document, but after a libyaml syntax error serde_yaml
    // returns the same error forever. Skip distinct errors and stop once one repeats, or the
    // iterator never ends.
    let mut previous_error = None;
    serde_yaml::Deserializer::from_str(manifest)
        .map_while(|document| {
            // serde_yaml 0.9.34 panics in de.rs on unknown aliases instead of returning an error.
            // The document is already loaded, so the loader advances and the next one is fine.
            let Ok(parsed) = catch_unwind(AssertUnwindSafe(|| {
                serde_yaml::Value::deserialize(document)
            })) else {
                return Some(None);
            };
            match parsed {
                Ok(document) => {
                    previous_error = None;
                    Some(Some(document))
                }
                Err(error) => {
                    let error = error.to_string();
                    (previous_error.replace(error.clone()) != Some(error)).then_some(None)
                }
            }
        })
        .flatten()
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
