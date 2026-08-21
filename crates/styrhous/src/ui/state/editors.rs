use super::*;

#[derive(Debug, Clone)]

pub(crate) struct YamlEditorWindowState {
    pub(crate) id: u64,
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) resource_name: String,
    pub(crate) original_yaml: Option<String>,
    pub(crate) edited_yaml: String,
    pub(crate) loading: bool,
    pub(crate) saving: bool,
    pub(crate) error: Option<String>,
    pub(crate) close_requested: bool,
    pub(crate) confirm_discard: bool,
    pub(crate) focus_requested: bool,
    pub(crate) schema: Option<ResourceSchema>,
    pub(crate) schema_loading: bool,
    pub(crate) diagnostics: Vec<YamlDiagnostic>,
    /// The last diagnostics shown in the pane while a newer document is being validated.
    /// These are intentionally separate from `diagnostics`, whose ranges must always match
    /// the current editor buffer before they are used for line markers or squiggles.
    pub(crate) retained_diagnostics: Vec<YamlDiagnostic>,
    pub(crate) scroll_to_diagnostic: Option<SourceRange>,
    pub(crate) server_validation: ValidationState,
    pub(crate) validation_revision: u64,
    pub(crate) validation_due: Option<Instant>,
    pub(crate) suggestions: Vec<CompletionSuggestion>,
    pub(crate) completion_context: Option<CompletionContext>,
    pub(crate) completion_cursor: Option<usize>,
    pub(crate) suggestions_visible: bool,
    pub(crate) suggestion_selection: usize,
    pub(crate) search: YamlEditorSearchState,
    pub(crate) highlight_cache: YamlEditorHighlightCache,
}

/// A syntax-highlighted job, independent of egui's font atlas.
///
/// The job is invalidated whenever its source or search state differs. Egui
/// continues to own the `Galley` cache, so glyph-atlas and font changes remain
/// handled by its normal lifecycle.
#[derive(Debug, Clone, Default)]
pub(crate) struct YamlEditorHighlightCache {
    entry: Option<(YamlEditorHighlightCacheKey, Arc<egui::text::LayoutJob>)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct YamlEditorHighlightCacheKey {
    search_query: String,
    search_regex_mode: bool,
    active_match: Option<Range<usize>>,
}

impl YamlEditorHighlightCacheKey {
    pub(crate) fn new(
        search_query: &str,
        search_regex_mode: bool,
        active_match: Option<&Range<usize>>,
    ) -> Self {
        Self {
            search_query: search_query.to_owned(),
            search_regex_mode,
            active_match: active_match.cloned(),
        }
    }
}

impl YamlEditorHighlightCache {
    pub(crate) fn layout_job(
        &self,
        key: &YamlEditorHighlightCacheKey,
        yaml: &str,
    ) -> Option<Arc<egui::text::LayoutJob>> {
        self.entry
            .as_ref()
            .filter(|(cached_key, job)| cached_key == key && job.text == yaml)
            .map(|(_, job)| Arc::clone(job))
    }

    pub(crate) fn store(&mut self, key: YamlEditorHighlightCacheKey, job: egui::text::LayoutJob) {
        self.entry = Some((key, Arc::new(job)));
    }

    #[cfg(test)]
    pub(crate) fn stored_job(&self) -> Option<&Arc<egui::text::LayoutJob>> {
        self.entry.as_ref().map(|(_, job)| job)
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct YamlEditorSearchState {
    pub(crate) query: String,
    pub(crate) regex_mode: bool,
    pub(crate) input_focused: bool,
    pub(crate) active_match: Option<usize>,
    /// The next rendered editor frame scrolls this match into view, then clears it.
    pub(crate) scroll_to_match: Option<usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) enum ValidationState {
    #[default]
    Idle,
    Pending,
    Valid,
    Failed(String),
}

pub(crate) fn diagnostics_from_api_error(
    error: &ResourceApiError,
    yaml: &str,
) -> Vec<YamlDiagnostic> {
    error
        .causes
        .iter()
        .filter_map(|cause| {
            let detail = if cause.message.is_empty() {
                cause.reason.as_str()
            } else {
                cause.message.as_str()
            };
            if detail.is_empty() {
                return None;
            }
            let message = if cause.field.is_empty() {
                detail.to_owned()
            } else {
                format!("{}: {detail}", cause.field)
            };
            let path = kubernetes_field_path_to_json_pointer(&cause.field).unwrap_or_default();
            Some(YamlDiagnostic::at_path(path, message).locate_in(yaml))
        })
        .collect()
}

pub(crate) fn api_error_message(error: &ResourceApiError) -> String {
    if !error.message.is_empty() {
        error.message.clone()
    } else {
        "The Kubernetes API rejected this resource".into()
    }
}

pub(crate) fn set_editor_diagnostics(
    editor: &mut YamlEditorWindowState,
    diagnostics: Vec<YamlDiagnostic>,
) {
    editor.diagnostics = diagnostics;
    if !editor.diagnostics.is_empty() {
        editor.retained_diagnostics = editor.diagnostics.clone();
    }
}

impl YamlEditorWindowState {
    pub(crate) fn is_modified(&self) -> bool {
        self.original_yaml
            .as_ref()
            .is_some_and(|original_yaml| original_yaml != &self.edited_yaml)
    }

    pub(crate) fn resource_matches(
        &self,
        cluster_key: i32,
        api_resource: &ApiResource,
        namespace: &Option<String>,
        resource_name: &str,
    ) -> bool {
        self.cluster_key == cluster_key
            && self.api_resource == *api_resource
            && self.namespace == *namespace
            && self.resource_name == resource_name
    }
}
