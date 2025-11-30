use std::cmp::Ordering;
use std::fmt::Display;
use k8s_openapi::api::core::v1::Namespace;
use crate::resource_extensions::ResourceExt;

#[derive(Debug, Eq, PartialEq, Ord, Clone)]
pub struct MinimalNamespace {
    pub name: String,
    pub display_name: Option<String>,
}

impl PartialOrd for MinimalNamespace {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let self_display_name = self.display_name.as_ref().unwrap_or(&self.name).to_lowercase();
        let other_display_name = other.display_name.as_ref().unwrap_or(&other.name).to_lowercase();

        self_display_name.partial_cmp(&other_display_name)
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
        &self.display_name.as_ref().unwrap_or(&self.name)
    }
}