//! Dynamic Kubernetes API construction shared by resource operations and detail watchers.

use crate::api_resource::ApiResource;
use anyhow::{Result, bail};
use kube::Api;
use kube::api::{DynamicObject, GroupVersionKind};

/// Create a dynamic API only when the caller's namespace agrees with the discovered scope.
pub(super) async fn create(
    client: &kube::Client,
    api_resource: &ApiResource,
    namespace: Option<&str>,
) -> Result<Api<DynamicObject>> {
    let group = if api_resource.group == "core" {
        ""
    } else {
        &api_resource.group
    };
    let gvk = GroupVersionKind::gvk(group, &api_resource.version, &api_resource.kind);
    let (resource, capabilities) = kube::discovery::pinned_kind(client, &gvk).await?;

    match (capabilities.scope, namespace) {
        (kube::discovery::Scope::Namespaced, Some(namespace)) => {
            Ok(Api::namespaced_with(client.clone(), namespace, &resource))
        }
        (kube::discovery::Scope::Cluster, None) => Ok(Api::all_with(client.clone(), &resource)),
        (scope, namespace) => bail!(
            "Resource scope mismatch: discovered {scope:?} scope with namespace {namespace:?}"
        ),
    }
}
