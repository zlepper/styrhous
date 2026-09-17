//! Pod-log stream ingestion, including concurrent tail and history backfill.

use super::{
    PodLogSourceFailed, PodLogSourceReconnecting, PodLogSourceRecovered, PodLogStreamEnded,
    PodLogStreamFailed, PodLogStreamTarget, WorkerResultSender,
};
use crate::helpers::ResultExt;
use crate::log_store::{LogRecord, LogStoreAppender, read_log_record, write_log_record};
use futures_util::{AsyncBufReadExt, StreamExt, TryStreamExt};
use k8s_openapi::api::core::v1::Pod;
use kube::api::{Api, LogParams};
use kube::runtime::utils::Backoff;
use kube::runtime::watcher::DefaultBackoff;
use std::cmp::Ordering as CmpOrdering;
use std::collections::BinaryHeap;
use std::fmt;
use std::io::{BufReader, Read, Seek, SeekFrom, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use tempfile::NamedTempFile;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};
use tokio::time::Instant;

#[derive(Clone)]
struct StreamTargetContext {
    log_window_id: u64,
    ingest: Arc<LogTimelineAppender>,
    completed_backfills: Arc<AtomicUsize>,
    target_count: usize,
    sender: WorkerResultSender,
}

struct LiveLogFollower {
    log_pods: Api<Pod>,
    status_pods: Api<Pod>,
    target: PodLogStreamTarget,
    source_index: usize,
    resume: Arc<StdMutex<FollowResumeState>>,
    context: StreamTargetContext,
}

struct LogTimelineAppender {
    appender: LogStoreAppender,
    log_window_id: u64,
    live: Mutex<Vec<PendingLiveBatch>>,
    live_capacity: Arc<Semaphore>,
    history: std::sync::Mutex<Vec<LogHistorySource>>,
}

impl LogTimelineAppender {
    fn new(
        appender: LogStoreAppender,
        log_window_id: u64,
        source_count: usize,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            appender,
            log_window_id,
            live: Mutex::new(Vec::new()),
            live_capacity: Arc::new(Semaphore::new(MAX_PENDING_LIVE_ROWS)),
            history: std::sync::Mutex::new(
                (0..source_count)
                    .map(|_| LogHistorySource::new())
                    .collect::<anyhow::Result<_>>()?,
            ),
        })
    }

    async fn append(
        &self,
        source_index: usize,
        records: Vec<LogRecord>,
        backfill: bool,
    ) -> anyhow::Result<()> {
        if backfill {
            // Each Kubernetes history response is chronological for its
            // container. Spool sources independently, then perform a bounded
            // k-way merge once every source has finished. This keeps the
            // aggregate history disk-backed regardless of its total size.
            let mut history = self
                .history
                .lock()
                .map_err(|_| anyhow::anyhow!("Log timeline history lock was poisoned"))?;
            return history
                .get_mut(source_index)
                .ok_or_else(|| anyhow::anyhow!("Unknown log timeline source"))?
                .history
                .append(records);
        }

        let record_count = u32::try_from(records.len())
            .map_err(|_| anyhow::anyhow!("A Pod log batch exceeds the live ordering capacity"))?;
        let permit = self
            .live_capacity
            .clone()
            .acquire_many_owned(record_count)
            .await
            .map_err(|_| anyhow::anyhow!("The live Pod log ordering buffer was closed"))?;
        self.live.lock().await.push(PendingLiveBatch {
            records,
            received_at: Instant::now(),
            _permit: permit,
        });
        Ok(())
    }

    async fn flush_live(&self) -> anyhow::Result<()> {
        let cutoff = Instant::now() - LIVE_ORDERING_WINDOW;
        self.flush_live_before(Some(cutoff)).await
    }

    async fn flush_all_live(&self) -> anyhow::Result<()> {
        self.flush_live_before(None).await
    }

    async fn flush_live_before(&self, cutoff: Option<Instant>) -> anyhow::Result<()> {
        let mut pending = self.live.lock().await;
        let mut records = Vec::new();
        let mut permits = Vec::new();
        let mut retained = Vec::with_capacity(pending.len());
        for pending_batch in std::mem::take(&mut *pending) {
            if cutoff.is_none_or(|cutoff| pending_batch.received_at <= cutoff) {
                records.extend(pending_batch.records);
                permits.push(pending_batch._permit);
            } else {
                retained.push(pending_batch);
            }
        }
        *pending = retained;
        drop(pending);
        if records.is_empty() {
            return Ok(());
        }
        records.sort_by(log_timestamp_order);
        let result = self.appender.append(self.log_window_id, records).await;
        drop(permits);
        result
    }

    async fn complete_backfill(&self) -> anyhow::Result<()> {
        let sources = std::mem::take(
            &mut *self
                .history
                .lock()
                .map_err(|_| anyhow::anyhow!("Log timeline history lock was poisoned"))?,
        );
        merge_history_spools(&self.appender, self.log_window_id, sources).await
    }

    async fn finish_backfill(&self) -> anyhow::Result<()> {
        self.complete_backfill().await?;
        self.appender.complete_backfill(self.log_window_id).await
    }

    fn capture_live_prefix(&self, source_index: usize, record: &LogRecord) -> anyhow::Result<()> {
        let mut history = self
            .history
            .lock()
            .map_err(|_| anyhow::anyhow!("Log timeline history lock was poisoned"))?;
        // The per-source spools move into the disk merge after all histories
        // finish. Follow streams intentionally continue afterwards, but no
        // longer need boundary capture.
        if history.is_empty() {
            return Ok(());
        }
        let source = history
            .get_mut(source_index)
            .ok_or_else(|| anyhow::anyhow!("Unknown log timeline source"))?;
        if !source.backfill_complete {
            source.live_prefix.append(std::iter::once(record.clone()))?;
        }
        Ok(())
    }

    fn complete_source_backfill(&self, source_index: usize) -> anyhow::Result<()> {
        let mut history = self
            .history
            .lock()
            .map_err(|_| anyhow::anyhow!("Log timeline history lock was poisoned"))?;
        history
            .get_mut(source_index)
            .ok_or_else(|| anyhow::anyhow!("Unknown log timeline source"))?
            .backfill_complete = true;
        Ok(())
    }
}

const LIVE_ORDERING_WINDOW: Duration = Duration::from_millis(100);
const LIVE_FLUSH_INTERVAL: Duration = Duration::from_millis(25);
const MAX_PENDING_LIVE_ROWS: usize = 4_096;
const HISTORY_MERGE_BATCH_SIZE: usize = 512;

struct PendingLiveBatch {
    records: Vec<LogRecord>,
    received_at: Instant,
    _permit: OwnedSemaphorePermit,
}

struct HistorySpool {
    data: NamedTempFile,
    offsets: NamedTempFile,
    total_lines: usize,
}

struct LogHistorySource {
    history: HistorySpool,
    live_prefix: HistorySpool,
    backfill_complete: bool,
}

impl LogHistorySource {
    fn new() -> anyhow::Result<Self> {
        Ok(Self {
            history: HistorySpool::new()?,
            live_prefix: HistorySpool::new()?,
            backfill_complete: false,
        })
    }
}

