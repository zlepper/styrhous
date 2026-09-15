//! Pod-log stream ingestion, including concurrent tail and history backfill.

use super::{
    PodLogSourceFailed, PodLogStreamEnded, PodLogStreamFailed, PodLogStreamTarget,
    WorkerResultSender,
};
use crate::helpers::ResultExt;
use crate::log_store::LogStoreAppender;
use futures_util::{AsyncBufReadExt, StreamExt, TryStreamExt};
use k8s_openapi::api::core::v1::Pod;
use kube::api::{Api, LogParams};
use std::cmp::Ordering as CmpOrdering;
use std::collections::BinaryHeap;
use std::io::{BufReader, ErrorKind, Read, Seek, SeekFrom, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tempfile::NamedTempFile;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::sync::Mutex;
use tokio::time::Instant;

#[derive(Clone)]
enum IngestAppender {
    Direct(LogStoreAppender),
    Interleaved(Arc<InterleavedAppender>),
}

#[derive(Clone)]
struct StreamTargetContext {
    log_window_id: u64,
    ingest: IngestAppender,
    completed_backfills: Arc<AtomicUsize>,
    target_count: usize,
    source_labels: bool,
    sender: WorkerResultSender,
}

struct InterleavedAppender {
    appender: LogStoreAppender,
    log_window_id: u64,
    live: Mutex<Vec<PendingLiveLine>>,
    history: std::sync::Mutex<Vec<InterleavedHistorySource>>,
}

impl InterleavedAppender {
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
                    .map(|_| InterleavedHistorySource::new())
                    .collect::<anyhow::Result<_>>()?,
            ),
        })
    }

    async fn append(
        &self,
        source_index: usize,
        lines: Vec<String>,
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
                .map_err(|_| anyhow::anyhow!("Interleaved log history lock was poisoned"))?;
            return history
                .get_mut(source_index)
                .ok_or_else(|| anyhow::anyhow!("Unknown interleaved log source"))?
                .history
                .append(lines);
        }

        let received_at = Instant::now();
        self.live.lock().await.extend(
            lines
                .into_iter()
                .map(|line| PendingLiveLine { line, received_at }),
        );
        // A follow stream has no cross-source watermark: a silent source can
        // always still deliver an older record. Retaining records briefly
        // gives concurrently arriving sources a shared ordering window while
        // ensuring a quiet source cannot stall the live viewer indefinitely.
        tokio::time::sleep(INTERLEAVE_WINDOW).await;
        self.flush_live().await
    }

    async fn flush_live(&self) -> anyhow::Result<()> {
        let cutoff = Instant::now() - INTERLEAVE_WINDOW;
        let mut pending = self.live.lock().await;
        let mut lines = Vec::new();
        let mut retained = Vec::with_capacity(pending.len());
        for pending_line in std::mem::take(&mut *pending) {
            if pending_line.received_at <= cutoff {
                lines.push(pending_line.line);
            } else {
                retained.push(pending_line);
            }
        }
        *pending = retained;
        drop(pending);
        if lines.is_empty() {
            return Ok(());
        }
        lines.sort_by(|left, right| log_timestamp_order(left, right));
        self.appender.append(self.log_window_id, lines).await
    }

    async fn complete_backfill(&self) -> anyhow::Result<()> {
        let sources = std::mem::take(
            &mut *self
                .history
                .lock()
                .map_err(|_| anyhow::anyhow!("Interleaved log history lock was poisoned"))?,
        );
        merge_history_spools(&self.appender, self.log_window_id, sources).await
    }

    fn capture_live_prefix(&self, source_index: usize, line: &str) -> anyhow::Result<()> {
        let mut history = self
            .history
            .lock()
            .map_err(|_| anyhow::anyhow!("Interleaved log history lock was poisoned"))?;
        // The per-source spools move into the disk merge after all histories
        // finish. Follow streams intentionally continue afterwards, but no
        // longer need boundary capture.
        if history.is_empty() {
            return Ok(());
        }
        let source = history
            .get_mut(source_index)
            .ok_or_else(|| anyhow::anyhow!("Unknown interleaved log source"))?;
        if !source.backfill_complete {
            source
                .live_prefix
                .append(std::iter::once(line.to_owned()))?;
        }
        Ok(())
    }

    fn complete_source_backfill(&self, source_index: usize) -> anyhow::Result<()> {
        let mut history = self
            .history
            .lock()
            .map_err(|_| anyhow::anyhow!("Interleaved log history lock was poisoned"))?;
        history
            .get_mut(source_index)
            .ok_or_else(|| anyhow::anyhow!("Unknown interleaved log source"))?
            .backfill_complete = true;
        Ok(())
    }
}

