use crate::resource_detail::{ConfigMapDetail, ResourceDetailPayload};
use k8s_openapi::api::core::v1::ConfigMap;

pub(crate) fn detail_payload(object: &kube::api::DynamicObject) -> Option<ResourceDetailPayload> {
    let config_map = k8s_openapi::serde_json::from_value::<ConfigMap>(
        k8s_openapi::serde_json::to_value(object).ok()?,
    )
    .ok()?;
    Some(ResourceDetailPayload::ConfigMap(ConfigMapDetail {
        data: config_map.data.unwrap_or_default(),
        immutable: config_map.immutable.unwrap_or(false),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detail_payload_includes_text_data_and_immutability() {
        let object: kube::api::DynamicObject =
            k8s_openapi::serde_json::from_value(k8s_openapi::serde_json::json!({
                "apiVersion": "v1",
                "kind": "ConfigMap",
                "metadata": {"name": "settings"},
                "immutable": true,
                "data": {"z-last": "two", "a-first": "one"},
                "binaryData": {"ignored": "AQI="}
            }))
            .unwrap();

        let Some(ResourceDetailPayload::ConfigMap(detail)) = detail_payload(&object) else {
            panic!("ConfigMap should produce a ConfigMap detail payload");
        };
        assert!(detail.immutable);
        assert_eq!(
            detail.data.keys().collect::<Vec<_>>(),
            vec!["a-first", "z-last"]
        );
        assert_eq!(detail.data["a-first"], "one");
    }
}