impl HistorySpool {
    fn new() -> anyhow::Result<Self> {
        Ok(Self {
            data: NamedTempFile::new()?,
            offsets: NamedTempFile::new()?,
            total_lines: 0,
        })
    }

    fn append(&mut self, records: impl IntoIterator<Item = LogRecord>) -> anyhow::Result<()> {
        let data = self.data.as_file_mut();
        let offsets = self.offsets.as_file_mut();
        let mut next_offset = data.seek(SeekFrom::End(0))?;
        for record in records {
            offsets.write_all(&next_offset.to_le_bytes())?;
            next_offset += write_log_record(data, &record)?;
            self.total_lines += 1;
        }
        data.flush()?;
        offsets.flush()?;
        Ok(())
    }

    fn into_reader(self, line_limit: usize) -> anyhow::Result<HistoryReader> {
        Ok(HistoryReader {
            data: BufReader::new(self.data.reopen()?),
            remaining: line_limit,
        })
    }

    fn overlap_with_live_prefix(&self, live_prefix: &Self) -> anyhow::Result<usize> {
        let max_overlap = self.total_lines.min(live_prefix.total_lines);
        for overlap in (1..=max_overlap).rev() {
            let history_start = self.total_lines - overlap;
            let mut matches = true;
            for offset in 0..overlap {
                if self.read_record(history_start + offset)? != live_prefix.read_record(offset)? {
                    matches = false;
                    break;
                }
            }
            if matches {
                return Ok(overlap);
            }
        }
        Ok(0)
    }

    fn read_record(&self, line_index: usize) -> anyhow::Result<LogRecord> {
        let mut offsets = self.offsets.reopen()?;
        offsets.seek(SeekFrom::Start(
            (line_index * std::mem::size_of::<u64>()) as u64,
        ))?;
        let mut offset = [0; 8];
        offsets.read_exact(&mut offset)?;
        let mut data = self.data.reopen()?;
        data.seek(SeekFrom::Start(u64::from_le_bytes(offset)))?;
        read_log_record(&mut BufReader::new(data))?
            .ok_or_else(|| anyhow::anyhow!("Missing log record in history spool"))
    }
}

struct HistoryReader {
    data: BufReader<std::fs::File>,
    remaining: usize,
}

struct HistoryMergeEntry {
    record: LogRecord,
    source_index: usize,
}

impl Ord for HistoryMergeEntry {
    fn cmp(&self, other: &Self) -> CmpOrdering {
        // BinaryHeap is a max heap, so reverse the chronological comparison.
        log_timestamp_order(&other.record, &self.record)
            .then_with(|| other.source_index.cmp(&self.source_index))
    }
}

impl PartialOrd for HistoryMergeEntry {
    fn partial_cmp(&self, other: &Self) -> Option<CmpOrdering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for HistoryMergeEntry {
    fn eq(&self, other: &Self) -> bool {
        self.source_index == other.source_index && self.record == other.record
    }
}

impl Eq for HistoryMergeEntry {}

async fn merge_history_spools(
    appender: &LogStoreAppender,
    log_window_id: u64,
    sources: Vec<LogHistorySource>,
) -> anyhow::Result<()> {
    let mut readers = sources
        .into_iter()
        .map(|source| {
            let overlap = source
                .history
                .overlap_with_live_prefix(&source.live_prefix)?;
            let line_limit = source.history.total_lines - overlap;
            source.history.into_reader(line_limit)
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let mut pending = BinaryHeap::new();
    for (source_index, reader) in readers.iter_mut().enumerate() {
        if let Some(record) = read_next_spooled_record(reader)? {
            pending.push(HistoryMergeEntry {
                record,
                source_index,
            });
        }
    }

    let mut batch = Vec::with_capacity(HISTORY_MERGE_BATCH_SIZE);
    while let Some(record) = next_history_merge_record(&mut readers, &mut pending)? {
        batch.push(record);
        if batch.len() == HISTORY_MERGE_BATCH_SIZE {
            appender
                .append_backfill(log_window_id, std::mem::take(&mut batch))
                .await?;
        }
    }
    if !batch.is_empty() {
        appender.append_backfill(log_window_id, batch).await?;
    }
    Ok(())
}

fn next_history_merge_record(
    readers: &mut [HistoryReader],
    pending: &mut BinaryHeap<HistoryMergeEntry>,
) -> anyhow::Result<Option<LogRecord>> {
    let Some(HistoryMergeEntry {
        record,
        source_index,
    }) = pending.pop()
    else {
        return Ok(None);
    };
    if let Some(next_record) = read_next_spooled_record(&mut readers[source_index])? {
        pending.push(HistoryMergeEntry {
            record: next_record,
            source_index,
        });
    }
    Ok(Some(record))
}

fn read_next_spooled_record(reader: &mut HistoryReader) -> anyhow::Result<Option<LogRecord>> {
    if reader.remaining == 0 {
        return Ok(None);
    }
    reader.remaining -= 1;
    read_log_record(&mut reader.data)
}

fn log_timestamp_order(left: &LogRecord, right: &LogRecord) -> std::cmp::Ordering {
    let left_timestamp = log_record_timestamp(left);
    let right_timestamp = log_record_timestamp(right);
    match (left_timestamp, right_timestamp) {
        (Some(left_timestamp), Some(right_timestamp)) => left_timestamp
            .cmp(&right_timestamp)
            .then_with(|| left.source.cmp(&right.source)),
        (None, None) => std::cmp::Ordering::Equal,
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
    }
}

fn log_record_timestamp(record: &LogRecord) -> Option<OffsetDateTime> {
    crate::ansi::parse_kubernetes_log_line(&record.text)
        .timestamp
        .and_then(|timestamp| OffsetDateTime::parse(&timestamp, &Rfc3339).ok())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ResumeBoundary {
    timestamp: OffsetDateTime,
    delivered_at_timestamp: Vec<LogRecord>,
}

#[derive(Default)]
struct ResumeCursor {
    boundary: Option<ResumeBoundary>,
}

impl ResumeCursor {
    fn observe(&mut self, record: &LogRecord) {
        let Some(timestamp) = log_record_timestamp(record) else {
            return;
        };
        match self.boundary.as_mut() {
            Some(boundary) if timestamp < boundary.timestamp => {}
            Some(boundary) if timestamp == boundary.timestamp => {
                boundary.delivered_at_timestamp.push(record.clone());
            }
            _ => {
                self.boundary = Some(ResumeBoundary {
                    timestamp,
                    delivered_at_timestamp: vec![record.clone()],
                });
            }
        }
    }
}

fn resume_filter(live: &ResumeCursor, history: &ResumeCursor) -> ReplayFilter {
    match (live.boundary.as_ref(), history.boundary.as_ref()) {
        (Some(live), Some(history)) if live.timestamp > history.timestamp => boundary_filter(live),
        (Some(live), Some(history)) if history.timestamp > live.timestamp => {
            boundary_filter(history)
        }
        (Some(live), Some(history)) => {
            let mut delivered = live.delivered_at_timestamp.clone();
            for (index, record) in history.delivered_at_timestamp.iter().enumerate() {
                let history_count = history.delivered_at_timestamp[..=index]
                    .iter()
                    .filter(|candidate| *candidate == record)
                    .count();
                let delivered_count = delivered
                    .iter()
                    .filter(|candidate| *candidate == record)
                    .count();
                if history_count > delivered_count {
                    delivered.push(record.clone());
                }
            }
            ReplayFilter {
                boundary: Some(live.timestamp),
                remaining_at_boundary: delivered,
            }
        }
        (Some(boundary), None) | (None, Some(boundary)) => boundary_filter(boundary),
        (None, None) => ReplayFilter {
            boundary: None,
            remaining_at_boundary: Vec::new(),
        },
    }
}

fn boundary_filter(boundary: &ResumeBoundary) -> ReplayFilter {
    ReplayFilter {
        boundary: Some(boundary.timestamp),
        remaining_at_boundary: boundary.delivered_at_timestamp.clone(),
    }
}

struct ReplayFilter {
    boundary: Option<OffsetDateTime>,
    remaining_at_boundary: Vec<LogRecord>,
}

struct FollowResumeState {
    live: ResumeCursor,
    history: ResumeCursor,
    live_accepted_records: usize,
    started_at: OffsetDateTime,
}

#[derive(Debug)]
enum AppendStreamError {
    Read(std::io::Error),
    Ingest(anyhow::Error),
}

impl fmt::Display for AppendStreamError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(error) => write!(formatter, "{error}"),
            Self::Ingest(error) => write!(formatter, "{error:#}"),
        }
    }
}

impl std::error::Error for AppendStreamError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read(error) => Some(error),
            Self::Ingest(error) => error.source(),
        }
    }
}

