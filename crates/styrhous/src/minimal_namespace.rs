use crate::resource_extensions::ResourceExt;
use k8s_openapi::api::core::v1::Namespace;
use std::cmp::Ordering;
use std::fmt::Display;

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct MinimalNamespace {
    pub name: String,
    pub display_name: Option<String>,
}

impl Ord for MinimalNamespace {
    fn cmp(&self, other: &Self) -> Ordering {
        self.display_name
            .as_ref()
            .unwrap_or(&self.name)
            .to_lowercase()
            .cmp(
                &other
                    .display_name
                    .as_ref()
                    .unwrap_or(&other.name)
                    .to_lowercase(),
            )
            .then_with(|| self.display_name.cmp(&other.display_name))
            .then_with(|| self.name.cmp(&other.name))
    }
}

impl PartialOrd for MinimalNamespace {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl From<Namespace> for MinimalNamespace {
    fn from(value: Namespace) -> Self {
        Self {
            display_name: value.try_get_display_name(),
            name: value.metadata.name.unwrap(),
        }
    }
}

impl Display for MinimalNamespace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.name, f)
    }
}

impl MinimalNamespace {
    pub fn get_name_to_display(&self) -> &str {
        self.display_name.as_ref().unwrap_or(&self.name)
    }
}
