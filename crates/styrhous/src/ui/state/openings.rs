use super::*;

impl UiState {
    pub(crate) fn open_terminal_settings(
        &mut self,
        settings: &TerminalLaunchSettings,
        commands_to_send: &mut Vec<WorkerCommandBox>,
    ) {
        self.replace_global_blade(
            Box::new(super::super::settings::TerminalSettingsBlade::new(
                settings.clone(),
            )),
            commands_to_send,
        );
    }

    pub(crate) fn open_settings_home(&mut self, commands_to_send: &mut Vec<WorkerCommandBox>) {
        self.replace_global_blade(
            Box::new(super::super::settings::SettingsHomeBlade),
            commands_to_send,
        );
    }

    pub(crate) fn open_pod_log_window(
        &mut self,
        cluster_key: i32,
        pod_name: String,
        namespace: Option<String>,
        container: PodLogContainer,
        commands_to_send: &mut Vec<WorkerCommandBox>,
    ) {
        let Some(namespace) = namespace else {
            return;
        };
        self.next_log_window_id += 1;
        let log_window_id = self.next_log_window_id;
        self.log_windows.insert(
            log_window_id,
            PodLogWindowState::new(
                log_window_id,
                cluster_key,
                namespace.clone(),
                pod_name.clone(),
                container.clone(),
            ),
        );
        commands_to_send.push(Box::new(crate::worker::StartPodLogStream {
            cluster_key,
            log_window_id,
            namespace,
            pod_name,
            container: container.name,
        }));
    }

    pub(crate) fn open_yaml_editor(
        &mut self,
        ctx: &egui::Context,
        cluster_key: i32,
        api_resource: ApiResource,
        namespace: Option<String>,
        resource_name: String,
        commands_to_send: &mut Vec<WorkerCommandBox>,
    ) {
        if let Some(editor) = self.yaml_editors.values_mut().find(|editor| {
            editor.resource_matches(cluster_key, &api_resource, &namespace, &resource_name)
        }) {
            editor.focus_requested = true;
            ctx.send_viewport_cmd_to(
                egui::ViewportId::from_hash_of(("yaml-editor-window", editor.id)),
                egui::ViewportCommand::Focus,
            );
            return;
        }

        self.next_yaml_editor_id += 1;
        let editor_id = self.next_yaml_editor_id;
        self.yaml_editors.insert(
            editor_id,
            YamlEditorWindowState {
                id: editor_id,
                cluster_key,
                api_resource: api_resource.clone(),
                namespace: namespace.clone(),
                resource_name: resource_name.clone(),
                original_yaml: None,
                edited_yaml: String::new(),
                loading: true,
                saving: false,
                error: None,
                close_requested: false,
                confirm_discard: false,
                focus_requested: false,
                schema: self
                    .resource_schemas
                    .get(&(cluster_key, api_resource.clone()))
                    .cloned(),
                schema_loading: !self
                    .resource_schemas
                    .contains_key(&(cluster_key, api_resource.clone())),
                diagnostics: Vec::new(),
                retained_diagnostics: Vec::new(),
                scroll_to_diagnostic: None,
                server_validation: ValidationState::Idle,
                validation_revision: 0,
                validation_due: None,
                suggestions: Vec::new(),
                completion_context: None,
                completion_cursor: None,
                suggestions_visible: false,
                suggestion_selection: 0,
                search: YamlEditorSearchState::default(),
                highlight_cache: YamlEditorHighlightCache::default(),
            },
        );
        commands_to_send.push(Box::new(crate::worker::GetResourceYaml {
            editor_id,
            cluster_key,
            api_resource: api_resource.clone(),
            namespace: namespace.clone(),
            resource_name: resource_name.clone(),
        }));
        if !self
            .resource_schemas
            .contains_key(&(cluster_key, api_resource.clone()))
        {
            commands_to_send.push(Box::new(crate::worker::LoadResourceSchema {
                editor_id,
                cluster_key,
                api_resource,
            }));
        }
    }
}