impl FollowResumeState {
    fn new() -> Self {
        Self {
            live: ResumeCursor::default(),
            history: ResumeCursor::default(),
            live_accepted_records: 0,
            started_at: OffsetDateTime::now_utc(),
        }
    }

    fn observe(&mut self, record: &LogRecord, backfill: bool) {
        if backfill {
            self.history.observe(record);
        } else {
            self.live.observe(record);
            self.live_accepted_records += 1;
        }
    }

    fn reconnect(&self) -> anyhow::Result<(k8s_openapi::jiff::Timestamp, ReplayFilter)> {
        // Kubernetes accepts sinceTime at second precision on every supported
        // server. Request one second of overlap, then discard exactly the
        // records already delivered at the timestamp boundary.
        let replay_filter = resume_filter(&self.live, &self.history);
        let since = replay_filter.boundary.unwrap_or(self.started_at) - time::Duration::SECOND;
        let since = since
            .format(&Rfc3339)?
            .parse::<k8s_openapi::jiff::Timestamp>()?;
        Ok((since, replay_filter))
    }
}

impl ReplayFilter {
    fn accepts(&mut self, record: &LogRecord) -> bool {
        let Some(boundary) = self.boundary else {
            return true;
        };
        let Some(timestamp) = log_record_timestamp(record) else {
            return true;
        };
        if timestamp < boundary {
            return false;
        }
        if timestamp == boundary
            && let Some(position) = self
                .remaining_at_boundary
                .iter()
                .position(|delivered| delivered == record)
        {
            self.remaining_at_boundary.swap_remove(position);
            return false;
        }
        true
    }
}

fn target_may_produce_more_logs(pod: &Pod, target: &PodLogStreamTarget) -> bool {
    let pod_phase_is_terminal = pod
        .status
        .as_ref()
        .and_then(|status| status.phase.as_deref())
        .is_some_and(|phase| matches!(phase, "Succeeded" | "Failed"));
    if pod_phase_is_terminal {
        return false;
    }
    let statuses = pod.status.as_ref().and_then(|status| match target.kind {
        crate::resource_table::ContainerKind::Init => status.init_container_statuses.as_deref(),
        crate::resource_table::ContainerKind::App => status.container_statuses.as_deref(),
        crate::resource_table::ContainerKind::Ephemeral => {
            status.ephemeral_container_statuses.as_deref()
        }
    });
    let Some(status) = statuses.and_then(|statuses| {
        statuses
            .iter()
            .find(|status| status.name == target.container)
    }) else {
        return true;
    };
    let Some(state) = &status.state else {
        return true;
    };
    if state.running.is_some() || state.waiting.is_some() {
        return true;
    }
    let Some(terminated) = &state.terminated else {
        return true;
    };
    match target.kind {
        crate::resource_table::ContainerKind::Ephemeral => false,
        crate::resource_table::ContainerKind::Init => {
            pod.spec
                .as_ref()
                .and_then(|spec| spec.init_containers.as_deref())
                .and_then(|containers| {
                    containers
                        .iter()
                        .find(|container| container.name == target.container)
                })
                .and_then(|container| container.restart_policy.as_deref())
                == Some("Always")
        }
        crate::resource_table::ContainerKind::App => {
            let restart_policy = pod
                .spec
                .as_ref()
                .and_then(|spec| spec.restart_policy.as_deref())
                .unwrap_or("Always");
            restart_policy == "Always"
                || (restart_policy == "OnFailure" && terminated.exit_code != 0)
        }
    }
}

pub(super) async fn stream(
    log_window_id: u64,
    log_client: kube::Client,
    status_client: kube::Client,
    targets: Vec<PodLogStreamTarget>,
    log_store_appender: LogStoreAppender,
    sender: WorkerResultSender,
) {
    let target_count = targets.len();
    let ingest = match LogTimelineAppender::new(log_store_appender, log_window_id, target_count) {
        Ok(appender) => Arc::new(appender),
        Err(error) => {
            sender
                .send(PodLogStreamFailed {
                    log_window_id,
                    error: format!("Could not initialize log timeline storage: {error:#}"),
                })
                .await
                .log_if_error("Failed to send Pod log stream failure");
            return;
        }
    };
    let completed_backfills = Arc::new(AtomicUsize::new(0));
    let context = StreamTargetContext {
        log_window_id,
        ingest,
        completed_backfills,
        target_count,
        sender: sender.clone(),
    };
    let target_results =
        futures_util::stream::iter(targets)
            .enumerate()
            .map(|(source_index, target)| {
                let log_client = log_client.clone();
                let status_client = status_client.clone();
                let context = context.clone();
                async move {
                    stream_target(log_client, status_client, target, source_index, context).await
                }
            })
            .buffer_unordered(target_count.max(1))
            .collect::<Vec<_>>();
    tokio::pin!(target_results);
    let mut live_flush = tokio::time::interval(LIVE_FLUSH_INTERVAL);
    live_flush.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let results = loop {
        tokio::select! {
            results = &mut target_results => {
                if let Err(error) = context.ingest.flush_all_live().await {
                    sender
                        .send(PodLogStreamFailed {
                            log_window_id,
                            error: format!("Could not flush the Pod log ordering buffer: {error:#}"),
                        })
                        .await
                        .log_if_error("Failed to send Pod log stream failure");
                    return;
                }
                break results;
            }
            _ = live_flush.tick() => {
                if let Err(error) = context.ingest.flush_live().await {
                    sender
                        .send(PodLogStreamFailed {
                            log_window_id,
                            error: format!("Could not flush the Pod log ordering buffer: {error:#}"),
                        })
                        .await
                        .log_if_error("Failed to send Pod log stream failure");
                    return;
                }
            }
        }
    };
    let failed = results.into_iter().filter(|failed| *failed).count();
    if failed == target_count {
        sender
            .send(PodLogStreamFailed {
                log_window_id,
                error: "None of the selected Pod log streams could be opened".to_owned(),
            })
            .await
            .log_if_error("Failed to send Pod log stream failure");
    } else {
        sender
            .send(PodLogStreamEnded { log_window_id })
            .await
            .log_if_error("Failed to send Pod log stream result");
    }
}

