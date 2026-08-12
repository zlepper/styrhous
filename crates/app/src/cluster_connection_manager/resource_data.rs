//! Shared optimistic-concurrency checks for resource-data mutations.

use anyhow::{Result, bail};
use std::collections::BTreeMap;

pub(super) fn validate_update_request(
    expected_values: &BTreeMap<String, String>,
    updated_values: &BTreeMap<String, String>,
    expected_resource_version: &str,
) -> Result<()> {
    if expected_values.is_empty() || updated_values.is_empty() {
        bail!("Resource data update must contain at least one existing value");
    }
    if expected_values.keys().ne(updated_values.keys()) {
        bail!("Resource data update expected and updated keys must match");
    }
    if expected_resource_version.is_empty() {
        bail!("Resource data update is missing the watched resource version");
    }
    Ok(())
}

pub(super) fn validate_resource_version(
    actual: Option<&str>,
    expected: &str,
    resource_kind: &str,
) -> Result<()> {
    if actual != Some(expected) {
        bail!("{resource_kind} changed on the cluster; reload its data before saving");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_update_requests() {
        let expected = BTreeMap::from([("key".to_owned(), "old".to_owned())]);
        assert_eq!(
            validate_update_request(&expected, &BTreeMap::new(), "1")
                .unwrap_err()
                .to_string(),
            "Resource data update must contain at least one existing value"
        );
        assert_eq!(
            validate_update_request(&BTreeMap::new(), &expected, "1")
                .unwrap_err()
                .to_string(),
            "Resource data update must contain at least one existing value"
        );
        assert_eq!(
            validate_update_request(&expected, &expected, "")
                .unwrap_err()
                .to_string(),
            "Resource data update is missing the watched resource version"
        );
        assert!(validate_update_request(&expected, &expected, "1").is_ok());
    }

    #[test]
    fn requires_the_watched_resource_version() {
        assert!(validate_resource_version(Some("2"), "1", "ConfigMap").is_err());
        assert!(validate_resource_version(None, "1", "ConfigMap").is_err());
        assert!(validate_resource_version(Some("1"), "1", "ConfigMap").is_ok());
    }
}
