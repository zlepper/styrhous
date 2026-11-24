use super::*;

impl ApplyResourceYaml {
    fn failure(&self) -> ResourceYamlApplyCommandFailed {
        ResourceYamlApplyCommandFailed {
            editor_id: self.editor_id,
            cluster_key: self.cluster_key,
            api_resource: self.api_resource.clone(),
            namespace: self.namespace.clone(),
            resource_name: self.resource_name.clone(),
            error: String::new(),
        }
    }
}

#[async_trait]
impl WorkerCommand for ApplyResourceYaml {
    type Output = Result<ResourceApplyCompleted, ResourceYamlApplyFailure>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let failure = self.failure();
        state
            .api_call(
                self.cluster_key,
                ApiCall::mutation("Applying the YAML"),
                |client| {
                    apply_resource_yaml(ResourceYamlApplyRequest {
                        editor_id: self.editor_id,
                        cluster_key: self.cluster_key,
                        client,
                        api_resource: self.api_resource,
                        namespace: self.namespace,
                        resource_name: self.resource_name,
                        original_yaml: self.original_yaml,
                        yaml: self.yaml,
                        resource_version: self.resource_version,
                        resource_uid: self.resource_uid,
                    })
                },
            )
            .await
            .map_err(|error| {
                ResourceYamlApplyFailure::Command(ResourceYamlApplyCommandFailed {
                    error: format!("{error:#?}"),
                    ..failure
                })
            })?
            .map_err(ResourceYamlApplyFailure::Api)
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }

    fn failure_if_abandoned(&self) -> Option<AbandonedFailure> {
        let failure = self.failure();
        abandoned_failure(move |error| {
            ResourceYamlApplyFailure::Command(ResourceYamlApplyCommandFailed { error, ..failure })
        })
    }
}

impl ValidateResourceYaml {
    fn failure(&self) -> ResourceYamlValidationCommandFailed {
        ResourceYamlValidationCommandFailed {
            editor_id: self.editor_id,
            revision: self.revision,
            error: String::new(),
        }
    }
}

#[async_trait]
impl WorkerCommand for ValidateResourceYaml {
    type Output = Result<ResourceYamlValidated, ResourceYamlValidationFailure>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let failure = self.failure();
        state
            .api_call(
                self.cluster_key,
                ApiCall::read("Validating the YAML"),
                |client| {
                    validate_resource_yaml(ResourceYamlValidationRequest {
                        editor_id: self.editor_id,
                        revision: self.revision,
                        cluster_key: self.cluster_key,
                        client,
                        api_resource: self.api_resource,
                        namespace: self.namespace,
                        resource_name: self.resource_name,
                        original_yaml: self.original_yaml,
                        yaml: self.yaml,
                        resource_version: self.resource_version,
                        resource_uid: self.resource_uid,
                    })
                },
            )
            .await
            .map_err(|error| {
                ResourceYamlValidationFailure::Command(ResourceYamlValidationCommandFailed {
                    error: format!("{error:#?}"),
                    ..failure
                })
            })?
            .map_err(ResourceYamlValidationFailure::Api)
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }

    fn failure_if_abandoned(&self) -> Option<AbandonedFailure> {
        let failure = self.failure();
        abandoned_failure(move |error| {
            ResourceYamlValidationFailure::Command(ResourceYamlValidationCommandFailed {
                error,
                ..failure
            })
        })
    }
}

impl UpdateResourceData {
    fn failure(&self) -> ResourceDataUpdateFailed {
        ResourceDataUpdateFailed {
            cluster_key: self.cluster_key,
            history_entry_id: self.history_entry_id,
            request_id: self.request_id,
            api_resource: self.api_resource.clone(),
            namespace: self.namespace.clone(),
            resource_name: self.resource_name.clone(),
            error: String::new(),
        }
    }
}

#[async_trait]
impl WorkerCommand for UpdateResourceData {
    type Output = Result<ResourceDataUpdateCompleted, ResourceDataUpdateFailed>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let failure = self.failure();
        state
            .api_call(
                self.cluster_key,
                ApiCall::mutation("Updating the data"),
                |client| {
                    update_resource_data(ResourceDataUpdateRequest {
                        cluster_key: self.cluster_key,
                        history_entry_id: self.history_entry_id,
                        request_id: self.request_id,
                        client,
                        api_resource: self.api_resource,
                        namespace: self.namespace,
                        resource_name: self.resource_name,
                        expected_values: &self.update.expected_values,
                        updated_values: &self.update.updated_values,
                        expected_resource_version: &self.update.expected_resource_version,
                    })
                },
            )
            .await
            .map_err(|error| ResourceDataUpdateFailed {
                error: format!("{error:#?}"),
                ..failure
            })
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }

    fn failure_if_abandoned(&self) -> Option<AbandonedFailure> {
        let failure = self.failure();
        abandoned_failure(move |error| ResourceDataUpdateFailed { error, ..failure })
    }
}
