use super::*;

impl GetResourceYaml {
    fn failure(&self) -> ResourceYamlFetchFailed {
        ResourceYamlFetchFailed {
            editor_id: self.editor_id,
            error: String::new(),
        }
    }
}

#[async_trait]
impl WorkerCommand for GetResourceYaml {
    type Output = Result<ResourceYamlFetched, ResourceYamlFetchFailed>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let failure = self.failure();
        state
            .api_call(self.cluster_key, ApiCall::read("Getting YAML"), |client| {
                get_resource_yaml(
                    self.editor_id,
                    self.cluster_key,
                    client,
                    self.api_resource,
                    self.namespace,
                    self.resource_name,
                )
            })
            .await
            .map_err(|error| ResourceYamlFetchFailed {
                error: format!("{error:#?}"),
                ..failure
            })
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }

    fn failure_if_abandoned(&self) -> Option<AbandonedFailure> {
        let failure = self.failure();
        abandoned_failure(move |error| ResourceYamlFetchFailed { error, ..failure })
    }
}

impl LoadResourceSchema {
    fn failure(&self) -> ResourceSchemaLoadFailed {
        ResourceSchemaLoadFailed {
            editor_id: self.editor_id,
            error: String::new(),
        }
    }
}

#[async_trait]
impl WorkerCommand for LoadResourceSchema {
    type Output = Result<ResourceSchemaLoaded, ResourceSchemaLoadFailed>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let failure = self.failure();
        // OpenAPI documents run to megabytes, so this call gets more patience than the rest.
        state
            .api_call(
                self.cluster_key,
                ApiCall::read("Loading the schema").patient(),
                |client| {
                    get_resource_schema(self.editor_id, self.cluster_key, client, self.api_resource)
                },
            )
            .await
            .map_err(|error| ResourceSchemaLoadFailed {
                error: format!("{error:#?}"),
                ..failure
            })
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }

    fn failure_if_abandoned(&self) -> Option<AbandonedFailure> {
        let failure = self.failure();
        abandoned_failure(move |error| ResourceSchemaLoadFailed { error, ..failure })
    }
}

impl DeleteResource {
    fn failure(&self) -> ResourceDeleteFailed {
        ResourceDeleteFailed {
            cluster_key: self.cluster_key,
            api_resource: self.api_resource.clone(),
            namespace: self.namespace.clone(),
            resource_name: self.resource_name.clone(),
            bulk_delete_id: self.bulk_delete_id,
            error: String::new(),
        }
    }
}

#[async_trait]
impl WorkerCommand for DeleteResource {
    type Output = Result<ResourceDeleteCompleted, ResourceDeleteFailed>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let failure = self.failure();
        state
            .api_call(
                self.cluster_key,
                ApiCall::mutation("Deleting the resource"),
                |client| {
                    delete_resource(
                        self.cluster_key,
                        client,
                        self.api_resource,
                        self.namespace,
                        self.resource_name,
                        self.resource_uid,
                        self.bulk_delete_id,
                    )
                },
            )
            .await
            .map_err(|error| ResourceDeleteFailed {
                error: format!("{error:#?}"),
                ..failure
            })
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }

    fn failure_if_abandoned(&self) -> Option<AbandonedFailure> {
        let failure = self.failure();
        abandoned_failure(move |error| ResourceDeleteFailed { error, ..failure })
    }
}

impl ForceDeleteResource {
    fn failure(&self) -> ResourceForceDeleteFailed {
        ResourceForceDeleteFailed {
            cluster_key: self.cluster_key,
            api_resource: self.api_resource.clone(),
            namespace: self.namespace.clone(),
            resource_name: self.resource_name.clone(),
            error: String::new(),
        }
    }
}

#[async_trait]
impl WorkerCommand for ForceDeleteResource {
    type Output = Result<ResourceForceDeleteCompleted, ResourceForceDeleteFailed>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let failure = self.failure();
        state
            .api_call(
                self.cluster_key,
                ApiCall::mutation("Force deleting the resource"),
                |client| {
                    force_delete_resource(
                        self.cluster_key,
                        client,
                        self.api_resource,
                        self.namespace,
                        self.resource_name,
                        self.resource_uid,
                    )
                },
            )
            .await
            .map_err(|error| ResourceForceDeleteFailed {
                error: format!("{error:#?}"),
                ..failure
            })
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }

    fn failure_if_abandoned(&self) -> Option<AbandonedFailure> {
        let failure = self.failure();
        abandoned_failure(move |error| ResourceForceDeleteFailed { error, ..failure })
    }
}

