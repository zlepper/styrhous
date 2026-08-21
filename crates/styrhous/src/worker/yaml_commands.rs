use super::*;

#[async_trait]
impl WorkerCommand for ApplyResourceYaml {
    type Output = Result<ResourceApplyCompleted, ResourceYamlApplyFailure>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let editor_id = self.editor_id;
        match state.client_for_cluster(self.cluster_key).await {
            Ok(client) => match apply_resource_yaml(
                editor_id,
                self.cluster_key,
                client,
                self.api_resource,
                self.namespace,
                self.resource_name,
                self.yaml,
            )
            .await
            {
                Ok(result) => result.map_err(ResourceYamlApplyFailure::Api),
                Err(error) => Err(ResourceYamlApplyFailure::Command(
                    ResourceYamlApplyCommandFailed {
                        editor_id,
                        error: format!("{error:#?}"),
                    },
                )),
            },
            Err(error) => Err(ResourceYamlApplyFailure::Command(
                ResourceYamlApplyCommandFailed {
                    editor_id,
                    error: format!("{error:#?}"),
                },
            )),
        }
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}

#[async_trait]
impl WorkerCommand for ValidateResourceYaml {
    type Output = Result<ResourceYamlValidated, ResourceYamlValidationFailure>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let editor_id = self.editor_id;
        let revision = self.revision;
        match state.client_for_cluster(self.cluster_key).await {
            Ok(client) => match validate_resource_yaml(ResourceYamlValidationRequest {
                editor_id,
                revision,
                cluster_key: self.cluster_key,
                client,
                api_resource: self.api_resource,
                namespace: self.namespace,
                resource_name: self.resource_name,
                yaml: self.yaml,
            })
            .await
            {
                Ok(result) => result.map_err(ResourceYamlValidationFailure::Api),
                Err(error) => Err(ResourceYamlValidationFailure::Command(
                    ResourceYamlValidationCommandFailed {
                        editor_id,
                        revision,
                        error: format!("{error:#?}"),
                    },
                )),
            },
            Err(error) => Err(ResourceYamlValidationFailure::Command(
                ResourceYamlValidationCommandFailed {
                    editor_id,
                    revision,
                    error: format!("{error:#?}"),
                },
            )),
        }
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}

#[async_trait]
impl WorkerCommand for UpdateResourceData {
    type Output = Result<ResourceDataUpdateCompleted, ResourceDataUpdateFailed>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let failure = ResourceDataUpdateFailed {
            cluster_key: self.cluster_key,
            history_entry_id: self.history_entry_id,
            request_id: self.request_id,
            error: String::new(),
        };
        match state.client_for_cluster(self.cluster_key).await {
            Ok(client) => update_resource_data(ResourceDataUpdateRequest {
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
            .await
            .map_err(|error| ResourceDataUpdateFailed {
                error: format!("{error:#?}"),
                ..failure
            }),
            Err(error) => Err(ResourceDataUpdateFailed {
                error: format!("{error:#?}"),
                ..failure
            }),
        }
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}