async fn stream_target(
    log_client: kube::Client,
    status_client: kube::Client,
    target: PodLogStreamTarget,
    source_index: usize,
    context: StreamTargetContext,
) -> bool {
    let log_pods: Api<Pod> = Api::namespaced(log_client, &target.namespace);
    let status_pods: Api<Pod> = Api::namespaced(status_client, &target.namespace);
    let resume = Arc::new(StdMutex::new(FollowResumeState::new()));
    let backfill_target = target.clone();
    let backfill_pods = status_pods.clone();
    let backfill_ingest = context.ingest.clone();
    let log_window_id = context.log_window_id;
    let live = LiveLogFollower {
        log_pods,
        status_pods,
        target: target.clone(),
        source_index,
        resume: resume.clone(),
        context: context.clone(),
    }
    .follow();
    let backfill_sender = context.sender;
    let backfill_resume = resume;
    let completed_backfills = context.completed_backfills;
    let target_count = context.target_count;
    let backfill = async move {
        let result: anyhow::Result<()> = async {
            let stream = backfill_pods
                .log_stream(
                    &backfill_target.pod_name,
                    &LogParams {
                        container: Some(backfill_target.container.clone()),
                        timestamps: true,
                        ..LogParams::default()
                    },
                )
                .await?;
            append_stream(
                stream,
                backfill_ingest.clone(),
                true,
                &backfill_target,
                source_index,
                backfill_resume,
                None,
            )
            .await
            .map_err(anyhow::Error::new)
        }
        .await;
        if let Err(error) = &result {
            send_source_failure(&backfill_sender, log_window_id, &backfill_target, error).await;
        }
        let mut failed = result.is_err();
        if let Err(error) = backfill_ingest.complete_source_backfill(source_index) {
            send_source_failure(&backfill_sender, log_window_id, &backfill_target, &error).await;
            failed = true;
        }
        if completed_backfills.fetch_add(1, Ordering::AcqRel) + 1 == target_count
            && let Err(error) = backfill_ingest.finish_backfill().await
        {
            send_source_failure(&backfill_sender, log_window_id, &backfill_target, &error).await;
            failed = true;
        }
        failed
    };
    let (live_result, backfill_result) = tokio::join!(live, backfill);
    // A source remains useful when either its historical response or its
    // follow stream succeeded. Source-level failures are already surfaced in
    // the viewer without hiding usable records behind a global failure state.
    live_result && backfill_result
}

impl LiveLogFollower {
    async fn follow(self) -> bool {
        let mut first_attempt = true;
        let mut reconnecting = false;
        let mut backoff = DefaultBackoff::default();
        loop {
            let request = self
                .resume
                .lock()
                .map_err(|_| anyhow::anyhow!("Pod log resume cursor lock was poisoned"))
                .and_then(|resume| {
                    let accepted_before = resume.live_accepted_records;
                    if first_attempt {
                        Ok((
                            LogParams {
                                container: Some(self.target.container.clone()),
                                follow: true,
                                tail_lines: Some(0),
                                timestamps: true,
                                ..LogParams::default()
                            },
                            None,
                            accepted_before,
                        ))
                    } else {
                        let (since_time, replay_filter) = resume.reconnect()?;
                        Ok((
                            LogParams {
                                container: Some(self.target.container.clone()),
                                follow: true,
                                since_time: Some(since_time),
                                timestamps: true,
                                ..LogParams::default()
                            },
                            Some(replay_filter),
                            accepted_before,
                        ))
                    }
                });
            let (params, replay_filter, accepted_before) = match request {
                Ok(request) => request,
                Err(error) => {
                    self.send_failure(&error).await;
                    return true;
                }
            };
            first_attempt = false;

            let mut open_error = None;
            let stream_result = match self
                .log_pods
                .log_stream(&self.target.pod_name, &params)
                .await
            {
                Ok(stream) => {
                    if reconnecting {
                        send_source_recovered(
                            &self.context.sender,
                            self.context.log_window_id,
                            &self.target,
                        )
                        .await;
                        reconnecting = false;
                    }
                    Some(
                        append_stream(
                            stream,
                            self.context.ingest.clone(),
                            false,
                            &self.target,
                            self.source_index,
                            self.resume.clone(),
                            replay_filter,
                        )
                        .await,
                    )
                }
                Err(error) if retryable_log_open_error(&error) => {
                    open_error = Some(format!("{error:#}"));
                    None
                }
                Err(error) => {
                    self.send_failure(&error).await;
                    return true;
                }
            };

            if let Some(Err(AppendStreamError::Ingest(error))) = &stream_result {
                self.send_failure(error).await;
                return true;
            }

            let accepted_after = self
                .resume
                .lock()
                .map(|resume| resume.live_accepted_records)
                .unwrap_or(accepted_before);
            if accepted_after > accepted_before {
                backoff.reset();
            }

            match self.status_pods.get_opt(&self.target.pod_name).await {
                Ok(Some(pod)) if !target_may_produce_more_logs(&pod, &self.target) => {
                    self.clear_reconnecting(reconnecting).await;
                    return false;
                }
                Ok(None) => {
                    self.clear_reconnecting(reconnecting).await;
                    return false;
                }
                Ok(Some(_)) => {}
                Err(error) if retryable_kube_error(&error) => {}
                Err(error) => {
                    self.send_failure(&error).await;
                    return true;
                }
            }

            let error = open_error.unwrap_or_else(|| match stream_result {
                Some(Err(error)) => format!("{error:#}"),
                Some(Ok(())) | None => "The log stream closed unexpectedly".to_owned(),
            });
            send_source_reconnecting(
                &self.context.sender,
                self.context.log_window_id,
                &self.target,
                error,
            )
            .await;
            reconnecting = true;
            tokio::time::sleep(backoff.next().unwrap_or(Duration::from_secs(30))).await;
        }
    }

    async fn send_failure(&self, error: &(impl fmt::Display + ?Sized)) {
        send_source_failure(
            &self.context.sender,
            self.context.log_window_id,
            &self.target,
            error,
        )
        .await;
    }

    async fn clear_reconnecting(&self, reconnecting: bool) {
        if reconnecting {
            send_source_recovered(
                &self.context.sender,
                self.context.log_window_id,
                &self.target,
            )
            .await;
        }
    }
}

