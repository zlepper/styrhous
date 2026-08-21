use super::*;

#[async_trait]
impl WorkerCommand for StartPodLogStream {
    type Output = Result<PodLogStreamStarted, PodLogStreamFailed>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let failure = |error| PodLogStreamFailed {
            log_window_id: self.log_window_id,
            error: format!("{error:#?}"),
        };
        let client = state
            .client_for_cluster(self.cluster_key)
            .await
            .map_err(failure)?;
        let log_store_appender =
            state
                .log_store_appender
                .clone()
                .ok_or_else(|| PodLogStreamFailed {
                    log_window_id: self.log_window_id,
                    error: format!(
                        "Pod log storage is not initialized for cluster_key {}",
                        self.cluster_key
                    ),
                })?;
        let key = (self.cluster_key, self.log_window_id);
        let event_sender = state.results.clone();
        state
            .log_streams
            .replace_after_abort(key, move || {
                tokio::spawn(pod_logs::stream(
                    self.log_window_id,
                    client,
                    self.namespace,
                    self.pod_name,
                    self.container,
                    log_store_appender,
                    event_sender,
                ))
            })
            .await;
        Ok(PodLogStreamStarted {
            log_window_id: self.log_window_id,
        })
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}

#[async_trait]
impl WorkerCommand for StopPodLogStream {
    type Output = Result<PodLogStreamEnded, WorkerError>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        state
            .log_streams
            .abort(&(self.cluster_key, self.log_window_id))
            .await;
        Ok(PodLogStreamEnded {
            log_window_id: self.log_window_id,
        })
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}
