//! Pod-log stream ingestion, including concurrent tail and history backfill.

use super::{
    PodLogSourceFailed, PodLogStreamEnded, PodLogStreamFailed, PodLogStreamTarget,
    WorkerResultSender,
};
use crate::helpers::ResultExt;
use crate::log_store::{LogRecord, LogStoreAppender, read_log_record, write_log_record};
use futures_util::{AsyncBufReadExt, StreamExt, TryStreamExt};
use k8s_openapi::api::core::v1::Pod;
use kube::api::{Api, LogParams};
use std::cmp::Ordering as CmpOrdering;
use std::collections::BinaryHeap;
use std::io::{BufReader, Read, Seek, SeekFrom, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tempfile::NamedTempFile;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::sync::Mutex;
use tokio::time::Instant;

#[derive(Clone)]
struct StreamTargetContext {
    log_window_id: u64,
    ingest: Arc<LogTimelineAppender>,
    completed_backfills: Arc<AtomicUsize>,
    target_count: usize,
    sender: WorkerResultSender,
}

struct LogTimelineAppender {
    appender: LogStoreAppender,
    log_window_id: u64,
    live: Mutex<Vec<PendingLiveLine>>,
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

        let received_at = Instant::now();
        self.live
            .lock()
            .await
            .extend(records.into_iter().map(|record| PendingLiveLine {
                record,
                received_at,
            }));
        // A follow stream has no cross-source watermark: a silent source can
        // always still deliver an older record. Retaining records briefly
        // gives concurrently arriving sources a shared ordering window while
        // ensuring a quiet source cannot stall the live viewer indefinitely.
        tokio::time::sleep(LIVE_ORDERING_WINDOW).await;
        self.flush_live().await
    }

    async fn flush_live(&self) -> anyhow::Result<()> {
        let cutoff = Instant::now() - LIVE_ORDERING_WINDOW;
        let mut pending = self.live.lock().await;
        let mut records = Vec::new();
        let mut retained = Vec::with_capacity(pending.len());
        for pending_line in std::mem::take(&mut *pending) {
            if pending_line.received_at <= cutoff {
                records.push(pending_line.record);
            } else {
                retained.push(pending_line);
            }
        }
        *pending = retained;
        drop(pending);
        if records.is_empty() {
            return Ok(());
        }
        records.sort_by(log_timestamp_order);
        self.appender.append(self.log_window_id, records).await
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
const HISTORY_MERGE_BATCH_SIZE: usize = 512;

struct PendingLiveLine {
    record: LogRecord,
    received_at: Instant,
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
    let timestamp = |record: &LogRecord| {
        crate::ansi::parse_kubernetes_log_line(&record.text)
            .timestamp
            .and_then(|timestamp| OffsetDateTime::parse(&timestamp, &Rfc3339).ok())
    };
    let left_timestamp = timestamp(left);
    let right_timestamp = timestamp(right);
    match (left_timestamp, right_timestamp) {
        (Some(left_timestamp), Some(right_timestamp)) => left_timestamp
            .cmp(&right_timestamp)
            .then_with(|| left.source.cmp(&right.source)),
        (None, None) => std::cmp::Ordering::Equal,
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
    }
}

pub(super) async fn stream(
    log_window_id: u64,
    client: kube::Client,
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
    let results = futures_util::stream::iter(targets)
        .enumerate()
        .map(|(source_index, target)| {
            let client = client.clone();
            let context = context.clone();
            async move { stream_target(client, target, source_index, context).await }
        })
        .buffer_unordered(target_count.max(1))
        .collect::<Vec<_>>()
        .await;
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
    client: kube::Client,
    target: PodLogStreamTarget,
    source_index: usize,
    context: StreamTargetContext,
) -> bool {
    let pods: Api<Pod> = Api::namespaced(client, &target.namespace);
    let backfill_target = target.clone();
    let tail_pods = pods.clone();
    let backfill_pods = pods.clone();
    let backfill_ingest = context.ingest.clone();
    let live_target = target.clone();
    let live_sender = context.sender.clone();
    let live_ingest = context.ingest.clone();
    let log_window_id = context.log_window_id;
    let live = async move {
        match tail_pods
            .log_stream(
                &live_target.pod_name,
                &LogParams {
                    container: Some(live_target.container.clone()),
                    follow: true,
                    tail_lines: Some(0),
                    timestamps: true,
                    ..LogParams::default()
                },
            )
            .await
        {
            Ok(stream) => {
                let result =
                    append_stream(stream, live_ingest, false, &live_target, source_index).await;
                if let Err(error) = &result {
                    send_source_failure(&live_sender, log_window_id, &live_target, error).await;
                }
                result.is_err()
            }
            Err(error) => {
                send_source_failure(&live_sender, log_window_id, &live_target, &error).await;
                true
            }
        }
    };
    let backfill_sender = context.sender;
    let completed_backfills = context.completed_backfills;
    let target_count = context.target_count;
    let backfill = async move {
        let result = async {
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
            )
            .await
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

async fn append_stream(
    stream: impl futures_util::AsyncBufRead + Unpin,
    ingest: Arc<LogTimelineAppender>,
    backfill: bool,
    target: &PodLogStreamTarget,
    source_index: usize,
) -> anyhow::Result<()> {
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
                            append_batch(&ingest, source_index, std::mem::take(&mut batch), backfill).await?;
                        }
                        return Err(error.into());
                    }
                };
                let record = LogRecord::from_source(line, source.clone());
                if !backfill {
                    ingest.capture_live_prefix(source_index, &record)?;
                }
                batch.push(record);
                if batch.len() >= 64 {
                    append_batch(&ingest, source_index, std::mem::take(&mut batch), backfill).await?;
                }
            }
            _ = flush.tick(), if !batch.is_empty() => {
                append_batch(&ingest, source_index, std::mem::take(&mut batch), backfill).await?;
            }
        }
    }
    if !batch.is_empty() {
        append_batch(&ingest, source_index, batch, backfill).await?;
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
    async fn one_source_stream_flushes_before_an_error_and_rebases_history() -> anyhow::Result<()> {
        let service = LogStoreService::default();
        assert!(service.open(1));
        let ingest = Arc::new(LogTimelineAppender::new(service.appender(), 1, 1)?);
        let target = PodLogStreamTarget {
            namespace: "payments".to_owned(),
            pod_name: "api-0".to_owned(),
            container: "server".to_owned(),
        };
        let live_line = "2026-09-14T09:00:01Z live";
        let mut live_bytes = live_line.as_bytes().to_vec();
        live_bytes.extend_from_slice(b"\n\xff");
        let live_stream = futures_util::io::Cursor::new(live_bytes);

        assert!(
            append_stream(live_stream, ingest.clone(), false, &target, 0)
                .await
                .is_err()
        );
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
