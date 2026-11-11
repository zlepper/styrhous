use super::{ManifestResource, ParsedManifest, manifest_resource_namespace, manifest_resources};
use crate::api_resource::ApiResource;
use crate::helm_release::{HelmRelease, StorageDriver};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

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

#[test]
fn manifest_inventory_terminates_and_keeps_documents_before_a_syntax_error() {
    let manifest = "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: settings\n---\nkey: [unclosed\n\tbad: : :\n---\napiVersion: v1\nkind: Secret\nmetadata:\n  name: after\n";
    let (sender, receiver) = mpsc::channel();
    // A regression spins forever, so parse on a detached thread and bound the wait.
    thread::spawn(move || {
        let _ = sender.send(manifest_resources(manifest, "apps"));
    });

    let resources = receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("parsing a manifest with a syntax error must terminate");

    assert_eq!(resources.len(), 1);
    assert_eq!(resources[0].name, "settings");
}

#[test]
fn manifest_inventory_skips_documents_that_fail_without_breaking_the_parser() {
    let manifest = "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: first\n---\napiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: dup\n  name: dup-again\n---\napiVersion: v1\nkind: Secret\nmetadata:\n  name: last\n";

    let names: Vec<_> = manifest_resources(manifest, "apps")
        .into_iter()
        .map(|resource| resource.name)
        .collect();

    assert_eq!(names, ["first", "last"]);
}

fn release_with(revision: i64, name: &str) -> HelmRelease {
    HelmRelease {
        storage: StorageDriver::Secret,
        storage_name: format!("sh.helm.release.v1.demo.v{revision}"),
        name: "demo".into(),
        namespace: "apps".into(),
        revision,
        status: "deployed".into(),
        description: String::new(),
        notes: String::new(),
        chart: "nginx".into(),
        chart_version: "1.0.0".into(),
        app_version: "1.0.0".into(),
        first_deployed: String::new(),
        last_deployed: String::new(),
        values: serde_json::Value::Null,
        manifest: format!("apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: {name}\n"),
        storage_labels: Default::default(),
        storage_annotations: Default::default(),
    }
}

#[test]
fn parsed_manifest_is_reused_until_the_revision_or_manifest_changes() {
    let mut cache = None;
    let release = release_with(1, "one");

    let first = ParsedManifest::resources_for(&mut cache, &release).as_ptr();
    let again = ParsedManifest::resources_for(&mut cache, &release);
    assert_eq!(first, again.as_ptr());
    assert_eq!(again[0].name, "one");

    let next_revision = release_with(2, "two");
    assert_eq!(
        ParsedManifest::resources_for(&mut cache, &next_revision)[0].name,
        "two"
    );

    let edited = release_with(2, "edited");
    assert_eq!(
        ParsedManifest::resources_for(&mut cache, &edited)[0].name,
        "edited"
    );
}

#[test]
fn manifest_inventory_survives_consecutive_unknown_aliases_between_valid_documents() {
    let manifest = "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: first\n---\nkind: ConfigMap\nmetadata:\n  name: *missing\n---\nkind: ConfigMap\nmetadata:\n  name: *also-missing\n---\napiVersion: v1\nkind: Secret\nmetadata:\n  name: last\n";

    let names: Vec<_> = manifest_resources(manifest, "apps")
        .into_iter()
        .map(|resource| resource.name)
        .collect();

    assert_eq!(names, ["first", "last"]);
}
