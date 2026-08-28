use k8s_openapi::api::core::v1::Namespace;
use std::collections::BTreeMap;
use std::fmt::Display;

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct MinimalNamespace {
    pub name: String,
    pub labels: BTreeMap<String, String>,
    pub annotations: BTreeMap<String, String>,
}

impl From<Namespace> for MinimalNamespace {
    fn from(value: Namespace) -> Self {
        Self {
            name: value.metadata.name.unwrap(),
            labels: value.metadata.labels.unwrap_or_default(),
            annotations: value.metadata.annotations.unwrap_or_default(),
        }
    }
}

impl Display for MinimalNamespace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.name, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;

    #[test]
    fn retains_namespace_metadata_for_local_display_configuration() {
        let namespace = MinimalNamespace::from(Namespace {
            metadata: ObjectMeta {
                name: Some("company-a1b2".into()),
                labels: Some(BTreeMap::from([("example/customer".into(), "Acme".into())])),
                annotations: Some(BTreeMap::from([(
                    "example/environment".into(),
                    "prod".into(),
                )])),
                ..Default::default()
            },
            ..Default::default()
        });

        assert_eq!(namespace.name, "company-a1b2");
        assert_eq!(namespace.labels["example/customer"], "Acme");
        assert_eq!(namespace.annotations["example/environment"], "prod");
    }
}
