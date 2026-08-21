use super::*;

fn api_resource(kind: &str) -> ApiResource {
    ApiResource {
        group: "core".to_owned(),
        version: "v1".to_owned(),
        kind: kind.to_owned(),
        name: format!("{}s", kind.to_ascii_lowercase()),
        namespaced: true,
    }
}

#[test]
fn extracts_copyable_service_diagnostics() {
    let object = serde_json::from_value(serde_json::json!({
        "apiVersion": "v1",
        "kind": "Service",
        "metadata": {"name": "api"},
        "spec": {
            "type": "LoadBalancer",
            "clusterIP": "10.96.0.10",
            "selector": {"app": "api"},
            "ports": [{"port": 443, "protocol": "TCP", "targetPort": "https"}]
        },
        "status": {"loadBalancer": {"ingress": [{"hostname": "api.example.test"}]}}
    }))
    .unwrap();

    let detail = detail_payload(&api_resource("Service"), &object).unwrap();
    let fields = &detail.sections[0].fields;
    assert!(
        fields
            .iter()
            .any(|field| field.label == "Cluster IP" && field.value == "10.96.0.10")
    );
    assert!(
        fields
            .iter()
            .any(|field| field.label == "Ports" && field.value.contains("443/TCP → https"))
    );
    assert!(
        fields
            .iter()
            .any(|field| field.label == "Load balancer ingress"
                && field.value.contains("api.example.test"))
    );
}

#[test]
fn only_curated_resource_kinds_receive_payload_diagnostics() {
    let object = serde_json::from_value(serde_json::json!({
        "apiVersion": "example.test/v1", "kind": "Widget", "metadata": {"name": "one"},
        "spec": {"endpoint": "https://example.test"}
    }))
    .unwrap();
    assert!(detail_payload(&api_resource("Widget"), &object).is_none());
}

#[test]
fn custom_resources_with_a_curated_kind_name_remain_metadata_only() {
    let object = serde_json::from_value(serde_json::json!({
        "apiVersion": "example.test/v1", "kind": "Service", "metadata": {"name": "one"},
        "spec": {"credentials": "must-not-be-rendered"}
    }))
    .unwrap();
    let mut resource = api_resource("Service");
    resource.group = "example.test".to_owned();
    assert!(detail_payload(&resource, &object).is_none());
}