fn retryable_kube_error(error: &kube::Error) -> bool {
    match error {
        kube::Error::Api(status) => {
            matches!(status.code, 408 | 409 | 410 | 429) || status.code >= 500
        }
        kube::Error::BuildRequest(_)
        | kube::Error::ProxyProtocolUnsupported { .. }
        | kube::Error::ProxyProtocolDisabled { .. }
        | kube::Error::TlsRequired => false,
        _ => true,
    }
}

fn retryable_log_open_error(error: &kube::Error) -> bool {
    retryable_kube_error(error)
        || matches!(error, kube::Error::Api(status) if matches!(status.code, 400 | 404))
}

async fn send_source_failure(
    sender: &WorkerResultSender,
    log_window_id: u64,
    target: &PodLogStreamTarget,
    error: &(impl std::fmt::Display + ?Sized),
) {
    sender
        .send(PodLogSourceFailed {
            log_window_id,
            target: target.clone(),
            error: format!("{error:#}"),
        })
        .await
        .log_if_error("Failed to send Pod log source failure");
}

async fn send_source_reconnecting(
    sender: &WorkerResultSender,
    log_window_id: u64,
    target: &PodLogStreamTarget,
    error: String,
) {
    sender
        .send(PodLogSourceReconnecting {
            log_window_id,
            target: target.clone(),
            error,
        })
        .await
        .log_if_error("Failed to send Pod log reconnecting state");
}

async fn send_source_recovered(
    sender: &WorkerResultSender,
    log_window_id: u64,
    target: &PodLogStreamTarget,
) {
    sender
        .send(PodLogSourceRecovered {
            log_window_id,
            target: target.clone(),
        })
        .await
        .log_if_error("Failed to send Pod log recovered state");
}

async fn append_stream(
    stream: impl futures_util::AsyncBufRead + Unpin,
    ingest: Arc<LogTimelineAppender>,
    backfill: bool,
    target: &PodLogStreamTarget,
    source_index: usize,
    resume: Arc<StdMutex<FollowResumeState>>,
    mut replay_filter: Option<ReplayFilter>,
) -> Result<(), AppendStreamError> {
    let mut lines = stream.lines();
    let mut batch = Vec::new();
    let source = target.display_name();
    let mut flush = tokio::time::interval(Duration::from_millis(100));
    flush.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            line = lines.try_next() => {
                let line = match line {
                    Ok(Some(line)) => line,
                    Ok(None) => break,
                    Err(error) => {
                        if !batch.is_empty() {
                            append_batch(&ingest, source_index, std::mem::take(&mut batch), backfill)
                                .await
                                .map_err(AppendStreamError::Ingest)?;
                        }
                        return Err(AppendStreamError::Read(error));
                    }
                };
                let record = LogRecord::from_source(line, source.clone());
                if replay_filter
                    .as_mut()
                    .is_some_and(|filter| !filter.accepts(&record))
                {
                    continue;
                }
                resume
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Pod log resume cursor lock was poisoned"))
                    .map_err(AppendStreamError::Ingest)?
                    .observe(&record, backfill);
                if !backfill {
                    ingest
                        .capture_live_prefix(source_index, &record)
                        .map_err(AppendStreamError::Ingest)?;
                }
                batch.push(record);
                if batch.len() >= 64 {
                    append_batch(&ingest, source_index, std::mem::take(&mut batch), backfill)
                        .await
                        .map_err(AppendStreamError::Ingest)?;
                }
            }
            _ = flush.tick(), if !batch.is_empty() => {
                append_batch(&ingest, source_index, std::mem::take(&mut batch), backfill)
                    .await
                    .map_err(AppendStreamError::Ingest)?;
            }
        }
    }
    if !batch.is_empty() {
        append_batch(&ingest, source_index, batch, backfill)
            .await
            .map_err(AppendStreamError::Ingest)?;
    }
    Ok(())
}

