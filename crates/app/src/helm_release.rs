//! Helm v3 release records stored by Kubernetes-native storage drivers.

use base64::Engine;
use flate2::read::GzDecoder;
use k8s_openapi::serde_json::Value;
use std::collections::BTreeMap;
use std::io::Read;

const MAX_ENCODED_RELEASE_BYTES: usize = 8 * 1024 * 1024;
const MAX_DECOMPRESSED_RELEASE_BYTES: usize = 32 * 1024 * 1024;

pub(crate) const GROUP: &str = "helm.sh";
pub(crate) const VERSION: &str = "v1";
pub(crate) const KIND: &str = "HelmRelease";
pub(crate) const NAME: &str = "releases";

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum StorageDriver {
    Secret,
    ConfigMap,
}

impl std::fmt::Display for StorageDriver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Secret => "Secret",
            Self::ConfigMap => "ConfigMap",
        })
    }
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct HelmRelease {
    pub(crate) storage: StorageDriver,
    pub(crate) storage_name: String,
    pub(crate) name: String,
    pub(crate) namespace: String,
    pub(crate) revision: i64,
    pub(crate) status: String,
    pub(crate) description: String,
    pub(crate) notes: String,
    pub(crate) chart: String,
    pub(crate) chart_version: String,
    pub(crate) app_version: String,
    pub(crate) first_deployed: String,
    pub(crate) last_deployed: String,
    pub(crate) values: Value,
    pub(crate) manifest: String,
    pub(crate) storage_labels: BTreeMap<String, String>,
    pub(crate) storage_annotations: BTreeMap<String, String>,
}

impl std::fmt::Debug for HelmRelease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HelmRelease")
            .field("storage", &self.storage)
            .field("storage_name", &self.storage_name)
            .field("name", &self.name)
            .field("namespace", &self.namespace)
            .field("revision", &self.revision)
            .field("status", &self.status)
            .field("chart", &self.chart)
            .field("chart_version", &self.chart_version)
            .field("app_version", &self.app_version)
            .finish_non_exhaustive()
    }
}

impl HelmRelease {
    pub(crate) fn id(&self) -> String {
        format!("{}/{}/{}", self.namespace, self.name, self.revision)
    }

    pub(crate) fn values_yaml(&self) -> String {
        serde_yaml::to_string(&self.values).unwrap_or_else(|_| self.values.to_string())
    }
}

/// Decode Helm's inner base64 + gzip JSON envelope. Kubernetes has already
/// decoded the outer Secret data base64 layer before this function is called.
pub(crate) fn decode_release(
    storage: StorageDriver,
    storage_name: String,
    encoded: &[u8],
) -> Result<HelmRelease, String> {
    if encoded.len() > MAX_ENCODED_RELEASE_BYTES {
        return Err("release data exceeds the supported size".to_owned());
    }
    let encoded = std::str::from_utf8(encoded).map_err(|_| "release data is not UTF-8 base64")?;
    let compressed = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| "release data is not valid base64")?;
    let mut json = String::new();
    GzDecoder::new(compressed.as_slice())
        .take((MAX_DECOMPRESSED_RELEASE_BYTES + 1) as u64)
        .read_to_string(&mut json)
        .map_err(|_| "release data is not a valid gzip payload")?;
    if json.len() > MAX_DECOMPRESSED_RELEASE_BYTES {
        return Err("release data exceeds the supported size".to_owned());
    }
    let value: Value = k8s_openapi::serde_json::from_str(&json)
        .map_err(|_| "release data is not valid Helm JSON")?;
    let get = |path: &[&str]| {
        path.iter()
            .try_fold(&value, |current, key| current.get(*key))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let revision = value
        .get("version")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let name = get(&["name"]);
    let namespace = get(&["namespace"]);
    if name.is_empty() || namespace.is_empty() || revision < 1 {
        return Err("release metadata is incomplete".to_owned());
    }
    Ok(HelmRelease {
        storage,
        storage_name,
        name,
        namespace,
        revision,
        status: get(&["info", "status"]),
        description: get(&["info", "description"]),
        notes: get(&["info", "notes"]),
        chart: get(&["chart", "metadata", "name"]),
        chart_version: get(&["chart", "metadata", "version"]),
        app_version: get(&["chart", "metadata", "appVersion"]),
        first_deployed: get(&["info", "first_deployed"]),
        last_deployed: get(&["info", "last_deployed"]),
        values: value
            .get("config")
            .cloned()
            .unwrap_or(Value::Object(Default::default())),
        manifest: get(&["manifest"]),
        storage_labels: BTreeMap::new(),
        storage_annotations: BTreeMap::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn encoded_release() -> Vec<u8> {
        let json = r#"{"name":"demo","namespace":"apps","version":2,"info":{"status":"deployed","description":"ok","notes":"Your release is ready."},"chart":{"metadata":{"name":"nginx","version":"1.2.3","appVersion":"1.25"}},"config":{"password":"not-logged"},"manifest":"apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: demo"}"#;
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gzip.write_all(json.as_bytes()).unwrap();
        base64::engine::general_purpose::STANDARD
            .encode(gzip.finish().unwrap())
            .into_bytes()
    }

    #[test]
    fn decodes_a_kubernetes_stored_helm_release_without_debugging_values() {
        let release = decode_release(
            StorageDriver::Secret,
            "sh.helm.release.v1.demo.v2".into(),
            &encoded_release(),
        )
        .unwrap();
        assert_eq!(release.id(), "apps/demo/2");
        assert_eq!(release.chart, "nginx");
        assert_eq!(release.notes, "Your release is ready.");
        assert!(release.values_yaml().contains("not-logged"));
        assert!(!format!("{release:?}").contains("not-logged"));
    }

    #[test]
    fn config_map_storage_uses_the_same_helm_envelope() {
        let release = decode_release(
            StorageDriver::ConfigMap,
            "sh.helm.release.v1.demo.v2".into(),
            &encoded_release(),
        )
        .unwrap();

        assert_eq!(release.storage, StorageDriver::ConfigMap);
        assert_eq!(
            release.manifest,
            "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: demo"
        );
    }
}
