use super::*;

#[derive(Debug, Clone)]

pub(crate) struct ResourceDataEditorState {
    /// The last resource data map accepted from the live watcher. Secret entries
    /// which cannot be represented as UTF-8 are deliberately absent.
    pub(crate) server_values: BTreeMap<String, String>,
    pub(crate) resource_version: String,
    pub(crate) draft_values: BTreeMap<String, String>,
    pub(crate) pending_external_values: Option<BTreeMap<String, String>>,
    pub(crate) pending_external_resource_version: Option<String>,
    pub(crate) revealed_secret_keys: HashSet<String>,
    pub(crate) saving: bool,
    pub(crate) pending_save_request_id: Option<u64>,
    pub(crate) save_error: Option<String>,
}

impl ResourceDataEditorState {
    pub(crate) fn new(values: BTreeMap<String, String>, resource_version: String) -> Self {
        Self {
            draft_values: values.clone(),
            server_values: values,
            resource_version,
            pending_external_values: None,
            pending_external_resource_version: None,
            revealed_secret_keys: HashSet::new(),
            saving: false,
            pending_save_request_id: None,
            save_error: None,
        }
    }

    pub(crate) fn is_modified(&self) -> bool {
        self.draft_values != self.server_values
    }

    pub(crate) fn changed_values(&self) -> (BTreeMap<String, String>, BTreeMap<String, String>) {
        let mut expected = BTreeMap::new();
        let mut updated = BTreeMap::new();
        for (key, value) in &self.draft_values {
            if self.server_values.get(key) != Some(value)
                && let Some(expected_value) = self.server_values.get(key)
            {
                expected.insert(key.clone(), expected_value.clone());
                updated.insert(key.clone(), value.clone());
            }
        }
        (expected, updated)
    }

    pub(crate) fn accept_watched_values(
        &mut self,
        values: BTreeMap<String, String>,
        resource_version: String,
    ) {
        if !self.is_modified() {
            self.server_values = values.clone();
            self.draft_values = values;
            self.resource_version = resource_version;
            self.pending_external_values = None;
            self.pending_external_resource_version = None;
            return;
        }
        if self.server_values != values {
            self.pending_external_values = Some(values);
            self.pending_external_resource_version = Some(resource_version);
        } else {
            self.resource_version = resource_version;
        }
    }

    pub(crate) fn use_external_values(&mut self) {
        let Some(values) = self.pending_external_values.take() else {
            return;
        };
        self.server_values = values.clone();
        self.draft_values = values;
        self.resource_version = self
            .pending_external_resource_version
            .take()
            .unwrap_or_default();
        self.save_error = None;
    }

    pub(crate) fn keep_local_edits(&mut self) {
        let Some(values) = self.pending_external_values.take() else {
            return;
        };
        let dirty_values = self
            .draft_values
            .iter()
            .filter(|(key, value)| self.server_values.get(*key) != Some(*value))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<BTreeMap<_, _>>();
        self.server_values = values.clone();
        self.resource_version = self
            .pending_external_resource_version
            .take()
            .unwrap_or_default();
        self.draft_values = values;
        for (key, value) in dirty_values {
            if self.server_values.contains_key(&key) {
                self.draft_values.insert(key, value);
            } else {
                self.save_error = Some(
                    "A changed data key was removed on the cluster and cannot be saved.".to_owned(),
                );
            }
        }
    }

    pub(crate) fn mark_saved(&mut self) {
        let (expected, updated) = self.changed_values();
        for key in expected.keys() {
            if let Some(value) = updated.get(key) {
                self.server_values.insert(key.clone(), value.clone());
            }
        }
        self.saving = false;
        self.pending_save_request_id = None;
        self.save_error = None;
    }
}