async fn append_batch(
    appender: &LogTimelineAppender,
    source_index: usize,
    records: Vec<LogRecord>,
    backfill: bool,
) -> anyhow::Result<()> {
    appender.append(source_index, records, backfill).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log_store::{LogStoreResult, LogStoreService};
    use crate::resource_table::ContainerKind;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn wait_for_store_result(
        service: &LogStoreService,
        matches: impl Fn(&LogStoreResult) -> bool,
    ) -> LogStoreResult {
        let start = std::time::Instant::now();
        loop {
            if let Some(result) = service.try_next_result()
                && matches(&result)
            {
                return result;
            }
            assert!(
                start.elapsed() < Duration::from_secs(2),
                "timed out waiting for log-store result"
            );
            std::thread::yield_now();
        }
    }

    fn record(text: &str, source: &str) -> LogRecord {
        LogRecord::from_source(text.to_owned(), source.to_owned())
    }

    async fn serve_scripted_http_response(
        listener: &tokio::net::TcpListener,
        content_type: &str,
        body: &str,
    ) -> std::io::Result<String> {
        let (mut socket, _) = listener.accept().await?;
        let mut request = Vec::new();
        let mut buffer = [0_u8; 1_024];
        loop {
            let read = socket.read(&mut buffer).await?;
            if read == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..read]);
            if request.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        socket.write_all(response.as_bytes()).await?;
        socket.shutdown().await?;
        Ok(String::from_utf8_lossy(&request).into_owned())
    }

    #[test]
    fn timestamp_order_uses_source_label_as_a_stable_tiebreaker() {
        let mut records = vec![
            record("2026-09-14T09:00:02Z later", "payments/worker-0 · worker"),
            record("2026-09-14T09:00:01Z first", "payments/worker-0 · worker"),
            record("2026-09-14T09:00:01Z first", "payments/api-0 · server"),
            record("no runtime timestamp", "payments/api-0 · server"),
        ];

        records.sort_by(log_timestamp_order);

        assert_eq!(
            records,
            [
                record("2026-09-14T09:00:01Z first", "payments/api-0 · server"),
                record("2026-09-14T09:00:01Z first", "payments/worker-0 · worker"),
                record("2026-09-14T09:00:02Z later", "payments/worker-0 · worker"),
                record("no runtime timestamp", "payments/api-0 · server"),
            ]
        );
    }

    #[test]
    fn timestamp_order_parses_fractional_rfc3339_timestamps() {
        let mut records = vec![
            record("2026-09-14T09:00:00.1Z later", "payments/api-0 · server"),
            record("2026-09-14T09:00:00Z earlier", "payments/worker-0 · worker"),
        ];

        records.sort_by(log_timestamp_order);

        assert_eq!(
            records,
            [
                record("2026-09-14T09:00:00Z earlier", "payments/worker-0 · worker"),
                record("2026-09-14T09:00:00.1Z later", "payments/api-0 · server"),
            ]
        );
    }

    #[test]
    fn resume_filter_discards_only_records_already_delivered_at_the_boundary() {
        let source = "payments/api-0 · server";
        let mut cursor = ResumeCursor::default();
        cursor.observe(&record("2026-09-14T09:00:00.5Z first", source));
        cursor.observe(&record("2026-09-14T09:00:00.5Z second", source));
        let mut replay = resume_filter(&cursor, &ResumeCursor::default());

        assert!(!replay.accepts(&record("2026-09-14T09:00:00Z older", source)));
        assert!(!replay.accepts(&record("2026-09-14T09:00:00.5Z first", source)));
        assert!(!replay.accepts(&record("2026-09-14T09:00:00.5Z second", source)));
        assert!(replay.accepts(&record("2026-09-14T09:00:00.5Z third", source)));
        assert!(replay.accepts(&record("2026-09-14T09:00:01Z newer", source)));
    }

    #[test]
    fn reconnect_deduplication_does_not_double_count_history_overlap() -> anyhow::Result<()> {
        let source = "payments/api-0 · server";
        let boundary = record("2026-09-14T09:00:00.5Z boundary", source);
        let mut resume = FollowResumeState::new();
        resume.observe(&boundary, true);
        resume.observe(&boundary, false);
        resume.observe(&boundary, true);

        let (_, mut replay) = resume.reconnect()?;
        assert!(!replay.accepts(&boundary));
        assert!(replay.accepts(&record("2026-09-14T09:00:00.5Z distinct record", source,)));
        Ok(())
    }

    #[test]
    fn reconnect_resumes_from_history_when_it_has_advanced_beyond_live() -> anyhow::Result<()> {
        let source = "payments/api-0 · server";
        let mut resume = FollowResumeState::new();
        resume.observe(&record("2026-09-14T09:00:01Z live", source), false);
        resume.observe(&record("2026-09-14T09:00:10Z history", source), true);

        let (_, mut replay) = resume.reconnect()?;
        assert!(!replay.accepts(&record("2026-09-14T09:00:05Z already in history", source,)));
        assert!(!replay.accepts(&record("2026-09-14T09:00:10Z history", source,)));
        assert!(replay.accepts(&record("2026-09-14T09:00:10Z new at boundary", source,)));
        Ok(())
    }

    #[test]
    fn equal_live_and_history_boundaries_merge_exact_record_multiplicity() -> anyhow::Result<()> {
        let source = "payments/api-0 · server";
        let duplicate = record("2026-09-14T09:00:10Z duplicate", source);
        let history_only = record("2026-09-14T09:00:10Z history only", source);
        let mut resume = FollowResumeState::new();
        resume.observe(&duplicate, false);
        resume.observe(&duplicate, true);
        resume.observe(&history_only, true);

        let (_, mut replay) = resume.reconnect()?;
        assert!(!replay.accepts(&duplicate));
        assert!(!replay.accepts(&history_only));
        assert!(replay.accepts(&record("2026-09-14T09:00:10Z new at boundary", source,)));
        Ok(())
    }

    #[test]
    fn retry_policy_retries_transient_api_failures_only() {
        let api_error = |code| {
            kube::Error::Api(Box::new(kube::core::Status {
                code,
                ..kube::core::Status::default()
            }))
        };

        assert!(retryable_kube_error(&api_error(408)));
        assert!(retryable_kube_error(&api_error(429)));
        assert!(retryable_kube_error(&api_error(503)));
        assert!(!retryable_kube_error(&api_error(400)));
        assert!(!retryable_kube_error(&api_error(403)));
        assert!(!retryable_kube_error(&api_error(404)));
        assert!(retryable_log_open_error(&api_error(400)));
        assert!(retryable_log_open_error(&api_error(404)));
        assert!(!retryable_log_open_error(&api_error(403)));
    }

    #[tokio::test]
    async fn live_follower_reconnects_resumes_deduplicates_and_stops_at_terminal_status()
    -> anyhow::Result<()> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let running_pod = serde_json::json!({
            "apiVersion": "v1",
            "kind": "Pod",
            "metadata": { "name": "api-0", "namespace": "payments" },
            "spec": {
                "restartPolicy": "Always",
                "containers": [{ "name": "server", "image": "example" }]
            },
            "status": {
                "phase": "Running",
                "containerStatuses": [{
                    "name": "server", "image": "example", "imageID": "example",
                    "ready": true, "restartCount": 0,
                    "state": { "running": { "startedAt": "2026-09-14T09:00:00Z" } }
                }]
            }
        })
        .to_string();
        let terminal_pod = serde_json::json!({
            "apiVersion": "v1",
            "kind": "Pod",
            "metadata": { "name": "api-0", "namespace": "payments" },
            "spec": {
                "restartPolicy": "Never",
                "containers": [{ "name": "server", "image": "example" }]
            },
            "status": {
                "phase": "Succeeded",
                "containerStatuses": [{
                    "name": "server", "image": "example", "imageID": "example",
                    "ready": false, "restartCount": 0,
                    "state": { "terminated": { "exitCode": 0 } }
                }]
            }
        })
        .to_string();
        let server = tokio::spawn(async move {
            let mut requests = Vec::new();
            for (content_type, body) in [
                (
                    "text/plain",
                    "2026-09-14T09:00:00Z first record\n".to_owned(),
                ),
                ("application/json", running_pod),
                (
                    "text/plain",
                    concat!(
                        "2026-09-14T09:00:00Z first record\n",
                        "2026-09-14T09:00:01Z second record\n"
                    )
                    .to_owned(),
                ),
                ("application/json", terminal_pod),
            ] {
                requests.push(
                    serve_scripted_http_response(&listener, content_type, &body)
                        .await
                        .expect("scripted Kubernetes response succeeds"),
                );
            }
            requests
        });

        let config = kube::Config::new(format!("http://{address}").parse()?);
        let client = kube::Client::try_from(config)?;
        let service = LogStoreService::default();
        assert!(service.open(1));
        let ingest = Arc::new(LogTimelineAppender::new(service.appender(), 1, 1)?);
        let (result_sender, mut result_receiver) = tokio::sync::mpsc::channel(8);
        let follower = LiveLogFollower {
            log_pods: Api::namespaced(client.clone(), "payments"),
            status_pods: Api::namespaced(client, "payments"),
            target: PodLogStreamTarget {
                namespace: "payments".into(),
                pod_name: "api-0".into(),
                container: "server".into(),
                kind: ContainerKind::App,
            },
            source_index: 0,
            resume: Arc::new(StdMutex::new(FollowResumeState::new())),
            context: StreamTargetContext {
                log_window_id: 1,
                ingest: ingest.clone(),
                completed_backfills: Arc::new(AtomicUsize::new(0)),
                target_count: 1,
                sender: WorkerResultSender::new(result_sender, None),
            },
        };

        assert!(
            !tokio::time::timeout(Duration::from_secs(5), follower.follow())
                .await
                .expect("the scripted reconnect lifecycle completes")
        );
        ingest.flush_all_live().await?;
        let LogStoreResult::Updated { appended_rows, .. } =
            wait_for_store_result(&service, |result| {
                matches!(result, LogStoreResult::Updated { window_id: 1, .. })
            })
        else {
            unreachable!()
        };
        assert_eq!(
            appended_rows
                .iter()
                .map(|row| row.text.as_str())
                .collect::<Vec<_>>(),
            ["first record", "second record"]
        );

        let events = std::iter::from_fn(|| result_receiver.try_recv().ok()).collect::<Vec<_>>();
        assert!(
            events
                .iter()
                .any(|event| { event.as_ref().as_any().is::<PodLogSourceReconnecting>() })
        );
        assert!(
            events
                .iter()
                .any(|event| { event.as_ref().as_any().is::<PodLogSourceRecovered>() })
        );
        let requests = server.await?;
        assert!(requests[0].contains("tailLines=0"));
        assert!(requests[2].contains("sinceTime="));
        Ok(())
    }

    #[test]
    fn container_lifecycle_retries_only_sources_that_can_produce_more_logs()
    -> Result<(), serde_json::Error> {
        let app = PodLogStreamTarget {
            namespace: "payments".into(),
            pod_name: "api-0".into(),
            container: "server".into(),
            kind: ContainerKind::App,
        };
        let running: Pod = serde_json::from_value(serde_json::json!({
            "spec": { "containers": [{ "name": "server" }] },
            "status": {
                "phase": "Running",
                "containerStatuses": [{
                    "name": "server",
                    "image": "example",
                    "imageID": "example",
                    "ready": true,
                    "restartCount": 0,
                    "state": { "running": { "startedAt": "2026-09-14T09:00:00Z" } }
                }]
            }
        }))?;
        assert!(target_may_produce_more_logs(&running, &app));

        let completed: Pod = serde_json::from_value(serde_json::json!({
            "spec": {
                "restartPolicy": "Never",
                "containers": [{ "name": "server" }]
            },
            "status": {
                "phase": "Succeeded",
                "containerStatuses": [{
                    "name": "server",
                    "image": "example",
                    "imageID": "example",
                    "ready": false,
                    "restartCount": 0,
                    "state": { "terminated": { "exitCode": 0 } }
                }]
            }
        }))?;
        assert!(!target_may_produce_more_logs(&completed, &app));

        let mut restartable = completed.clone();
        restartable.status.as_mut().expect("Pod has status").phase = Some("Running".into());
        restartable
            .spec
            .as_mut()
            .expect("Pod has a spec")
            .restart_policy = Some("Always".into());
        assert!(target_may_produce_more_logs(&restartable, &app));

        let mut on_failure = restartable.clone();
        on_failure
            .spec
            .as_mut()
            .expect("Pod has a spec")
            .restart_policy = Some("OnFailure".into());
        assert!(!target_may_produce_more_logs(&on_failure, &app));
        on_failure
            .status
            .as_mut()
            .expect("Pod has status")
            .container_statuses
            .as_mut()
            .expect("Pod has container status")[0]
            .state
            .as_mut()
            .expect("container has state")
            .terminated
            .as_mut()
            .expect("container is terminated")
            .exit_code = 1;
        assert!(target_may_produce_more_logs(&on_failure, &app));

        let init_target = PodLogStreamTarget {
            container: "setup".into(),
            kind: ContainerKind::Init,
            ..app.clone()
        };
        let mut init_completed: Pod = serde_json::from_value(serde_json::json!({
            "spec": {
                "initContainers": [{ "name": "setup", "image": "example" }],
                "containers": [{ "name": "server", "image": "example" }]
            },
            "status": {
                "phase": "Running",
                "initContainerStatuses": [{
                    "name": "setup", "image": "example", "imageID": "example",
                    "ready": false, "restartCount": 0,
                    "state": { "terminated": { "exitCode": 0 } }
                }]
            }
        }))?;
        assert!(!target_may_produce_more_logs(&init_completed, &init_target));
        init_completed
            .spec
            .as_mut()
            .expect("Pod has a spec")
            .init_containers
            .as_mut()
            .expect("Pod has init containers")[0]
            .restart_policy = Some("Always".into());
        assert!(target_may_produce_more_logs(&init_completed, &init_target));

        let ephemeral_target = PodLogStreamTarget {
            container: "debugger".into(),
            kind: ContainerKind::Ephemeral,
            ..app.clone()
        };
        let ephemeral_completed: Pod = serde_json::from_value(serde_json::json!({
            "spec": { "containers": [{ "name": "server", "image": "example" }] },
            "status": {
                "phase": "Running",
                "ephemeralContainerStatuses": [{
                    "name": "debugger", "image": "example", "imageID": "example",
                    "ready": false, "restartCount": 0,
                    "state": { "terminated": { "exitCode": 0 } }
                }]
            }
        }))?;
        assert!(!target_may_produce_more_logs(
            &ephemeral_completed,
            &ephemeral_target
        ));
        Ok(())
    }

    #[test]
    fn history_spool_persists_length_delimited_log_records() -> anyhow::Result<()> {
        let mut spool = HistorySpool::new()?;
        spool.append(vec![
            record("2026-09-14T09:00:00Z first line", "payments/api-0 · server"),
            record("second\nline remains one record", "payments/api-0 · server"),
        ])?;

        let spool_lines = spool.total_lines;
        let mut reader = spool.into_reader(spool_lines)?;
        assert_eq!(
            read_next_spooled_record(&mut reader)?,
            Some(record(
                "2026-09-14T09:00:00Z first line",
                "payments/api-0 · server"
            ))
        );
        assert_eq!(
            read_next_spooled_record(&mut reader)?,
            Some(record(
                "second\nline remains one record",
                "payments/api-0 · server"
            ))
        );
        assert_eq!(read_next_spooled_record(&mut reader)?, None);
        Ok(())
    }

    #[test]
    fn history_spools_merge_chronologically_without_loading_them_all() -> anyhow::Result<()> {
        let mut api = HistorySpool::new()?;
        api.append(vec![
            record("2026-09-14T09:00:01Z api first", "payments/api-0 · server"),
            record("2026-09-14T09:00:03Z api third", "payments/api-0 · server"),
        ])?;
        let mut worker = HistorySpool::new()?;
        worker.append(vec![
            record(
                "2026-09-14T09:00:02Z worker second",
                "payments/worker-0 · worker",
            ),
            record(
                "2026-09-14T09:00:04Z worker fourth",
                "payments/worker-0 · worker",
            ),
        ])?;

        let api_lines = api.total_lines;
        let worker_lines = worker.total_lines;
        let mut readers = vec![
            api.into_reader(api_lines)?,
            worker.into_reader(worker_lines)?,
        ];
        let mut pending = BinaryHeap::new();
        for (source_index, reader) in readers.iter_mut().enumerate() {
            pending.push(HistoryMergeEntry {
                record: read_next_spooled_record(reader)?.expect("source has a first line"),
                source_index,
            });
        }

        let mut merged = Vec::new();
        while let Some(record) = next_history_merge_record(&mut readers, &mut pending)? {
            merged.push(record);
        }
        assert_eq!(
            merged,
            [
                record("2026-09-14T09:00:01Z api first", "payments/api-0 · server"),
                record(
                    "2026-09-14T09:00:02Z worker second",
                    "payments/worker-0 · worker"
                ),
                record("2026-09-14T09:00:03Z api third", "payments/api-0 · server"),
                record(
                    "2026-09-14T09:00:04Z worker fourth",
                    "payments/worker-0 · worker"
                ),
            ]
        );
        Ok(())
    }

    #[test]
    fn history_overlap_only_removes_a_source_boundary_suffix() -> anyhow::Result<()> {
        let mut history = HistorySpool::new()?;
        history.append(vec![
            record("2026-09-14T09:00:00Z before", "payments/api-0 · server"),
            record(
                "2026-09-14T09:00:01Z overlap one",
                "payments/api-0 · server",
            ),
            record(
                "2026-09-14T09:00:02Z overlap two",
                "payments/api-0 · server",
            ),
        ])?;
        let mut live = HistorySpool::new()?;
        live.append(vec![
            record(
                "2026-09-14T09:00:01Z overlap one",
                "payments/api-0 · server",
            ),
            record(
                "2026-09-14T09:00:02Z overlap two",
                "payments/api-0 · server",
            ),
            record("2026-09-14T09:00:03Z after", "payments/api-0 · server"),
        ])?;

        assert_eq!(history.overlap_with_live_prefix(&live)?, 2);
        Ok(())
    }

    #[test]
    fn live_lines_continue_after_history_spools_move_into_the_merge() -> anyhow::Result<()> {
        let service = LogStoreService::default();
        let appender = LogTimelineAppender::new(service.appender(), 1, 1)?;
        appender
            .history
            .lock()
            .expect("history lock is available")
            .clear();

        appender.capture_live_prefix(
            0,
            &record(
                "2026-09-14T09:00:00Z late live line",
                "payments/api-0 · server",
            ),
        )?;
        Ok(())
    }

    #[tokio::test]
    async fn live_batches_do_not_wait_for_the_cross_source_ordering_window() -> anyhow::Result<()> {
        let service = LogStoreService::default();
        assert!(service.open(1));
        let ingest = LogTimelineAppender::new(service.appender(), 1, 1)?;

        tokio::time::timeout(
            Duration::from_millis(50),
            ingest.append(
                0,
                vec![record(
                    "2026-09-14T09:00:00Z live",
                    "payments/api-0 · server",
                )],
                false,
            ),
        )
        .await
        .expect("enqueueing a live batch must not sleep for the ordering window")?;

        ingest.flush_all_live().await?;
        let LogStoreResult::Updated { appended_rows, .. } =
            wait_for_store_result(&service, |result| {
                matches!(result, LogStoreResult::Updated { window_id: 1, .. })
            })
        else {
            unreachable!()
        };
        assert_eq!(appended_rows[0].text, "live");
        Ok(())
    }

    #[tokio::test]
    async fn live_ordering_buffer_applies_backpressure_at_its_row_limit() -> anyhow::Result<()> {
        let service = LogStoreService::default();
        assert!(service.open(1));
        let ingest = LogTimelineAppender::new(service.appender(), 1, 1)?;
        let records = (0..MAX_PENDING_LIVE_ROWS)
            .map(|index| {
                record(
                    &format!("2026-09-14T09:00:00Z live {index}"),
                    "payments/api-0 · server",
                )
            })
            .collect();
        ingest.append(0, records, false).await?;

        let blocked = ingest.append(
            0,
            vec![record(
                "2026-09-14T09:00:01Z blocked",
                "payments/api-0 · server",
            )],
            false,
        );
        tokio::pin!(blocked);
        assert!(
            tokio::time::timeout(Duration::from_millis(25), &mut blocked)
                .await
                .is_err(),
            "the producer must wait while the ordering buffer is full"
        );

        ingest.flush_all_live().await?;
        tokio::time::timeout(Duration::from_secs(1), blocked).await??;
        Ok(())
    }

    #[tokio::test]
    async fn live_ordering_buffer_flushes_sources_by_timestamp() -> anyhow::Result<()> {
        let service = LogStoreService::default();
        assert!(service.open(1));
        let ingest = LogTimelineAppender::new(service.appender(), 1, 2)?;
        ingest
            .append(
                0,
                vec![record(
                    "2026-09-14T09:00:02Z later",
                    "payments/api-0 · server",
                )],
                false,
            )
            .await?;
        ingest
            .append(
                1,
                vec![record(
                    "2026-09-14T09:00:01Z earlier",
                    "payments/worker-0 · worker",
                )],
                false,
            )
            .await?;

        ingest.flush_all_live().await?;
        let LogStoreResult::Updated { appended_rows, .. } =
            wait_for_store_result(&service, |result| {
                matches!(result, LogStoreResult::Updated { window_id: 1, .. })
            })
        else {
            unreachable!()
        };
        assert_eq!(
            appended_rows
                .iter()
                .map(|row| row.text.as_str())
                .collect::<Vec<_>>(),
            ["earlier", "later"]
        );
        Ok(())
    }

    #[tokio::test]
    async fn one_source_stream_flushes_before_an_error_and_rebases_history() -> anyhow::Result<()> {
        let service = LogStoreService::default();
        assert!(service.open(1));
        let ingest = Arc::new(LogTimelineAppender::new(service.appender(), 1, 1)?);
        let target = PodLogStreamTarget {
            namespace: "payments".to_owned(),
            pod_name: "api-0".to_owned(),
            container: "server".to_owned(),
            kind: ContainerKind::App,
        };
        let live_line = "2026-09-14T09:00:01Z live";
        let mut live_bytes = live_line.as_bytes().to_vec();
        live_bytes.extend_from_slice(b"\n\xff");
        let live_stream = futures_util::io::Cursor::new(live_bytes);

        assert!(
            append_stream(
                live_stream,
                ingest.clone(),
                false,
                &target,
                0,
                Arc::new(StdMutex::new(FollowResumeState::new())),
                None,
            )
            .await
            .is_err()
        );
        ingest.flush_all_live().await?;
        let LogStoreResult::Updated { appended_rows, .. } =
            wait_for_store_result(&service, |result| {
                matches!(result, LogStoreResult::Updated { window_id: 1, .. })
            })
        else {
            unreachable!()
        };
        assert_eq!(appended_rows[0].text, "live");
        assert_eq!(
            appended_rows[0].source.as_deref(),
            Some("payments/api-0 · server")
        );

        ingest
            .append(
                0,
                vec![
                    record("2026-09-14T09:00:00Z history", "payments/api-0 · server"),
                    record(live_line, "payments/api-0 · server"),
                ],
                true,
            )
            .await?;
        ingest.complete_source_backfill(0)?;
        ingest.finish_backfill().await?;
        let _ = wait_for_store_result(&service, |result| {
            matches!(result, LogStoreResult::Rebased { window_id: 1, .. })
        });

        assert!(service.load_page(1, 0, false, 0));
        let LogStoreResult::PageLoaded { rows, .. } = wait_for_store_result(&service, |result| {
            matches!(result, LogStoreResult::PageLoaded { window_id: 1, .. })
        }) else {
            unreachable!()
        };
        assert_eq!(
            rows.iter().map(|row| row.text.as_str()).collect::<Vec<_>>(),
            ["history", "live"]
        );
        assert!(
            rows.iter()
                .all(|row| { row.source.as_deref() == Some("payments/api-0 · server") })
        );
        Ok(())
    }
}