impl RestartDeployment {
    fn failure(&self) -> DeploymentRestartFailed {
        DeploymentRestartFailed {
            cluster_key: self.cluster_key,
            namespace: self.namespace.clone(),
            resource_name: self.resource_name.clone(),
            error: String::new(),
        }
    }
}

#[async_trait]
impl WorkerCommand for RestartDeployment {
    type Output = Result<DeploymentRestartCompleted, DeploymentRestartFailed>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let failure = self.failure();
        state
            .api_call(
                self.cluster_key,
                ApiCall::mutation("Restarting the Deployment"),
                |client| {
                    restart_deployment(self.cluster_key, client, self.namespace, self.resource_name)
                },
            )
            .await
            .map_err(|error| DeploymentRestartFailed {
                error: format!("{error:#?}"),
                ..failure
            })
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }

    fn failure_if_abandoned(&self) -> Option<AbandonedFailure> {
        let failure = self.failure();
        abandoned_failure(move |error| DeploymentRestartFailed { error, ..failure })
    }
}

impl RunCronJob {
    fn failure(&self) -> CronJobRunFailed {
        CronJobRunFailed {
            cluster_key: self.cluster_key,
            operation_id: self.operation_id,
            namespace: self.namespace.clone(),
            cron_job_name: self.resource_name.clone(),
            error: String::new(),
        }
    }
}

#[async_trait]
impl WorkerCommand for RunCronJob {
    type Output = Result<CronJobRunCompleted, CronJobRunFailed>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let failure = self.failure();
        state
            .api_call(
                self.cluster_key,
                ApiCall::mutation("Running the CronJob"),
                |client| async move {
                    run_cron_job(client, &self.namespace, &self.resource_name)
                        .await
                        .map(|job_name| CronJobRunCompleted {
                            cluster_key: self.cluster_key,
                            operation_id: self.operation_id,
                            namespace: self.namespace,
                            cron_job_name: self.resource_name,
                            job_name,
                        })
                },
            )
            .await
            .map_err(|error| CronJobRunFailed {
                error: format!("{error:#?}"),
                ..failure
            })
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }

    fn failure_if_abandoned(&self) -> Option<AbandonedFailure> {
        let failure = self.failure();
        abandoned_failure(move |error| CronJobRunFailed { error, ..failure })
    }
}

impl GetDeploymentRevisions {
    fn failure(&self) -> DeploymentRevisionsFetchFailed {
        DeploymentRevisionsFetchFailed {
            cluster_key: self.cluster_key,
            request_id: self.request_id,
            error: String::new(),
        }
    }
}

#[async_trait]
impl WorkerCommand for GetDeploymentRevisions {
    type Output = Result<DeploymentRevisionsFetched, DeploymentRevisionsFetchFailed>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let failure = self.failure();
        state
            .api_call(
                self.cluster_key,
                ApiCall::read("Loading the Deployment revisions"),
                |client| deployment_revisions(client, &self.namespace, &self.resource_name),
            )
            .await
            .map(|history| DeploymentRevisionsFetched {
                cluster_key: self.cluster_key,
                request_id: self.request_id,
                history,
            })
            .map_err(|error| DeploymentRevisionsFetchFailed {
                error: format!("{error:#}"),
                ..failure
            })
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }

    fn failure_if_abandoned(&self) -> Option<AbandonedFailure> {
        let failure = self.failure();
        abandoned_failure(move |error| DeploymentRevisionsFetchFailed { error, ..failure })
    }
}

impl RollbackDeployment {
    fn failure(&self) -> DeploymentRollbackFailed {
        DeploymentRollbackFailed {
            cluster_key: self.cluster_key,
            operation_id: self.operation_id,
            revision: self.revision,
            error: String::new(),
        }
    }
}

