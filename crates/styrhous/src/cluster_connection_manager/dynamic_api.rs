//! Dynamic Kubernetes API construction shared by resource operations and watchers.

use crate::api_resource::ApiResource;
use anyhow::{Result, bail};
use kube::Api;
use kube::api::{DynamicObject, GroupVersionKind};

/// kube's description of `api_resource`, built from what discovery already reported
/// instead of asking the API server for it again.
pub(super) fn kube_api_resource(api_resource: &ApiResource) -> kube::discovery::ApiResource {
    let group = if api_resource.group == "core" {
        ""
    } else {
        &api_resource.group
    };
    let gvk = GroupVersionKind::gvk(group, &api_resource.version, &api_resource.kind);
    kube::discovery::ApiResource::from_gvk_with_plural(&gvk, &api_resource.name)
}

/// Create a dynamic API only when the caller's namespace agrees with the discovered scope.
pub(super) fn create(
    client: &kube::Client,
    api_resource: &ApiResource,
    namespace: Option<&str>,
) -> Result<Api<DynamicObject>> {
    let resource = kube_api_resource(api_resource);
    match (api_resource.namespaced, namespace) {
        (true, Some(namespace)) => Ok(Api::namespaced_with(client.clone(), namespace, &resource)),
        (false, None) => Ok(Api::all_with(client.clone(), &resource)),
        (namespaced, namespace) => bail!(
            "Resource scope mismatch: {} is {} but the request has namespace {namespace:?}",
            api_resource.kind,
            if namespaced {
                "namespaced"
            } else {
                "cluster-scoped"
            },
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resource(group: &str, namespaced: bool) -> ApiResource {
        ApiResource {
            group: group.to_owned(),
            version: "v1".to_owned(),
            kind: "Widget".to_owned(),
            name: "widgets".to_owned(),
            namespaced,
        }
    }

    fn client() -> kube::Client {
        let config = kube::Config::new("https://kubernetes.example".parse().expect("valid URL"));
        kube::Client::try_from(config).expect("client")
    }

    #[test]
    fn the_kube_resource_uses_the_discovered_plural_and_the_empty_core_group() {
        let custom = kube_api_resource(&resource("example.com", true));
        let core = kube_api_resource(&resource("core", true));

        assert_eq!(custom.api_version, "example.com/v1");
        assert_eq!(custom.plural, "widgets");
        assert_eq!(core.group, "");
        assert_eq!(core.api_version, "v1");
    }

    #[tokio::test]
    async fn the_namespace_must_match_the_discovered_scope() {
        let client = client();

        assert!(create(&client, &resource("example.com", true), Some("default")).is_ok());
        assert!(create(&client, &resource("example.com", false), None).is_ok());
        assert!(create(&client, &resource("example.com", true), None).is_err());
        assert!(create(&client, &resource("example.com", false), Some("default")).is_err());
    }
}
