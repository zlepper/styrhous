use super::{ManifestResource, manifest_resource_namespace, manifest_resources};
use crate::api_resource::ApiResource;

#[test]
fn manifest_inventory_defaults_a_namespaced_object_to_the_release_namespace() {
    let resources = manifest_resources(
        "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: settings\n---\napiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: api\n  namespace: workloads\n",
        "apps",
    );

    assert_eq!(resources.len(), 2);
    assert_eq!(resources[0].namespace.as_deref(), Some("apps"));
    assert_eq!(resources[1].namespace.as_deref(), Some("workloads"));
}

#[test]
fn manifest_inventory_marks_cluster_scoped_resources_as_cluster_wide() {
    let resource = ManifestResource {
        api_version: "rbac.authorization.k8s.io/v1".into(),
        kind: "ClusterRole".into(),
        name: "readers".into(),
        namespace: Some("apps".into()),
    };
    let api_resource = ApiResource {
        group: "rbac.authorization.k8s.io".into(),
        version: "v1".into(),
        kind: "ClusterRole".into(),
        name: "clusterroles".into(),
        namespaced: false,
    };

    assert_eq!(
        manifest_resource_namespace(&resource, Some(&api_resource)),
        None
    );
}