#[async_trait]
impl WorkerCommand for RollbackDeployment {
    type Output = Result<DeploymentRolledBack, DeploymentRollbackFailed>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let failure = self.failure();
        state
            .api_call(
                self.cluster_key,
                ApiCall::mutation("Rolling back the Deployment"),
                |client| {
                    rollback_deployment(client, &self.namespace, &self.resource_name, self.revision)
                },
            )
            .await
            .map(|outcome| DeploymentRolledBack {
                cluster_key: self.cluster_key,
                operation_id: self.operation_id,
                revision: self.revision,
                outcome,
            })
            .map_err(|error| DeploymentRollbackFailed {
                error: format!("{error:#}"),
                ..failure
            })
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }

    fn failure_if_abandoned(&self) -> Option<AbandonedFailure> {
        let failure = self.failure();
        abandoned_failure(move |error| DeploymentRollbackFailed { error, ..failure })
    }
}

#[async_trait]
impl WorkerCommand for RollbackHelmRelease {
    type Output = Result<HelmRollbackCompleted, HelmRollbackFailed>;

    async fn execute(self, _state: &WorkerState) -> Self::Output {
        let result = rollback_helm_release(&self.target).await;
        let HelmRollbackTarget {
            release, revision, ..
        } = self.target;
        match result {
            Ok(()) => Ok(HelmRollbackCompleted {
                cluster_key: self.cluster_key,
                operation_id: self.operation_id,
                release,
                revision,
            }),
            Err(error) => Err(HelmRollbackFailed {
                cluster_key: self.cluster_key,
                operation_id: self.operation_id,
                release,
                revision,
                helm_missing: is_cli_not_installed(&error),
                error: format!("{error:#}"),
            }),
        }
    }

    // Deliberately not routed to the cluster's runtime: `helm` does not use the
    // session's client, and a rollback can run for minutes, which would stall a
    // reconnect that drains that runtime's in-flight commands. Operation ids
    // stay unique across sessions, so a late result cannot settle a new one.
}

impl GetResourceScale {
    fn failure(&self) -> ResourceScaleFetchFailed {
        ResourceScaleFetchFailed {
            cluster_key: self.cluster_key,
            request_id: self.request_id,
            api_resource: self.api_resource.clone(),
            namespace: self.namespace.clone(),
            resource_name: self.resource_name.clone(),
            error: String::new(),
        }
    }
}

#[async_trait]
impl WorkerCommand for GetResourceScale {
    type Output = Result<ResourceScaleFetched, ResourceScaleFetchFailed>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let failure = self.failure();
        state
            .api_call(
                self.cluster_key,
                ApiCall::read("Getting the scale"),
                |client| {
                    get_resource_scale(
                        self.cluster_key,
                        self.request_id,
                        client,
                        self.api_resource,
                        self.namespace,
                        self.resource_name,
                    )
                },
            )
            .await
            .map_err(|error| ResourceScaleFetchFailed {
                error: format!("{error:#?}"),
                ..failure
            })
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }

    fn failure_if_abandoned(&self) -> Option<AbandonedFailure> {
        let failure = self.failure();
        abandoned_failure(move |error| ResourceScaleFetchFailed { error, ..failure })
    }
}

impl UpdateResourceScale {
    fn failure(&self) -> ResourceScaleUpdateFailed {
        ResourceScaleUpdateFailed {
            cluster_key: self.cluster_key,
            api_resource: self.api_resource.clone(),
            namespace: self.namespace.clone(),
            resource_name: self.resource_name.clone(),
            error: String::new(),
        }
    }
}

#[async_trait]
impl WorkerCommand for UpdateResourceScale {
    type Output = Result<ResourceScaleUpdated, ResourceScaleUpdateFailed>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let failure = self.failure();
        state
            .api_call(
                self.cluster_key,
                ApiCall::mutation("Updating the scale"),
                |client| {
                    update_resource_scale(
                        self.cluster_key,
                        client,
                        self.api_resource,
                        self.namespace,
                        self.resource_name,
                        self.replicas,
                    )
                },
            )
            .await
            .map_err(|error| ResourceScaleUpdateFailed {
                error: format!("{error:#?}"),
                ..failure
            })
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }

    fn failure_if_abandoned(&self) -> Option<AbandonedFailure> {
        let failure = self.failure();
        abandoned_failure(move |error| ResourceScaleUpdateFailed { error, ..failure })
    }
}
