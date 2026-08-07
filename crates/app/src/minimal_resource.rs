use crate::resource_table::CellValue;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use time::OffsetDateTime;

/// A lightweight representation of any Kubernetes resource for UI display.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MinimalResource {
    /// Unique identifier (metadata.uid or fallback to namespace/name)
    pub uid: String,
    /// Resource name
    pub name: String,
    /// Namespace (None for cluster-scoped resources)
    pub namespace: Option<String>,
    /// Creation timestamp
    pub creation_timestamp: Option<OffsetDateTime>,
    /// Type-specific values keyed by the selected resource table definition.
    pub cells: BTreeMap<String, CellValue>,
}

impl Ord for MinimalResource {
    fn cmp(&self, other: &Self) -> Ordering {
        self.name.to_lowercase().cmp(&other.name.to_lowercase())
    }
}

impl PartialOrd for MinimalResource {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl MinimalResource {
    /// Calculate age from creation_timestamp as human-readable string.
    pub fn age(&self) -> String {
        format_age(self.creation_timestamp)
    }
}

pub(crate) fn format_age(creation_timestamp: Option<OffsetDateTime>) -> String {
    match creation_timestamp {
        Some(ts) => {
            let now = OffsetDateTime::now_utc();
            let duration = now - ts;
            format_duration(duration)
        }
        None => "Unknown".to_string(),
    }
}

fn format_duration(duration: time::Duration) -> String {
    let seconds = duration.whole_seconds();
    if seconds < 0 {
        return "0s".to_string();
    }

    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;

    if days > 0 {
        format!("{}d", days)
    } else if hours > 0 {
        format!("{}h", hours)
    } else if minutes > 0 {
        format!("{}m", minutes)
    } else {
        format!("{}s", secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_age_formatting() {
        let now = OffsetDateTime::now_utc();

        // 2 hours ago
        let resource = MinimalResource {
            uid: "test".to_string(),
            name: "test-pod".to_string(),
            namespace: Some("default".to_string()),
            creation_timestamp: Some(now - time::Duration::hours(2)),
            cells: BTreeMap::new(),
        };
        assert_eq!(resource.age(), "2h");

        // 3 days ago
        let resource = MinimalResource {
            uid: "test".to_string(),
            name: "test-pod".to_string(),
            namespace: Some("default".to_string()),
            creation_timestamp: Some(now - time::Duration::days(3)),
            cells: BTreeMap::new(),
        };
        assert_eq!(resource.age(), "3d");

        // 45 minutes ago
        let resource = MinimalResource {
            uid: "test".to_string(),
            name: "test-pod".to_string(),
            namespace: Some("default".to_string()),
            creation_timestamp: Some(now - time::Duration::minutes(45)),
            cells: BTreeMap::new(),
        };
        assert_eq!(resource.age(), "45m");

        // 30 seconds ago
        let resource = MinimalResource {
            uid: "test".to_string(),
            name: "test-pod".to_string(),
            namespace: Some("default".to_string()),
            creation_timestamp: Some(now - time::Duration::seconds(30)),
            cells: BTreeMap::new(),
        };
        assert_eq!(resource.age(), "30s");
    }

    #[test]
    fn test_unknown_age() {
        let resource = MinimalResource {
            uid: "test".to_string(),
            name: "test-pod".to_string(),
            namespace: None,
            creation_timestamp: None,
            cells: BTreeMap::new(),
        };
        assert_eq!(resource.age(), "Unknown");
    }
}
