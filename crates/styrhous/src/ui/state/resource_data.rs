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
        let mut saved_values = self.server_values.clone();
        for key in expected.keys() {
            if let Some(value) = updated.get(key) {
                saved_values.insert(key.clone(), value.clone());
            }
        }
        match self.pending_external_values.as_ref() {
            Some(external_values) if external_values == &saved_values => {
                self.server_values = saved_values;
                self.pending_external_values = None;
                if let Some(resource_version) = self.pending_external_resource_version.take() {
                    self.resource_version = resource_version;
                }
            }
            Some(_) => {}
            None => {
                self.server_values = saved_values;
                self.pending_external_resource_version = None;
            }
        }
        self.saving = false;
        self.pending_save_request_id = None;
        self.save_error = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completed_save_reconciles_an_early_matching_watch_update() {
        let mut editor = saving_editor();
        editor.accept_watched_values(
            BTreeMap::from([
                ("password".to_owned(), "updated".to_owned()),
                ("preserved".to_owned(), "value".to_owned()),
            ]),
            "2".to_owned(),
        );

        editor.mark_saved();

        assert_eq!(
            editor.server_values,
            BTreeMap::from([
                ("password".to_owned(), "updated".to_owned()),
                ("preserved".to_owned(), "value".to_owned()),
            ])
        );
        assert_eq!(editor.resource_version, "2");
        assert_eq!(editor.pending_external_values, None);
        assert_eq!(editor.pending_external_resource_version, None);
        assert!(!editor.saving);
        assert_eq!(editor.pending_save_request_id, None);
    }

    #[test]
    fn completed_save_preserves_a_divergent_external_watch_update() {
        let mut editor = saving_editor();
        let external_values = BTreeMap::from([
            ("password".to_owned(), "external".to_owned()),
            ("preserved".to_owned(), "value".to_owned()),
        ]);
        editor.accept_watched_values(external_values.clone(), "2".to_owned());

        editor.mark_saved();

        assert_eq!(
            editor.server_values.get("password").map(String::as_str),
            Some("original")
        );
        assert_eq!(
            editor.draft_values.get("password").map(String::as_str),
            Some("updated")
        );
        assert_eq!(editor.resource_version, "1");
        assert_eq!(editor.pending_external_values, Some(external_values));
        assert_eq!(
            editor.pending_external_resource_version.as_deref(),
            Some("2")
        );
        assert!(!editor.saving);
        assert_eq!(editor.pending_save_request_id, None);

        editor.keep_local_edits();

        assert_eq!(
            editor.server_values.get("password").map(String::as_str),
            Some("external")
        );
        assert_eq!(
            editor.draft_values.get("password").map(String::as_str),
            Some("updated")
        );
        assert_eq!(editor.resource_version, "2");
        assert_eq!(editor.pending_external_values, None);
        assert_eq!(editor.pending_external_resource_version, None);
    }

    fn saving_editor() -> ResourceDataEditorState {
        let mut editor = ResourceDataEditorState::new(
            BTreeMap::from([
                ("password".to_owned(), "original".to_owned()),
                ("preserved".to_owned(), "value".to_owned()),
            ]),
            "1".to_owned(),
        );
        editor
            .draft_values
            .insert("password".to_owned(), "updated".to_owned());
        editor.saving = true;
        editor.pending_save_request_id = Some(7);
        editor
    }
}
