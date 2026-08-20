use crate::resource_detail::{ResourceDetailPayload, SecretDataDetail, SecretDetail};
use k8s_openapi::api::core::v1::Secret;

pub(crate) fn detail_payload(object: &kube::api::DynamicObject) -> Option<ResourceDetailPayload> {
    let secret = k8s_openapi::serde_json::from_value::<Secret>(
        k8s_openapi::serde_json::to_value(object).ok()?,
    )
    .ok()?;
    let data = secret
        .data
        .unwrap_or_default()
        .into_iter()
        .map(|(key, value)| {
            let bytes = value.0;
            let byte_len = bytes.len();
            let text = String::from_utf8(bytes).ok();
            (key, SecretDataDetail { byte_len, text })
        })
        .collect();
    Some(ResourceDetailPayload::Secret(SecretDetail {
        data,
        immutable: secret.immutable.unwrap_or(false),
        type_: secret.type_.unwrap_or_else(|| "Opaque".to_owned()),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detail_payload_classifies_text_and_binary_secret_values() {
        let object: kube::api::DynamicObject =
            k8s_openapi::serde_json::from_value(k8s_openapi::serde_json::json!({
                "apiVersion": "v1",
                "kind": "Secret",
                "metadata": {"name": "credentials"},
                "type": "kubernetes.io/basic-auth",
                "immutable": true,
                "data": {"password": "c2VjcmV0", "binary": "/wA="}
            }))
            .unwrap();

        let Some(ResourceDetailPayload::Secret(detail)) = detail_payload(&object) else {
            panic!("Secret should produce a Secret detail payload");
        };
        assert_eq!(detail.type_, "kubernetes.io/basic-auth");
        assert!(detail.immutable);
        assert_eq!(detail.data["password"].text.as_deref(), Some("secret"));
        assert_eq!(detail.data["password"].byte_len, 6);
        assert_eq!(detail.data["binary"].text, None);
        assert_eq!(detail.data["binary"].byte_len, 2);
    }
}
