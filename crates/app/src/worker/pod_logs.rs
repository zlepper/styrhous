//! Pod-log stream ingestion, including concurrent tail and history backfill.

use super::{WorkerResult, WorkerResultSender};
use crate::helpers::ResultExt;
use crate::log_store::LogStoreAppender;
use futures_util::{AsyncBufReadExt, TryStreamExt};
use k8s_openapi::api::core::v1::Pod;
use kube::api::{Api, LogParams};
use std::time::Duration;

pub(super) async fn stream(
    log_window_id: u64,
    client: kube::Client,
    namespace: String,
    pod_name: String,
    container: String,
    log_store_appender: LogStoreAppender,
    sender: WorkerResultSender,
) {
    let result = async {
        let pods: Api<Pod> = Api::namespaced(client, &namespace);
        let backfill_container = container.clone();
        let tail_stream = pods
            .log_stream(
                &pod_name,
                &LogParams {
                    container: Some(container),
                    follow: true,
                    tail_lines: Some(1_000),
                    timestamps: true,
                    ..LogParams::default()
                },
            )
            .await?;
        let backfill_pods = pods.clone();
        let backfill_pod_name = pod_name.clone();
        let backfill_appender = log_store_appender.clone();
        let live = append_stream(tail_stream, log_store_appender, log_window_id, false);
        let backfill = async move {
            let stream = backfill_pods
                .log_stream(
                    &backfill_pod_name,
                    &LogParams {
                        container: Some(backfill_container),
                        timestamps: true,
                        ..LogParams::default()
                    },
                )
                .await?;
            append_stream(stream, backfill_appender.clone(), log_window_id, true).await?;
            backfill_appender.complete_backfill(log_window_id).await
        };
        tokio::try_join!(live, backfill)?;
        anyhow::Ok(())
    }
    .await;
    let result = match result {
        Ok(()) => WorkerResult::PodLogStreamEnded { log_window_id },
        Err(error) => WorkerResult::PodLogStreamFailed {
            log_window_id,
            error: format!("{error:#}"),
        },
    };
    sender
        .send(result)
        .log_if_error("Failed to send Pod log stream result");
}

async fn append_stream(
    stream: impl futures_util::AsyncBufRead + Unpin,
    log_store_appender: LogStoreAppender,
    log_window_id: u64,
    backfill: bool,
) -> anyhow::Result<()> {
    let mut lines = stream.lines();
    let mut batch = Vec::new();
    let mut flush = tokio::time::interval(Duration::from_millis(100));
    flush.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            line = lines.try_next() => {
                let Some(line) = line? else { break };
                batch.push(line);
                if batch.len() >= 64 {
                    append_batch(&log_store_appender, log_window_id, std::mem::take(&mut batch), backfill).await?;
                }
            }
            _ = flush.tick(), if !batch.is_empty() => {
                append_batch(&log_store_appender, log_window_id, std::mem::take(&mut batch), backfill).await?;
            }
        }
    }
    if !batch.is_empty() {
        append_batch(&log_store_appender, log_window_id, batch, backfill).await?;
    }
    Ok(())
}

async fn append_batch(
    appender: &LogStoreAppender,
    log_window_id: u64,
    lines: Vec<String>,
    backfill: bool,
) -> anyhow::Result<()> {
    if backfill {
        appender.append_backfill(log_window_id, lines).await
    } else {
        appender.append(log_window_id, lines).await
    }
}