const INTERLEAVE_WINDOW: Duration = Duration::from_millis(100);
const HISTORY_MERGE_BATCH_SIZE: usize = 512;

struct PendingLiveLine {
    line: String,
    received_at: Instant,
}

struct HistorySpool {
    data: NamedTempFile,
    offsets: NamedTempFile,
    total_lines: usize,
}

struct InterleavedHistorySource {
    history: HistorySpool,
    live_prefix: HistorySpool,
    backfill_complete: bool,
}

impl InterleavedHistorySource {
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

    fn append(&mut self, lines: impl IntoIterator<Item = String>) -> anyhow::Result<()> {
        let data = self.data.as_file_mut();
        let offsets = self.offsets.as_file_mut();
        let mut next_offset = data.seek(SeekFrom::End(0))?;
        for line in lines {
            let bytes = line.as_bytes();
            let length = u32::try_from(bytes.len())
                .map_err(|_| anyhow::anyhow!("A log line exceeds 4 GiB"))?;
            offsets.write_all(&next_offset.to_le_bytes())?;
            data.write_all(&length.to_le_bytes())?;
            data.write_all(bytes)?;
            next_offset += u64::from(length) + 4;
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
                if self.read_line(history_start + offset)? != live_prefix.read_line(offset)? {
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

    fn read_line(&self, line_index: usize) -> anyhow::Result<String> {
        let mut offsets = self.offsets.reopen()?;
        offsets.seek(SeekFrom::Start(
            (line_index * std::mem::size_of::<u64>()) as u64,
        ))?;
        let mut offset = [0; 8];
        offsets.read_exact(&mut offset)?;
        let mut data = self.data.reopen()?;
        data.seek(SeekFrom::Start(u64::from_le_bytes(offset)))?;
        read_spooled_record(&mut BufReader::new(data))?
            .ok_or_else(|| anyhow::anyhow!("Missing log record in history spool"))
    }
}

struct HistoryReader {
    data: BufReader<std::fs::File>,
    remaining: usize,
}

struct HistoryMergeEntry {
    line: String,
    source_index: usize,
}

impl Ord for HistoryMergeEntry {
    fn cmp(&self, other: &Self) -> CmpOrdering {
        // BinaryHeap is a max heap, so reverse the chronological comparison.
        log_timestamp_order(&other.line, &self.line)
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
        self.source_index == other.source_index && self.line == other.line
    }
}

impl Eq for HistoryMergeEntry {}

async fn merge_history_spools(
    appender: &LogStoreAppender,
    log_window_id: u64,
    sources: Vec<InterleavedHistorySource>,
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
        if let Some(line) = read_spooled_line(reader)? {
            pending.push(HistoryMergeEntry { line, source_index });
        }
    }

    let mut batch = Vec::with_capacity(HISTORY_MERGE_BATCH_SIZE);
    while let Some(line) = next_history_merge_line(&mut readers, &mut pending)? {
        batch.push(line);
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

fn next_history_merge_line(
    readers: &mut [HistoryReader],
    pending: &mut BinaryHeap<HistoryMergeEntry>,
) -> anyhow::Result<Option<String>> {
    let Some(HistoryMergeEntry { line, source_index }) = pending.pop() else {
        return Ok(None);
    };
    if let Some(next_line) = read_spooled_line(&mut readers[source_index])? {
        pending.push(HistoryMergeEntry {
            line: next_line,
            source_index,
        });
    }
    Ok(Some(line))
}

fn read_spooled_line(reader: &mut HistoryReader) -> anyhow::Result<Option<String>> {
    if reader.remaining == 0 {
        return Ok(None);
    }
    reader.remaining -= 1;
    read_spooled_record(&mut reader.data)
}

fn read_spooled_record(reader: &mut BufReader<std::fs::File>) -> anyhow::Result<Option<String>> {
    let mut length = [0; 4];
    match reader.read_exact(&mut length) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error.into()),
    }
    let mut bytes = vec![0; u32::from_le_bytes(length) as usize];
    reader.read_exact(&mut bytes)?;
    Ok(Some(String::from_utf8(bytes)?))
}

impl IngestAppender {
    fn observe_live_line(&self, source_index: usize, line: &str) -> anyhow::Result<()> {
        if let Self::Interleaved(appender) = self {
            appender.capture_live_prefix(source_index, line)?;
        }
        Ok(())
    }

    async fn append(
        &self,
        log_window_id: u64,
        source_index: usize,
        lines: Vec<String>,
        backfill: bool,
    ) -> anyhow::Result<()> {
        match self {
            Self::Direct(appender) => {
                if backfill {
                    appender.append_backfill(log_window_id, lines).await
                } else {
                    appender.append(log_window_id, lines).await
                }
            }
            Self::Interleaved(appender) => appender.append(source_index, lines, backfill).await,
        }
    }

    async fn complete_backfill(&self, log_window_id: u64) -> anyhow::Result<()> {
        if let Self::Interleaved(appender) = self {
            appender.complete_backfill().await?;
        }
        match self {
            Self::Direct(appender) => appender.complete_backfill(log_window_id).await,
            Self::Interleaved(appender) => appender.appender.complete_backfill(log_window_id).await,
        }
    }

    fn complete_source_backfill(&self, source_index: usize) -> anyhow::Result<()> {
        if let Self::Interleaved(appender) = self {
            appender.complete_source_backfill(source_index)?;
        }
        Ok(())
    }
}

fn log_timestamp_order(left: &str, right: &str) -> std::cmp::Ordering {
    let timestamp = |line: &str| {
        crate::ansi::parse_kubernetes_log_line(line)
            .timestamp
            .and_then(|timestamp| OffsetDateTime::parse(&timestamp, &Rfc3339).ok())
    };
    let left_timestamp = timestamp(left);
    let right_timestamp = timestamp(right);
    match (left_timestamp, right_timestamp) {
        (Some(left_timestamp), Some(right_timestamp)) => left_timestamp
            .cmp(&right_timestamp)
            .then_with(|| log_source_label(left).cmp(log_source_label(right))),
        (None, None) => std::cmp::Ordering::Equal,
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
    }
}

fn log_source_label(line: &str) -> &str {
    line.split_once(' ')
        .map(|(_, message)| message)
        .unwrap_or(line)
        .strip_prefix('[')
        .and_then(|message| message.split_once(']'))
        .map(|(label, _)| label)
        .unwrap_or_default()
}

pub(super) async fn stream(
    log_window_id: u64,
    client: kube::Client,
    targets: Vec<PodLogStreamTarget>,
    log_store_appender: LogStoreAppender,
    sender: WorkerResultSender,
) {
    let target_count = targets.len();
    let source_labels = target_count > 1;
    let ingest = if source_labels {
        match InterleavedAppender::new(log_store_appender, log_window_id, target_count) {
            Ok(appender) => IngestAppender::Interleaved(Arc::new(appender)),
            Err(error) => {
                sender
                    .send(PodLogStreamFailed {
                        log_window_id,
                        error: format!("Could not initialize interleaved log storage: {error:#}"),
                    })
                    .await
                    .log_if_error("Failed to send Pod log stream failure");
                return;
            }
        }
    } else {
        IngestAppender::Direct(log_store_appender)
    };
    let completed_backfills = Arc::new(AtomicUsize::new(0));
    let context = StreamTargetContext {
        log_window_id,
        ingest,
        completed_backfills,
        target_count,
        source_labels,
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
    let source_labels = context.source_labels;
    let live = async move {
        match tail_pods
            .log_stream(
                &live_target.pod_name,
                &LogParams {
                    container: Some(live_target.container.clone()),
                    follow: true,
                    tail_lines: Some(if source_labels { 0 } else { 1_000 }),
                    timestamps: true,
                    ..LogParams::default()
                },
            )
            .await
        {
            Ok(stream) => {
                let result = append_stream(
                    stream,
                    live_ingest,
                    log_window_id,
                    false,
                    &live_target,
                    source_labels,
                    source_index,
                )
                .await;
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
                log_window_id,
                true,
                &backfill_target,
                source_labels,
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
            && let Err(error) = backfill_ingest.complete_backfill(log_window_id).await
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
    ingest: IngestAppender,
    log_window_id: u64,
    backfill: bool,
    target: &PodLogStreamTarget,
    source_labels: bool,
    source_index: usize,
) -> anyhow::Result<()> {
    let mut lines = stream.lines();
    let mut batch = Vec::new();
    let mut flush = tokio::time::interval(Duration::from_millis(100));
    flush.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            line = lines.try_next() => {
                let Some(line) = line? else { break };
                let line = if source_labels {
                    label_line(&line, target)
                } else {
                    line
                };
                if !backfill {
                    ingest.observe_live_line(source_index, &line)?;
                }
                batch.push(line);
                if batch.len() >= 64 {
                    append_batch(&ingest, log_window_id, source_index, std::mem::take(&mut batch), backfill).await?;
                }
            }
            _ = flush.tick(), if !batch.is_empty() => {
                append_batch(&ingest, log_window_id, source_index, std::mem::take(&mut batch), backfill).await?;
            }
        }
    }
    if !batch.is_empty() {
        append_batch(&ingest, log_window_id, source_index, batch, backfill).await?;
    }
    Ok(())
}

fn label_line(line: &str, target: &PodLogStreamTarget) -> String {
    let label = target.display_name();
    if crate::ansi::parse_kubernetes_log_line(line)
        .timestamp
        .is_some()
        && let Some((timestamp, message)) = line.split_once(' ')
    {
        format!("{timestamp} [{label}] {message}")
    } else {
        format!("[{label}] {line}")
    }
}

async fn append_batch(
    appender: &IngestAppender,
    log_window_id: u64,
    source_index: usize,
    lines: Vec<String>,
    backfill: bool,
) -> anyhow::Result<()> {
    appender
        .append(log_window_id, source_index, lines, backfill)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log_store::LogStoreService;

    #[test]
    fn labels_timestamped_and_plain_records_with_their_source() {
        let target = PodLogStreamTarget {
            namespace: "payments".into(),
            pod_name: "api-0".into(),
            container: "server".into(),
        };

        assert_eq!(
            label_line("2026-09-14T09:00:00Z ready", &target),
            "2026-09-14T09:00:00Z [payments/api-0 · server] ready"
        );
        assert_eq!(
            label_line("plain output", &target),
            "[payments/api-0 · server] plain output"
        );
    }

    #[test]
    fn timestamp_order_uses_source_label_as_a_stable_tiebreaker() {
        let mut lines = vec![
            "2026-09-14T09:00:02Z [payments/worker-0 · worker] later".to_owned(),
            "2026-09-14T09:00:01Z [payments/worker-0 · worker] first".to_owned(),
            "2026-09-14T09:00:01Z [payments/api-0 · server] first".to_owned(),
            "no runtime timestamp".to_owned(),
        ];

        lines.sort_by(|left, right| log_timestamp_order(left, right));

        assert_eq!(
            lines,
            [
                "2026-09-14T09:00:01Z [payments/api-0 · server] first",
                "2026-09-14T09:00:01Z [payments/worker-0 · worker] first",
                "2026-09-14T09:00:02Z [payments/worker-0 · worker] later",
                "no runtime timestamp",
            ]
        );
    }

    #[test]
    fn timestamp_order_parses_fractional_rfc3339_timestamps() {
        let mut lines = vec![
            "2026-09-14T09:00:00.1Z [payments/api-0 · server] later".to_owned(),
            "2026-09-14T09:00:00Z [payments/worker-0 · worker] earlier".to_owned(),
        ];

        lines.sort_by(|left, right| log_timestamp_order(left, right));

        assert_eq!(
            lines,
            [
                "2026-09-14T09:00:00Z [payments/worker-0 · worker] earlier",
                "2026-09-14T09:00:00.1Z [payments/api-0 · server] later",
            ]
        );
    }

    #[test]
    fn history_spool_persists_length_delimited_log_records() -> anyhow::Result<()> {
        let mut spool = HistorySpool::new()?;
        spool.append(vec![
            "2026-09-14T09:00:00Z first line".to_owned(),
            "second\nline remains one record".to_owned(),
        ])?;

        let spool_lines = spool.total_lines;
        let mut reader = spool.into_reader(spool_lines)?;
        assert_eq!(
            read_spooled_line(&mut reader)?,
            Some("2026-09-14T09:00:00Z first line".to_owned())
        );
        assert_eq!(
            read_spooled_line(&mut reader)?,
            Some("second\nline remains one record".to_owned())
        );
        assert_eq!(read_spooled_line(&mut reader)?, None);
        Ok(())
    }

    #[test]
    fn history_spools_merge_chronologically_without_loading_them_all() -> anyhow::Result<()> {
        let mut api = HistorySpool::new()?;
        api.append(vec![
            "2026-09-14T09:00:01Z [payments/api-0 · server] api first".to_owned(),
            "2026-09-14T09:00:03Z [payments/api-0 · server] api third".to_owned(),
        ])?;
        let mut worker = HistorySpool::new()?;
        worker.append(vec![
            "2026-09-14T09:00:02Z [payments/worker-0 · worker] worker second".to_owned(),
            "2026-09-14T09:00:04Z [payments/worker-0 · worker] worker fourth".to_owned(),
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
                line: read_spooled_line(reader)?.expect("source has a first line"),
                source_index,
            });
        }

        let mut merged = Vec::new();
        while let Some(line) = next_history_merge_line(&mut readers, &mut pending)? {
            merged.push(line);
        }
        assert_eq!(
            merged,
            [
                "2026-09-14T09:00:01Z [payments/api-0 · server] api first",
                "2026-09-14T09:00:02Z [payments/worker-0 · worker] worker second",
                "2026-09-14T09:00:03Z [payments/api-0 · server] api third",
                "2026-09-14T09:00:04Z [payments/worker-0 · worker] worker fourth",
            ]
        );
        Ok(())
    }

    #[test]
    fn history_overlap_only_removes_a_source_boundary_suffix() -> anyhow::Result<()> {
        let mut history = HistorySpool::new()?;
        history.append(vec![
            "2026-09-14T09:00:00Z [payments/api-0 · server] before".to_owned(),
            "2026-09-14T09:00:01Z [payments/api-0 · server] overlap one".to_owned(),
            "2026-09-14T09:00:02Z [payments/api-0 · server] overlap two".to_owned(),
        ])?;
        let mut live = HistorySpool::new()?;
        live.append(vec![
            "2026-09-14T09:00:01Z [payments/api-0 · server] overlap one".to_owned(),
            "2026-09-14T09:00:02Z [payments/api-0 · server] overlap two".to_owned(),
            "2026-09-14T09:00:03Z [payments/api-0 · server] after".to_owned(),
        ])?;

        assert_eq!(history.overlap_with_live_prefix(&live)?, 2);
        Ok(())
    }

    #[test]
    fn live_lines_continue_after_history_spools_move_into_the_merge() -> anyhow::Result<()> {
        let service = LogStoreService::default();
        let appender = InterleavedAppender::new(service.appender(), 1, 1)?;
        appender
            .history
            .lock()
            .expect("history lock is available")
            .clear();

        appender.capture_live_prefix(0, "2026-09-14T09:00:00Z late live line")?;
        Ok(())
    }
}
