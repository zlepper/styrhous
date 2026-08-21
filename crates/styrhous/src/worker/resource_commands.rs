use super::*;

#[async_trait]
impl WorkerCommand for GetResourceYaml {
    type Output = Result<ResourceYamlFetched, ResourceYamlFetchFailed>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let editor_id = self.editor_id;
        match state.client_for_cluster(self.cluster_key).await {
            Ok(client) => get_resource_yaml(
                editor_id,
                self.cluster_key,
                client,
                self.api_resource,
                self.namespace,
                self.resource_name,
            )
            .await
            .map_err(|error| ResourceYamlFetchFailed {
                editor_id,
                error: format!("{error:#?}"),
            }),
            Err(error) => Err(ResourceYamlFetchFailed {
                editor_id,
                error: format!("{error:#?}"),
            }),
        }
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}

#[async_trait]
impl WorkerCommand for LoadResourceSchema {
    type Output = Result<ResourceSchemaLoaded, ResourceSchemaLoadFailed>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let editor_id = self.editor_id;
        match state.client_for_cluster(self.cluster_key).await {
            Ok(client) => {
                get_resource_schema(editor_id, self.cluster_key, client, self.api_resource)
                    .await
                    .map_err(|error| ResourceSchemaLoadFailed {
                        editor_id,
                        error: format!("{error:#?}"),
                    })
            }
            Err(error) => Err(ResourceSchemaLoadFailed {
                editor_id,
                error: format!("{error:#?}"),
            }),
        }
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}

#[async_trait]
impl WorkerCommand for DeleteResource {
    type Output = Result<ResourceDeleteCompleted, ResourceDeleteFailed>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let failure = ResourceDeleteFailed {
            cluster_key: self.cluster_key,
            api_resource: self.api_resource.clone(),
            namespace: self.namespace.clone(),
            resource_name: self.resource_name.clone(),
            bulk_delete_id: self.bulk_delete_id,
            error: String::new(),
        };
        match state.client_for_cluster(self.cluster_key).await {
            Ok(client) => delete_resource(
                self.cluster_key,
                client,
                self.api_resource,
                self.namespace,
                self.resource_name,
                self.resource_uid,
                self.bulk_delete_id,
            )
            .await
            .map_err(|error| ResourceDeleteFailed {
                error: format!("{error:#?}"),
                ..failure
            }),
            Err(error) => Err(ResourceDeleteFailed {
                error: format!("{error:#?}"),
                ..failure
            }),
        }
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}

#[async_trait]
impl WorkerCommand for ForceDeleteResource {
    type Output = Result<ResourceForceDeleteCompleted, ResourceForceDeleteFailed>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let cluster_key = self.cluster_key;
        match state.client_for_cluster(cluster_key).await {
            Ok(client) => force_delete_resource(
                cluster_key,
                client,
                self.api_resource,
                self.namespace,
                self.resource_name,
                self.resource_uid,
            )
            .await
            .map_err(|error| ResourceForceDeleteFailed {
                cluster_key,
                error: format!("{error:#?}"),
            }),
            Err(error) => Err(ResourceForceDeleteFailed {
                cluster_key,
                error: format!("{error:#?}"),
            }),
        }
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}

#[async_trait]
impl WorkerCommand for RestartDeployment {
    type Output = Result<DeploymentRestartCompleted, DeploymentRestartFailed>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let cluster_key = self.cluster_key;
        match state.client_for_cluster(cluster_key).await {
            Ok(client) => restart_deployment(client, self.namespace, self.resource_name)
                .await
                .map_err(|error| DeploymentRestartFailed {
                    cluster_key,
                    error: format!("{error:#?}"),
                }),
            Err(error) => Err(DeploymentRestartFailed {
                cluster_key,
                error: format!("{error:#?}"),
            }),
        }
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}

#[async_trait]
impl WorkerCommand for RunCronJob {
    type Output = Result<CronJobRunCompleted, CronJobRunFailed>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let cluster_key = self.cluster_key;
        match state.client_for_cluster(cluster_key).await {
            Ok(client) => run_cron_job(client, self.namespace, self.resource_name)
                .await
                .map_err(|error| CronJobRunFailed {
                    cluster_key,
                    error: format!("{error:#?}"),
                }),
            Err(error) => Err(CronJobRunFailed {
                cluster_key,
                error: format!("{error:#?}"),
            }),
        }
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}

#[async_trait]
impl WorkerCommand for GetResourceScale {
    type Output = Result<ResourceScaleFetched, ResourceScaleFailed>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let cluster_key = self.cluster_key;
        match state.client_for_cluster(cluster_key).await {
            Ok(client) => get_resource_scale(
                cluster_key,
                client,
                self.api_resource,
                self.namespace,
                self.resource_name,
            )
            .await
            .map_err(|error| ResourceScaleFailed {
                cluster_key,
                error: format!("{error:#?}"),
            }),
            Err(error) => Err(ResourceScaleFailed {
                cluster_key,
                error: format!("{error:#?}"),
            }),
        }
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}

#[async_trait]
impl WorkerCommand for UpdateResourceScale {
    type Output = Result<ResourceScaleUpdated, ResourceScaleFailed>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let cluster_key = self.cluster_key;
        match state.client_for_cluster(cluster_key).await {
            Ok(client) => update_resource_scale(
                cluster_key,
                client,
                self.api_resource,
                self.namespace,
                self.resource_name,
                self.replicas,
            )
            .await
            .map_err(|error| ResourceScaleFailed {
                cluster_key,
                error: format!("{error:#?}"),
            }),
            Err(error) => Err(ResourceScaleFailed {
                cluster_key,
                error: format!("{error:#?}"),
            }),
        }
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}
