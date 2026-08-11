//! Disk-backed storage and search for native pod-log windows.
//!
//! The Kubernetes worker streams directly into this spool through a bounded
//! ingress handle. The UI only consumes coalesced progress and paged results.

use crate::ansi::{AnsiStyleSpan, parse_kubernetes_log_line};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use tempfile::NamedTempFile;

pub(crate) const LOG_PAGE_SIZE: usize = 256;

#[derive(Debug, Clone, Copy)]
pub(crate) struct LogStoreConfig {
    pub(crate) page_size: usize,
    pub(crate) command_channel_capacity: usize,
    pub(crate) result_channel_capacity: usize,
    pub(crate) search_progress_interval: usize,
}

impl Default for LogStoreConfig {
    fn default() -> Self {
        Self {
            page_size: LOG_PAGE_SIZE,
            command_channel_capacity: 128,
            result_channel_capacity: 128,
            search_progress_interval: 2048,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LogPageRow {
    pub(crate) display_row: usize,
    pub(crate) line_index: usize,
    pub(crate) timestamp: Option<String>,
    pub(crate) text: String,
    pub(crate) style_spans: Vec<AnsiStyleSpan>,
    pub(crate) match_ranges: Vec<(usize, usize)>,
}

#[derive(Debug)]
pub(crate) enum LogStoreResult {
    Updated {
        window_id: u64,
        total_lines: usize,
        completed_search: Option<(u64, usize)>,
        /// Parsed rows from the live tail. The UI can draw these immediately
        /// while a normal page request catches up from the disk spool.
        appended_rows: Vec<LogPageRow>,
        /// Number of older records written to the history spool so far.
        backfill_lines: Option<usize>,
    },
    Rebased {
        window_id: u64,
        total_lines: usize,
        /// Maps a row from the pre-rebase live segment into the combined
        /// logical index, keeping the visible log record in place.
        live_start: usize,
        /// Number of records supplied by the completed history request.
        history_lines: usize,
    },
    SearchProgress {
        window_id: u64,
        generation: u64,
        scanned_lines: usize,
        total_lines: usize,
        match_count: usize,
    },
    SearchCompleted {
        window_id: u64,
        generation: u64,
        match_count: usize,
    },
    PageLoaded {
        window_id: u64,
        generation: u64,
        filter_matches: bool,
        page_start: usize,
        total_rows: usize,
        rows: Vec<LogPageRow>,
    },
    MatchResolved {
        window_id: u64,
        generation: u64,
        match_row: usize,
        line_index: usize,
    },
    Copied {
        window_id: u64,
        selection_generation: u64,
        text: String,
    },
    Failed {
        window_id: u64,
        error: String,
    },
}

#[derive(Debug)]
enum Command {
    Open {
        window_id: u64,
    },
    Append {
        window_id: u64,
        lines: Vec<String>,
    },
    AppendBackfill {
        window_id: u64,
        lines: Vec<String>,
    },
    CompleteBackfill {
        window_id: u64,
    },
    Search {
        window_id: u64,
        generation: u64,
        query: String,
        regex_mode: bool,
    },
    LoadPage {
        window_id: u64,
        generation: u64,
        filter_matches: bool,
        page_start: usize,
    },
    ResolveMatch {
        window_id: u64,
        generation: u64,
        match_row: usize,
    },
    Copy {
        window_id: u64,
        selection_generation: u64,
        generation: u64,
        filter_matches: bool,
        start_row: usize,
        start_byte: usize,
        end_row: usize,
        end_byte: usize,
        show_line_numbers: bool,
        show_timestamps: bool,
    },
    Close {
        window_id: u64,
    },
    ScanProgress {
        window_id: u64,
        generation: u64,
        scanned_lines: usize,
    },
    ScanMatches {
        window_id: u64,
        generation: u64,
        scanned_lines: usize,
        line_indices: Vec<usize>,
    },
    ScanCompleted {
        window_id: u64,
        generation: u64,
        scanned_lines: usize,
    },
}

/// The UI-facing handle for the dedicated log-store thread.
pub(crate) struct LogStoreService {
    commands: StoreCommandSender,
    appender: LogStoreAppender,
    results: mpsc::Receiver<LogStoreResult>,
    live_updates: Arc<LiveUpdates>,
}

/// Bounded ingestion handle owned by the Kubernetes worker.
///
/// Sending an append waits for space in the disk-spool command queue. The
/// caller therefore stops reading Kubernetes response data while the spool is
/// busy instead of dropping batches on a UI-owned queue.
#[derive(Clone)]
pub(crate) struct LogStoreAppender {
    live_commands: StoreCommandSender,
    backfill_commands: StoreCommandSender,
}

impl LogStoreAppender {
    pub(crate) async fn append(&self, window_id: u64, lines: Vec<String>) -> anyhow::Result<()> {
        if lines.is_empty() {
            return Ok(());
        }
        let commands = self.live_commands.clone();
        tokio::task::spawn_blocking(move || {
            commands
                .send(Command::Append { window_id, lines })
                .map_err(|_| anyhow::anyhow!("Log storage stopped before the stream finished"))
        })
        .await
        .map_err(|error| anyhow::anyhow!("Log storage task failed: {error}"))?
    }

    fn try_append(&self, window_id: u64, lines: Vec<String>) -> bool {
        self.live_commands
            .try_send(Command::Append { window_id, lines })
            .is_ok()
    }

    pub(crate) async fn append_backfill(
        &self,
        window_id: u64,
        lines: Vec<String>,
    ) -> anyhow::Result<()> {
        if lines.is_empty() {
            return Ok(());
        }
        let commands = self.backfill_commands.clone();
        tokio::task::spawn_blocking(move || {
            commands
                .send(Command::AppendBackfill { window_id, lines })
                .map_err(|_| anyhow::anyhow!("Log storage stopped before backfill finished"))
        })
        .await
        .map_err(|error| anyhow::anyhow!("Log storage task failed: {error}"))?
    }

    pub(crate) async fn complete_backfill(&self, window_id: u64) -> anyhow::Result<()> {
        let commands = self.backfill_commands.clone();
        tokio::task::spawn_blocking(move || {
            commands
                .send(Command::CompleteBackfill { window_id })
                .map_err(|_| anyhow::anyhow!("Log storage stopped before backfill completed"))
        })
        .await
        .map_err(|error| anyhow::anyhow!("Log storage task failed: {error}"))?
    }
}

/// A bounded command lane with a coalesced wake-up for the storage thread.
///
/// The append lane remains bounded, so the Kubernetes stream still applies
/// backpressure. The separate control lane lets page reads overtake queued
/// appends without waking the store in a polling loop while it is idle.
#[derive(Clone)]
struct StoreCommandSender {
    commands: mpsc::SyncSender<Command>,
    wake: mpsc::Sender<()>,
    work_pending: Arc<AtomicBool>,
}

impl StoreCommandSender {
    fn send(&self, command: Command) -> Result<(), mpsc::SendError<Command>> {
        self.commands.send(command)?;
        self.notify();
        Ok(())
    }

    fn try_send(&self, command: Command) -> Result<(), mpsc::TrySendError<Command>> {
        self.commands.try_send(command)?;
        self.notify();
        Ok(())
    }

    fn notify(&self) {
        if !self.work_pending.swap(true, Ordering::AcqRel) {
            let _ = self.wake.send(());
        }
    }
}

const MAX_COALESCED_LIVE_ROWS: usize = 2 * LOG_PAGE_SIZE;

#[derive(Default)]
struct PendingLiveUpdate {
    total_lines: usize,
    completed_search: Option<(u64, usize)>,
    appended_rows: Vec<LogPageRow>,
    backfill_lines: Option<usize>,
}

struct LiveUpdates {
    updates: std::sync::Mutex<HashMap<u64, PendingLiveUpdate>>,
    repaint_requested: AtomicBool,
    repaint_context: Option<egui::Context>,
}

impl LiveUpdates {
    fn new(repaint_context: Option<egui::Context>) -> Self {
        Self {
            updates: std::sync::Mutex::new(HashMap::new()),
            repaint_requested: AtomicBool::new(false),
            repaint_context,
        }
    }

    fn publish_live(&self, window_id: u64, summary: AppendSummary) {
        let mut updates = self
            .updates
            .lock()
            .expect("live update lock is not poisoned");
        let update = updates.entry(window_id).or_default();
        update.total_lines = summary.total_lines;
        update.completed_search = summary.completed_search;
        update.appended_rows.extend(summary.appended_rows);
        if update.appended_rows.len() > MAX_COALESCED_LIVE_ROWS {
            let excess = update.appended_rows.len() - MAX_COALESCED_LIVE_ROWS;
            update.appended_rows.drain(..excess);
        }
        drop(updates);
        self.request_repaint();
    }

    fn publish_backfill(&self, window_id: u64, total_lines: usize, backfill_lines: usize) {
        let mut updates = self
            .updates
            .lock()
            .expect("live update lock is not poisoned");
        let update = updates.entry(window_id).or_default();
        update.total_lines = total_lines;
        update.backfill_lines = Some(backfill_lines);
        drop(updates);
        self.request_repaint();
    }

    fn request_repaint(&self) {
        if !self.repaint_requested.swap(true, Ordering::AcqRel)
            && let Some(context) = &self.repaint_context
        {
            context.request_repaint();
        }
    }

    fn take_next(&self) -> Option<LogStoreResult> {
        let mut updates = self
            .updates
            .lock()
            .expect("live update lock is not poisoned");
        let window_id = updates.keys().next().copied()?;
        let update = updates
            .remove(&window_id)
            .map(|update| LogStoreResult::Updated {
                window_id,
                total_lines: update.total_lines,
                completed_search: update.completed_search,
                appended_rows: update.appended_rows,
                backfill_lines: update.backfill_lines,
            });
        if updates.is_empty() {
            // Reset while holding the lock so a concurrent publisher either
            // sees the reset value or is included in this drain.
            self.repaint_requested.store(false, Ordering::Release);
        }
        update
    }

    fn remove(&self, window_id: u64) {
        let mut updates = self
            .updates
            .lock()
            .expect("live update lock is not poisoned");
        updates.remove(&window_id);
        if updates.is_empty() {
            self.repaint_requested.store(false, Ordering::Release);
        }
    }
}

impl Default for LogStoreService {
    fn default() -> Self {
        Self::new(LogStoreConfig::default())
    }
}

impl LogStoreService {
    pub(crate) fn new(config: LogStoreConfig) -> Self {
        Self::new_with_repaint_context(config, None)
    }

    pub(crate) fn with_repaint_context(context: egui::Context) -> Self {
        Self::new_with_repaint_context(LogStoreConfig::default(), Some(context))
    }

    fn new_with_repaint_context(
        config: LogStoreConfig,
        repaint_context: Option<egui::Context>,
    ) -> Self {
        let config = LogStoreConfig {
            page_size: config.page_size.max(1),
            command_channel_capacity: config.command_channel_capacity.max(1),
            result_channel_capacity: config.result_channel_capacity.max(1),
            search_progress_interval: config.search_progress_interval.max(1),
        };
        let (commands, control_receiver) = mpsc::sync_channel(config.command_channel_capacity);
        let (live_commands, live_receiver) = mpsc::sync_channel(config.command_channel_capacity);
        let (backfill_commands, backfill_receiver) =
            mpsc::sync_channel(config.command_channel_capacity);
        let (scan_commands, scan_receiver) = mpsc::sync_channel(config.command_channel_capacity);
        let (wake_sender, wake_receiver) = mpsc::channel();
        let work_pending = Arc::new(AtomicBool::new(false));
        let make_sender = |commands| StoreCommandSender {
            commands,
            wake: wake_sender.clone(),
            work_pending: work_pending.clone(),
        };
        let commands = make_sender(commands);
        let appender = LogStoreAppender {
            live_commands: make_sender(live_commands),
            backfill_commands: make_sender(backfill_commands),
        };
        let scan_sender = make_sender(scan_commands);
        let (result_sender, results) = mpsc::sync_channel(config.result_channel_capacity);
        let result_sender = LogStoreResultSender::new(result_sender, repaint_context);
        let live_updates = Arc::new(LiveUpdates::new(result_sender.repaint_context.clone()));
        let store_live_updates = live_updates.clone();
        thread::Builder::new()
            .name("pod-log-store".to_owned())
            .spawn(move || {
                run_store(
                    control_receiver,
                    live_receiver,
                    backfill_receiver,
                    scan_receiver,
                    wake_receiver,
                    work_pending,
                    scan_sender,
                    result_sender,
                    store_live_updates,
                    config,
                )
            })
            .expect("Failed to start pod log store thread");
        Self {
            commands,
            appender,
            results,
            live_updates,
        }
    }
    pub(crate) fn open(&self, window_id: u64) -> bool {
        self.send(Command::Open { window_id })
    }

    pub(crate) fn append(&self, window_id: u64, lines: Vec<String>) -> bool {
        if !lines.is_empty() {
            self.appender.try_append(window_id, lines)
        } else {
            true
        }
    }

    pub(crate) fn appender(&self) -> LogStoreAppender {
        self.appender.clone()
    }

    pub(crate) fn search(
        &self,
        window_id: u64,
        generation: u64,
        query: String,
        regex_mode: bool,
    ) -> bool {
        self.send(Command::Search {
            window_id,
            generation,
            query,
            regex_mode,
        })
    }

    pub(crate) fn load_page(
        &self,
        window_id: u64,
        generation: u64,
        filter_matches: bool,
        page_start: usize,
    ) -> bool {
        self.send(Command::LoadPage {
            window_id,
            generation,
            filter_matches,
            page_start,
        })
    }

    pub(crate) fn close(&self, window_id: u64) -> bool {
        self.send(Command::Close { window_id })
    }

    pub(crate) fn resolve_match(&self, window_id: u64, generation: u64, match_row: usize) -> bool {
        self.send(Command::ResolveMatch {
            window_id,
            generation,
            match_row,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn copy(
        &self,
        window_id: u64,
        selection_generation: u64,
        generation: u64,
        filter_matches: bool,
        start_row: usize,
        start_byte: usize,
        end_row: usize,
        end_byte: usize,
        show_line_numbers: bool,
        show_timestamps: bool,
    ) -> bool {
        self.send(Command::Copy {
            window_id,
            selection_generation,
            generation,
            filter_matches,
            start_row,
            start_byte,
            end_row,
            end_byte,
            show_line_numbers,
            show_timestamps,
        })
    }

    pub(crate) fn try_next_result(&self) -> Option<LogStoreResult> {
        // Page and rebase results must not sit behind an indefinitely busy
        // live stream. Progress updates are coalesced and can safely wait.
        self.results
            .try_recv()
            .ok()
            .or_else(|| self.live_updates.take_next())
    }

    fn send(&self, command: Command) -> bool {
        self.commands.try_send(command).is_ok()
    }
}

fn run_store(
    control_receiver: mpsc::Receiver<Command>,
    live_receiver: mpsc::Receiver<Command>,
    backfill_receiver: mpsc::Receiver<Command>,
    scan_receiver: mpsc::Receiver<Command>,
    wake_receiver: mpsc::Receiver<()>,
    work_pending: Arc<AtomicBool>,
    scan_sender: StoreCommandSender,
    result_sender: LogStoreResultSender,
    live_updates: Arc<LiveUpdates>,
    config: LogStoreConfig,
) {
    let mut stores = HashMap::<u64, LogStore>::new();
    let mut closed = HashSet::new();
    loop {
        let command = next_store_command(
            &control_receiver,
            &live_receiver,
            &backfill_receiver,
            &scan_receiver,
        );
        let command = if let Some(command) = command {
            command
        } else {
            // Clear the coalesced notification only after draining all lanes.
            // If a command arrives while doing so, the second check handles it;
            // if it arrives afterwards, its sender wakes us.
            work_pending.store(false, Ordering::Release);
            if let Some(command) = next_store_command(
                &control_receiver,
                &live_receiver,
                &backfill_receiver,
                &scan_receiver,
            ) {
                work_pending.store(true, Ordering::Release);
                command
            } else if wake_receiver.recv().is_ok() {
                continue;
            } else {
                break;
            }
        };
        match command {
            Command::Open { window_id } => {
                closed.remove(&window_id);
                if let Err(error) = stores
                    .entry(window_id)
                    .or_insert_with(LogStore::new)
                    .init_error()
                {
                    send_failure(&result_sender, window_id, error);
                    stores.remove(&window_id);
                }
            }
            Command::Append { window_id, lines } => {
                if closed.contains(&window_id) {
                    continue;
                }
                let store = stores.entry(window_id).or_insert_with(LogStore::new);
                match store.append(lines) {
                    Ok(summary) => live_updates.publish_live(window_id, summary),
                    Err(error) => send_failure(&result_sender, window_id, error),
                }
            }
            Command::AppendBackfill { window_id, lines } => {
                if closed.contains(&window_id) {
                    continue;
                }
                let store = stores.entry(window_id).or_insert_with(LogStore::new);
                match store.append_backfill(lines) {
                    Ok(()) => {
                        let backfill_lines = store
                            .backfill
                            .as_ref()
                            .expect("backfill store was initialized")
                            .total_lines;
                        live_updates.publish_backfill(
                            window_id,
                            store.visible_total_lines(),
                            backfill_lines,
                        );
                    }
                    Err(error) => send_failure(&result_sender, window_id, error),
                }
            }
            Command::CompleteBackfill { window_id } => {
                let Some(store) = stores.get_mut(&window_id) else {
                    continue;
                };
                match store.complete_backfill() {
                    Ok(Some(rebase)) => {
                        // A rebase replaces the logical row space. Do not let
                        // a coalesced pre-rebase tail update arrive after it.
                        live_updates.remove(window_id);
                        send_result(
                            &result_sender,
                            LogStoreResult::Rebased {
                                window_id,
                                total_lines: store.visible_total_lines(),
                                live_start: rebase.live_start,
                                history_lines: rebase.history_lines,
                            },
                        )
                    }
                    Ok(None) => {}
                    Err(error) => send_failure(&result_sender, window_id, error),
                }
            }
            Command::Search {
                window_id,
                generation,
                query,
                regex_mode,
            } => {
                let Some(store) = stores.get_mut(&window_id) else {
                    continue;
                };
                match store.start_search(
                    window_id,
                    generation,
                    query,
                    regex_mode,
                    scan_sender.clone(),
                    config.search_progress_interval,
                ) {
                    Ok(()) => {}
                    Err(error) => send_failure(&result_sender, window_id, error),
                }
            }
            Command::LoadPage {
                window_id,
                generation,
                filter_matches,
                page_start,
            } => {
                let Some(store) = stores.get_mut(&window_id) else {
                    continue;
                };
                match store.page(generation, filter_matches, page_start, config.page_size) {
                    Ok((total_rows, rows)) => send_result(
                        &result_sender,
                        LogStoreResult::PageLoaded {
                            window_id,
                            generation,
                            filter_matches,
                            page_start,
                            total_rows,
                            rows,
                        },
                    ),
                    Err(error) => send_failure(&result_sender, window_id, error),
                }
            }
            Command::ResolveMatch {
                window_id,
                generation,
                match_row,
            } => {
                let Some(store) = stores.get_mut(&window_id) else {
                    continue;
                };
                match store.match_line(generation, match_row) {
                    Ok(Some(line_index)) => send_result(
                        &result_sender,
                        LogStoreResult::MatchResolved {
                            window_id,
                            generation,
                            match_row,
                            line_index,
                        },
                    ),
                    Ok(None) => {}
                    Err(error) => send_failure(&result_sender, window_id, error),
                }
            }
            Command::Copy {
                window_id,
                selection_generation,
                generation,
                filter_matches,
                start_row,
                start_byte,
                end_row,
                end_byte,
                show_line_numbers,
                show_timestamps,
            } => {
                let Some(store) = stores.get_mut(&window_id) else {
                    continue;
                };
                match store.copy_range(
                    generation,
                    filter_matches,
                    start_row,
                    start_byte,
                    end_row,
                    end_byte,
                    show_line_numbers,
                    show_timestamps,
                ) {
                    Ok(text) => send_result(
                        &result_sender,
                        LogStoreResult::Copied {
                            window_id,
                            selection_generation,
                            text,
                        },
                    ),
                    Err(error) => send_failure(&result_sender, window_id, error),
                }
            }
            Command::Close { window_id } => {
                stores.remove(&window_id);
                closed.insert(window_id);
                live_updates.remove(window_id);
            }
            Command::ScanProgress {
                window_id,
                generation,
                scanned_lines,
            } => {
                let Some(store) = stores.get(&window_id) else {
                    continue;
                };
                if store.search_generation() == Some(generation) {
                    send_result(
                        &result_sender,
                        LogStoreResult::SearchProgress {
                            window_id,
                            generation,
                            scanned_lines,
                            total_lines: store.visible_total_lines(),
                            match_count: store
                                .search
                                .as_ref()
                                .map_or(0, |search| search.match_count),
                        },
                    );
                }
            }
            Command::ScanMatches {
                window_id,
                generation,
                scanned_lines,
                line_indices,
            } => {
                let Some(store) = stores.get_mut(&window_id) else {
                    continue;
                };
                if store.search_generation() != Some(generation) {
                    continue;
                }
                match store.append_search_matches(line_indices) {
                    Ok(match_count) => send_result(
                        &result_sender,
                        LogStoreResult::SearchProgress {
                            window_id,
                            generation,
                            scanned_lines,
                            total_lines: store.visible_total_lines(),
                            match_count,
                        },
                    ),
                    Err(error) => send_failure(&result_sender, window_id, error),
                }
            }
            Command::ScanCompleted {
                window_id,
                generation,
                scanned_lines,
            } => {
                let Some(store) = stores.get_mut(&window_id) else {
                    continue;
                };
                if store.search_generation() != Some(generation) {
                    continue;
                }
                match store.finish_search(scanned_lines) {
                    Ok(match_count) => send_result(
                        &result_sender,
                        LogStoreResult::SearchCompleted {
                            window_id,
                            generation,
                            match_count,
                        },
                    ),
                    Err(error) => send_failure(&result_sender, window_id, error),
                }
            }
        }
    }
}

fn next_store_command(
    control_receiver: &mpsc::Receiver<Command>,
    live_receiver: &mpsc::Receiver<Command>,
    backfill_receiver: &mpsc::Receiver<Command>,
    scan_receiver: &mpsc::Receiver<Command>,
) -> Option<Command> {
    control_receiver
        .try_recv()
        .ok()
        .or_else(|| live_receiver.try_recv().ok())
        .or_else(|| backfill_receiver.try_recv().ok())
        .or_else(|| scan_receiver.try_recv().ok())
}

#[derive(Clone)]
struct LogStoreResultSender {
    sender: mpsc::SyncSender<LogStoreResult>,
    repaint_context: Option<egui::Context>,
}

impl LogStoreResultSender {
    fn new(
        sender: mpsc::SyncSender<LogStoreResult>,
        repaint_context: Option<egui::Context>,
    ) -> Self {
        Self {
            sender,
            repaint_context,
        }
    }

    fn send(&self, result: LogStoreResult) -> Result<(), mpsc::SendError<LogStoreResult>> {
        self.sender.send(result)?;
        if let Some(context) = &self.repaint_context {
            context.request_repaint();
        }
        Ok(())
    }
}

fn send_result(sender: &LogStoreResultSender, result: LogStoreResult) {
    let _ = sender.send(result);
}

fn send_failure(sender: &LogStoreResultSender, window_id: u64, error: impl std::fmt::Display) {
    send_result(
        sender,
        LogStoreResult::Failed {
            window_id,
            error: error.to_string(),
        },
    );
}

struct LogStore {
    data: Option<NamedTempFile>,
    offsets: Option<NamedTempFile>,
    backfill: Option<BackfillStore>,
    rebase: Option<LogRebase>,
    total_lines: usize,
    search: Option<SearchState>,
    initialization_error: Option<String>,
}

/// The completed historical response stays in a separate pair of spool files.
/// A rebase joins it with the live segment logically, without copying either
/// log body through memory or rewriting the live spool.
struct BackfillStore {
    data: NamedTempFile,
    offsets: NamedTempFile,
    total_lines: usize,
}

#[derive(Clone, Copy)]
struct LogRebase {
    history_lines: usize,
    live_start: usize,
}

struct LogicalReader {
    live_data: File,
    live_offsets: File,
    backfill: Option<(File, File)>,
    rebase: Option<LogRebase>,
}

impl LogicalReader {
    fn read_line(&mut self, line_index: usize) -> anyhow::Result<String> {
        if let Some(rebase) = self.rebase {
            if line_index < rebase.history_lines {
                let (data, offsets) = self
                    .backfill
                    .as_mut()
                    .expect("rebased reader retains a history segment");
                return read_line_from(data, offsets, line_index);
            }
            return read_line_from(
                &mut self.live_data,
                &mut self.live_offsets,
                line_index - rebase.history_lines + rebase.live_start,
            );
        }
        read_line_from(&mut self.live_data, &mut self.live_offsets, line_index)
    }
}

struct SearchState {
    generation: u64,
    matcher: Regex,
    cancellation: Arc<AtomicBool>,
    match_offsets: Option<NamedTempFile>,
    match_count: usize,
    complete: bool,
}

struct AppendSummary {
    total_lines: usize,
    completed_search: Option<(u64, usize)>,
    appended_rows: Vec<LogPageRow>,
}

impl BackfillStore {
    fn new() -> anyhow::Result<Self> {
        Ok(Self {
            data: NamedTempFile::new()?,
            offsets: NamedTempFile::new()?,
            total_lines: 0,
        })
    }

    fn append(&mut self, lines: Vec<String>) -> anyhow::Result<()> {
        let appended_lines = lines.len();
        let mut data = self.data.reopen()?;
        let mut offsets = self.offsets.reopen()?;
        let mut next_offset = data.seek(SeekFrom::End(0))?;
        let mut line_offsets = Vec::with_capacity(lines.len());
        for line in lines {
            let bytes = line.as_bytes();
            let length = u32::try_from(bytes.len())
                .map_err(|_| anyhow::anyhow!("A log line exceeds 4 GiB"))?;
            line_offsets.push(next_offset);
            data.write_all(&length.to_le_bytes())?;
            data.write_all(bytes)?;
            next_offset += u64::from(length) + 4;
        }
        data.flush()?;
        offsets.seek(SeekFrom::End(0))?;
        for offset in line_offsets {
            offsets.write_all(&offset.to_le_bytes())?;
        }
        offsets.flush()?;
        self.total_lines += appended_lines;
        Ok(())
    }

    fn read_line(&self, line_index: usize) -> anyhow::Result<String> {
        let mut data = self.data.reopen()?;
        let mut offsets = self.offsets.reopen()?;
        read_line_from(&mut data, &mut offsets, line_index)
    }
}

impl LogStore {
    fn new() -> Self {
        match (NamedTempFile::new(), NamedTempFile::new()) {
            (Ok(data), Ok(offsets)) => Self {
                data: Some(data),
                offsets: Some(offsets),
                backfill: None,
                rebase: None,
                total_lines: 0,
                search: None,
                initialization_error: None,
            },
            (data, offsets) => Self {
                data: None,
                offsets: None,
                backfill: None,
                rebase: None,
                total_lines: 0,
                search: None,
                initialization_error: Some(format!(
                    "Unable to create temporary log storage: {}",
                    data.err()
                        .or_else(|| offsets.err())
                        .expect("one tempfile creation must fail")
                )),
            },
        }
    }

    fn init_error(&self) -> Result<(), &str> {
        self.initialization_error.as_deref().map_or(Ok(()), Err)
    }

    fn data(&self) -> Result<&NamedTempFile, anyhow::Error> {
        self.data.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                self.initialization_error
                    .clone()
                    .unwrap_or_else(|| "Log store is unavailable".to_owned())
            )
        })
    }

    fn offsets(&self) -> Result<&NamedTempFile, anyhow::Error> {
        self.offsets.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                self.initialization_error
                    .clone()
                    .unwrap_or_else(|| "Log store is unavailable".to_owned())
            )
        })
    }

    fn visible_total_lines(&self) -> usize {
        self.rebase.map_or(self.total_lines, |rebase| {
            rebase.history_lines + self.total_lines.saturating_sub(rebase.live_start)
        })
    }

    fn append_backfill(&mut self, lines: Vec<String>) -> anyhow::Result<()> {
        if self.backfill.is_none() {
            self.backfill = Some(BackfillStore::new()?);
        }
        self.backfill
            .as_mut()
            .expect("backfill store was initialized")
            .append(lines)
    }

    fn logical_reader(&self) -> anyhow::Result<LogicalReader> {
        Ok(LogicalReader {
            live_data: self.data()?.reopen()?,
            live_offsets: self.offsets()?.reopen()?,
            backfill: self
                .backfill
                .as_ref()
                .map(|backfill| {
                    Ok::<_, anyhow::Error>((backfill.data.reopen()?, backfill.offsets.reopen()?))
                })
                .transpose()?,
            rebase: self.rebase,
        })
    }

    fn complete_backfill(&mut self) -> anyhow::Result<Option<LogRebase>> {
        let Some(backfill) = &self.backfill else {
            return Ok(None);
        };
        if backfill.total_lines == 0 {
            return Ok(None);
        }

        let overlap = self.find_backfill_overlap(backfill)?;
        let rebase = LogRebase {
            history_lines: backfill.total_lines,
            live_start: overlap,
        };
        if let Some(search) = &self.search {
            search.cancellation.store(true, Ordering::Relaxed);
        }
        self.search = None;
        self.rebase = Some(rebase);
        Ok(Some(rebase))
    }

    fn find_backfill_overlap(&self, backfill: &BackfillStore) -> anyhow::Result<usize> {
        // The first live records are the requested tail. Comparing ordered raw
        // records (which include Kubernetes timestamps) avoids treating
        // repeated messages with the same timestamp as an overlap.
        const MAX_OVERLAP_LINES: usize = 4 * LOG_PAGE_SIZE;
        let max_overlap = self
            .total_lines
            .min(backfill.total_lines)
            .min(MAX_OVERLAP_LINES);
        for overlap in (1..=max_overlap).rev() {
            let history_start = backfill.total_lines - overlap;
            let mut matches = true;
            for offset in 0..overlap {
                if backfill.read_line(history_start + offset)? != self.read_live_line(offset)? {
                    matches = false;
                    break;
                }
            }
            if matches {
                return Ok(overlap);
            }
        }
        // The fallback deliberately keeps both segments. It may duplicate a
        // small boundary, but it cannot discard recent live output.
        Ok(0)
    }

    fn append(&mut self, lines: Vec<String>) -> anyhow::Result<AppendSummary> {
        self.init_error()
            .map_err(|error| anyhow::Error::msg(error.to_owned()))?;
        let mut data = self.data()?.reopen()?;
        let mut offsets = self.offsets()?.reopen()?;
        let mut next_offset = data.seek(SeekFrom::End(0))?;
        let first_line_index = self.visible_total_lines();
        let completed_matcher = self
            .search
            .as_ref()
            .filter(|search| search.complete)
            .map(|search| search.matcher.clone());
        let mut line_offsets = Vec::with_capacity(lines.len());
        let mut matching_line_indices = Vec::new();
        let mut appended_rows = Vec::with_capacity(lines.len());

        for (relative_line_index, line) in lines.iter().enumerate() {
            let bytes = line.as_bytes();
            let length = u32::try_from(bytes.len())
                .map_err(|_| anyhow::anyhow!("A log line exceeds 4 GiB"))?;
            line_offsets.push(next_offset);
            data.write_all(&length.to_le_bytes())?;
            data.write_all(bytes)?;
            next_offset += u64::from(length) + 4;
            let visible_line = parse_kubernetes_log_line(line);
            let match_ranges = completed_matcher.as_ref().map_or_else(Vec::new, |matcher| {
                matcher
                    .find_iter(&visible_line.line.text)
                    .map(|matched| (matched.start(), matched.end()))
                    .collect()
            });
            if completed_matcher
                .as_ref()
                .is_some_and(|_| !match_ranges.is_empty())
            {
                matching_line_indices.push(first_line_index + relative_line_index);
            }
            appended_rows.push(LogPageRow {
                display_row: first_line_index + relative_line_index,
                line_index: first_line_index + relative_line_index,
                timestamp: visible_line.timestamp,
                text: visible_line.line.text,
                style_spans: visible_line.line.style_spans,
                match_ranges,
            });
        }
        data.flush()?;
        // The index is published only after the complete batch of records, so
        // readers can never observe a partially spooled logical line.
        offsets.seek(SeekFrom::End(0))?;
        for offset in line_offsets {
            offsets.write_all(&offset.to_le_bytes())?;
        }
        offsets.flush()?;
        self.total_lines += lines.len();
        if let Some(search) = &mut self.search
            && search.complete
            && !matching_line_indices.is_empty()
        {
            let mut matches = search
                .match_offsets
                .as_ref()
                .expect("completed search has an index")
                .reopen()?;
            matches.seek(SeekFrom::End(0))?;
            for line_index in matching_line_indices {
                matches.write_all(&line_index.to_le_bytes())?;
                search.match_count += 1;
            }
            matches.flush()?;
        }
        Ok(AppendSummary {
            total_lines: self.visible_total_lines(),
            completed_search: self
                .search
                .as_ref()
                .filter(|search| search.complete)
                .map(|search| (search.generation, search.match_count)),
            appended_rows,
        })
    }

    fn start_search(
        &mut self,
        window_id: u64,
        generation: u64,
        query: String,
        regex_mode: bool,
        sender: StoreCommandSender,
        search_progress_interval: usize,
    ) -> anyhow::Result<()> {
        if let Some(search) = &self.search {
            search.cancellation.store(true, Ordering::Relaxed);
        }
        if query.is_empty() {
            self.search = None;
            return Ok(());
        }
        let pattern = if regex_mode {
            query
        } else {
            regex::escape(&query)
        };
        let matcher = regex::RegexBuilder::new(&pattern)
            .case_insensitive(true)
            .build()?;
        let cancellation = Arc::new(AtomicBool::new(false));
        let scan_lines = self.visible_total_lines();
        let reader = self.logical_reader()?;
        self.search = Some(SearchState {
            generation,
            matcher: matcher.clone(),
            cancellation: cancellation.clone(),
            match_offsets: Some(NamedTempFile::new()?),
            match_count: 0,
            complete: false,
        });
        thread::Builder::new()
            .name("pod-log-search".to_owned())
            .spawn(move || {
                scan_records(
                    reader,
                    scan_lines,
                    matcher,
                    cancellation,
                    window_id,
                    generation,
                    sender,
                    search_progress_interval,
                )
            })
            .map_err(anyhow::Error::from)?;
        Ok(())
    }

    fn append_search_matches(&mut self, line_indices: Vec<usize>) -> anyhow::Result<usize> {
        let Some(search) = &mut self.search else {
            return Ok(0);
        };
        let mut file = search
            .match_offsets
            .as_ref()
            .expect("search index exists")
            .reopen()?;
        file.seek(SeekFrom::End(0))?;
        for line_index in line_indices {
            file.write_all(&line_index.to_le_bytes())?;
            search.match_count += 1;
        }
        file.flush()?;
        Ok(search.match_count)
    }

    fn finish_search(&mut self, scanned_lines: usize) -> anyhow::Result<usize> {
        let Some(search) = &self.search else {
            return Ok(0);
        };
        let matcher = search.matcher.clone();
        let mut tail_matches = Vec::new();
        // Any lines appended while the background scan ran are searched once
        // here before the index becomes visible.
        for line_index in scanned_lines..self.visible_total_lines() {
            let line = self.read_line(line_index)?;
            if matcher.is_match(&parse_kubernetes_log_line(&line).line.text) {
                tail_matches.push(line_index);
            }
        }
        let search = self.search.as_mut().expect("search state must still exist");
        let matches = search.match_offsets.as_ref().expect("search index exists");
        let mut file = matches.reopen()?;
        file.seek(SeekFrom::End(0))?;
        for line_index in tail_matches {
            file.write_all(&line_index.to_le_bytes())?;
            search.match_count += 1;
        }
        file.flush()?;
        search.complete = true;
        Ok(search.match_count)
    }

    fn page(
        &mut self,
        generation: u64,
        filter_matches: bool,
        page_start: usize,
        page_size: usize,
    ) -> anyhow::Result<(usize, Vec<LogPageRow>)> {
        let (total_rows, matcher, match_offsets) = if filter_matches {
            let Some(search) = &self.search else {
                return Ok((0, Vec::new()));
            };
            if search.generation != generation {
                return Ok((0, Vec::new()));
            }
            (
                search.match_count,
                Some(search.matcher.clone()),
                Some(
                    search
                        .match_offsets
                        .as_ref()
                        .expect("search has index")
                        .reopen()?,
                ),
            )
        } else {
            (
                self.visible_total_lines(),
                self.search
                    .as_ref()
                    .filter(|search| search.generation == generation)
                    .map(|search| search.matcher.clone()),
                None,
            )
        };
        let end = (page_start + page_size).min(total_rows);
        let mut rows = Vec::with_capacity(end.saturating_sub(page_start));
        let mut matching_offsets = match_offsets;
        for display_row in page_start..end {
            let line_index = if filter_matches {
                read_u64_at(
                    matching_offsets
                        .as_mut()
                        .expect("filtered log pages have a match index"),
                    display_row,
                )? as usize
            } else {
                display_row
            };
            let parsed = parse_kubernetes_log_line(&self.read_line(line_index)?);
            let match_ranges = matcher
                .as_ref()
                .map(|matcher| {
                    matcher
                        .find_iter(&parsed.line.text)
                        .map(|range| (range.start(), range.end()))
                        .collect()
                })
                .unwrap_or_default();
            rows.push(LogPageRow {
                display_row,
                line_index,
                timestamp: parsed.timestamp,
                text: parsed.line.text,
                style_spans: parsed.line.style_spans,
                match_ranges,
            });
        }
        Ok((total_rows, rows))
    }

    fn search_generation(&self) -> Option<u64> {
        self.search.as_ref().map(|search| search.generation)
    }

    fn match_line(&self, generation: u64, match_row: usize) -> anyhow::Result<Option<usize>> {
        let Some(search) = &self.search else {
            return Ok(None);
        };
        if search.generation != generation || match_row >= search.match_count {
            return Ok(None);
        }
        let mut offsets = search
            .match_offsets
            .as_ref()
            .expect("search has index")
            .reopen()?;
        Ok(Some(read_u64_at(&mut offsets, match_row)? as usize))
    }

    #[allow(clippy::too_many_arguments)]
    fn copy_range(
        &mut self,
        generation: u64,
        filter_matches: bool,
        start_row: usize,
        start_byte: usize,
        end_row: usize,
        end_byte: usize,
        show_line_numbers: bool,
        show_timestamps: bool,
    ) -> anyhow::Result<String> {
        let total_rows = if filter_matches {
            let Some(search) = &self.search else {
                return Ok(String::new());
            };
            if search.generation != generation {
                return Ok(String::new());
            }
            search.match_count
        } else {
            self.visible_total_lines()
        };
        if start_row >= total_rows || start_row > end_row {
            return Ok(String::new());
        }
        let end_row = end_row.min(total_rows - 1);
        let mut text = String::new();
        let mut match_offsets = filter_matches
            .then(|| {
                self.search
                    .as_ref()
                    .expect("filtered copy has a search")
                    .match_offsets
                    .as_ref()
                    .expect("filtered copy has an index")
                    .reopen()
            })
            .transpose()?;
        for display_row in start_row..=end_row {
            let line_index = if let Some(offsets) = &mut match_offsets {
                read_u64_at(offsets, display_row)? as usize
            } else {
                display_row
            };
            let parsed = parse_kubernetes_log_line(&self.read_line(line_index)?);
            let line = parsed.line.text;
            let start = if display_row == start_row {
                floor_char_boundary(&line, start_byte)
            } else {
                0
            };
            let end = if display_row == end_row {
                floor_char_boundary(&line, end_byte)
            } else {
                line.len()
            };
            if display_row != start_row {
                text.push('\n');
            }
            if show_line_numbers {
                use std::fmt::Write as _;
                write!(text, "{line_index:>6}  ")?;
            }
            if show_timestamps && let Some(timestamp) = parsed.timestamp {
                text.push_str(&timestamp);
                text.push_str("  ");
            }
            if start < end {
                text.push_str(&line[start..end]);
            }
        }
        Ok(text)
    }

    fn read_line(&self, line_index: usize) -> anyhow::Result<String> {
        if let Some(rebase) = self.rebase {
            if line_index < rebase.history_lines {
                return self
                    .backfill
                    .as_ref()
                    .expect("rebased store retains its history segment")
                    .read_line(line_index);
            }
            return self.read_live_line(line_index - rebase.history_lines + rebase.live_start);
        }
        self.read_live_line(line_index)
    }

    fn read_live_line(&self, line_index: usize) -> anyhow::Result<String> {
        let mut data = self.data()?.reopen()?;
        let offset = read_u64_at(&mut self.offsets()?.reopen()?, line_index)?;
        read_line_at(&mut data, offset)
    }
}

fn floor_char_boundary(text: &str, byte_offset: usize) -> usize {
    let mut byte_offset = byte_offset.min(text.len());
    while byte_offset > 0 && !text.is_char_boundary(byte_offset) {
        byte_offset -= 1;
    }
    byte_offset
}

fn read_line_from(
    data: &mut File,
    offsets: &mut File,
    line_index: usize,
) -> anyhow::Result<String> {
    let offset = read_u64_at(offsets, line_index)?;
    read_line_at(data, offset)
}

fn read_line_at(data: &mut File, offset: u64) -> anyhow::Result<String> {
    data.seek(SeekFrom::Start(offset))?;
    let mut length = [0_u8; 4];
    data.read_exact(&mut length)?;
    let mut bytes = vec![0; u32::from_le_bytes(length) as usize];
    data.read_exact(&mut bytes)?;
    String::from_utf8(bytes).map_err(anyhow::Error::from)
}

fn read_u64_at(file: &mut File, index: usize) -> anyhow::Result<u64> {
    file.seek(SeekFrom::Start((index * std::mem::size_of::<u64>()) as u64))?;
    let mut bytes = [0_u8; 8];
    file.read_exact(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

fn scan_records(
    mut reader: LogicalReader,
    scan_lines: usize,
    matcher: Regex,
    cancellation: Arc<AtomicBool>,
    window_id: u64,
    generation: u64,
    sender: StoreCommandSender,
    search_progress_interval: usize,
) {
    let mut scanned_lines = 0;
    let mut match_lines = Vec::new();
    while scanned_lines < scan_lines {
        if cancellation.load(Ordering::Relaxed) {
            return;
        }
        let Ok(line) = reader.read_line(scanned_lines) else {
            return;
        };
        if matcher.is_match(&parse_kubernetes_log_line(&line).line.text) {
            match_lines.push(scanned_lines);
        }
        scanned_lines += 1;
        if scanned_lines % search_progress_interval == 0 {
            if !match_lines.is_empty() {
                let _ = sender.send(Command::ScanMatches {
                    window_id,
                    generation,
                    scanned_lines,
                    line_indices: std::mem::take(&mut match_lines),
                });
            } else {
                let _ = sender.send(Command::ScanProgress {
                    window_id,
                    generation,
                    scanned_lines,
                });
            }
        }
    }
    if cancellation.load(Ordering::Relaxed) {
        return;
    }
    if !match_lines.is_empty() {
        let _ = sender.send(Command::ScanMatches {
            window_id,
            generation,
            scanned_lines,
            line_indices: match_lines,
        });
    }
    let _ = sender.send(Command::ScanCompleted {
        window_id,
        generation,
        scanned_lines,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    #[test]
    fn store_results_request_a_repaint_when_context_is_attached() {
        let context = egui::Context::default();
        let repaint_count = Arc::new(AtomicUsize::new(0));
        let repaint_count_for_callback = repaint_count.clone();
        context.set_request_repaint_callback(move |_| {
            repaint_count_for_callback.fetch_add(1, Ordering::Relaxed);
        });
        let service = LogStoreService::with_repaint_context(context);

        assert!(service.append(1, vec!["line 0".to_owned()]));
        let _ = wait_for(&service, |result| {
            matches!(result, LogStoreResult::Updated { .. })
        });

        let start = std::time::Instant::now();
        while repaint_count.load(Ordering::Relaxed) == 0 {
            assert!(
                start.elapsed() < Duration::from_secs(2),
                "timed out waiting for log-store repaint request"
            );
            thread::yield_now();
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn live_updates_include_parsed_rows_and_backfill_progress() {
        let service = LogStoreService::default();
        service.open(77);
        assert!(service.append(
            77,
            vec!["2026-08-10T12:34:56Z \u{1b}[31merror\u{1b}[0m".to_owned()]
        ));

        let LogStoreResult::Updated {
            total_lines,
            appended_rows,
            backfill_lines,
            ..
        } = wait_for(&service, |result| {
            matches!(result, LogStoreResult::Updated { window_id: 77, .. })
        })
        else {
            unreachable!()
        };
        assert_eq!(total_lines, 1);
        assert_eq!(backfill_lines, None);
        assert_eq!(appended_rows.len(), 1);
        assert_eq!(appended_rows[0].display_row, 0);
        assert_eq!(appended_rows[0].line_index, 0);
        assert_eq!(
            appended_rows[0].timestamp.as_deref(),
            Some("2026-08-10T12:34:56Z")
        );
        assert_eq!(appended_rows[0].text, "error");
        assert_eq!(appended_rows[0].style_spans.len(), 1);

        let appender = service.appender();
        appender
            .append_backfill(77, vec!["old one".into(), "old two".into()])
            .await
            .expect("history append is accepted");
        let LogStoreResult::Updated {
            total_lines,
            appended_rows,
            backfill_lines,
            ..
        } = wait_for(&service, |result| {
            matches!(
                result,
                LogStoreResult::Updated {
                    window_id: 77,
                    backfill_lines: Some(2),
                    ..
                }
            )
        })
        else {
            unreachable!()
        };
        assert_eq!(total_lines, 1);
        assert!(appended_rows.is_empty());
        assert_eq!(backfill_lines, Some(2));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn direct_ingestion_waits_for_spool_capacity_without_ui_draining_updates() {
        let service = LogStoreService::new(LogStoreConfig {
            command_channel_capacity: 1,
            result_channel_capacity: 1,
            ..LogStoreConfig::default()
        });
        let appender = service.appender();

        // This intentionally exceeds both bounded command queues many times
        // while leaving the UI result side untouched. Every batch must be
        // accepted eventually; a try_send data path would fail here.
        for line_index in 0..512 {
            appender
                .append(41, vec![format!("line {line_index}")])
                .await
                .expect("direct ingestion waits instead of dropping a batch");
        }

        let updated = wait_for(&service, |result| {
            matches!(
                result,
                LogStoreResult::Updated {
                    window_id: 41,
                    total_lines: 512,
                    ..
                }
            )
        });
        assert!(matches!(
            updated,
            LogStoreResult::Updated {
                window_id: 41,
                total_lines: 512,
                ..
            }
        ));

        assert!(service.load_page(41, 0, false, 256));
        let LogStoreResult::PageLoaded { rows, .. } = wait_for(&service, |result| {
            matches!(
                result,
                LogStoreResult::PageLoaded {
                    page_start: 256,
                    ..
                }
            )
        }) else {
            unreachable!()
        };
        assert_eq!(rows.len(), LOG_PAGE_SIZE);
        assert_eq!(rows[0].text, "line 256");
    }

    #[test]
    fn page_reads_overtake_pending_append_batches() {
        let (control_sender, control_receiver) = mpsc::sync_channel(1);
        let (_scan_sender, scan_receiver) = mpsc::sync_channel(1);
        let (append_sender, append_receiver) = mpsc::sync_channel(1);
        let (_backfill_sender, backfill_receiver) = mpsc::sync_channel(1);
        append_sender
            .send(Command::Append {
                window_id: 1,
                lines: vec!["queued append".to_owned()],
            })
            .expect("append queue accepts the batch");
        control_sender
            .send(Command::LoadPage {
                window_id: 1,
                generation: 0,
                filter_matches: false,
                page_start: 0,
            })
            .expect("control queue accepts the page request");

        assert!(matches!(
            next_store_command(
                &control_receiver,
                &append_receiver,
                &backfill_receiver,
                &scan_receiver,
            ),
            Some(Command::LoadPage { window_id: 1, .. })
        ));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn completed_backfill_rebases_overlapping_tail_without_changing_records() {
        let service = LogStoreService::default();
        let appender = service.appender();
        service.open(12);
        appender
            .append(
                12,
                vec![
                    "2026-08-10T10:00:02Z tail two".into(),
                    "2026-08-10T10:00:03Z tail three".into(),
                    "2026-08-10T10:00:04Z live four".into(),
                ],
            )
            .await
            .expect("initial tail is spooled");
        let _ = wait_for(&service, |result| {
            matches!(
                result,
                LogStoreResult::Updated {
                    window_id: 12,
                    total_lines: 3,
                    ..
                }
            )
        });

        appender
            .append_backfill(
                12,
                vec![
                    "2026-08-10T10:00:00Z history zero".into(),
                    "2026-08-10T10:00:01Z history one".into(),
                    "2026-08-10T10:00:02Z tail two".into(),
                    "2026-08-10T10:00:03Z tail three".into(),
                ],
            )
            .await
            .expect("history is spooled");
        appender
            .complete_backfill(12)
            .await
            .expect("history completion is accepted");

        assert!(matches!(
            wait_for(&service, |result| matches!(
                result,
                LogStoreResult::Rebased { .. }
            )),
            LogStoreResult::Rebased {
                window_id: 12,
                total_lines: 5,
                history_lines: 4,
                live_start: 2,
            }
        ));

        assert!(service.load_page(12, 0, false, 0));
        let LogStoreResult::PageLoaded {
            total_rows, rows, ..
        } = wait_for(&service, |result| {
            matches!(result, LogStoreResult::PageLoaded { .. })
        })
        else {
            unreachable!()
        };
        assert_eq!(total_rows, 5);
        assert_eq!(
            rows.into_iter().map(|row| row.text).collect::<Vec<_>>(),
            vec![
                "history zero",
                "history one",
                "tail two",
                "tail three",
                "live four",
            ]
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn rebase_keeps_new_live_records_after_the_overlapping_tail() {
        let service = LogStoreService::default();
        let appender = service.appender();
        service.open(13);
        appender
            .append(13, vec!["tail one".into(), "tail two".into()])
            .await
            .expect("initial tail is spooled");
        let _ = wait_for(&service, |result| {
            matches!(result, LogStoreResult::Updated { .. })
        });
        appender
            .append_backfill(13, vec!["old".into(), "tail one".into()])
            .await
            .expect("history is spooled");
        appender
            .complete_backfill(13)
            .await
            .expect("history completion is accepted");
        let _ = wait_for(&service, |result| {
            matches!(result, LogStoreResult::Rebased { .. })
        });

        appender
            .append(13, vec!["new live".into()])
            .await
            .expect("live stream remains writable after rebase");
        assert!(matches!(
            wait_for(&service, |result| {
                matches!(
                    result,
                    LogStoreResult::Updated {
                        window_id: 13,
                        total_lines: 4,
                        ..
                    }
                )
            }),
            LogStoreResult::Updated { .. }
        ));

        assert!(service.load_page(13, 0, false, 0));
        let LogStoreResult::PageLoaded { rows, .. } = wait_for(&service, |result| {
            matches!(result, LogStoreResult::PageLoaded { .. })
        }) else {
            unreachable!()
        };
        assert_eq!(
            rows.into_iter().map(|row| row.text).collect::<Vec<_>>(),
            vec!["old", "tail one", "tail two", "new live"]
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn unmatched_backfill_keeps_both_segments_instead_of_losing_live_logs() {
        let service = LogStoreService::default();
        let appender = service.appender();
        service.open(14);
        appender
            .append(14, vec!["live one".into(), "live two".into()])
            .await
            .expect("initial tail is spooled");
        let _ = wait_for(&service, |result| {
            matches!(result, LogStoreResult::Updated { .. })
        });
        appender
            .append_backfill(14, vec!["history one".into(), "history two".into()])
            .await
            .expect("history is spooled");
        appender
            .complete_backfill(14)
            .await
            .expect("history completion is accepted");

        assert!(matches!(
            wait_for(&service, |result| matches!(
                result,
                LogStoreResult::Rebased { .. }
            )),
            LogStoreResult::Rebased {
                history_lines: 2,
                live_start: 0,
                total_lines: 4,
                ..
            }
        ));
        assert!(service.load_page(14, 0, false, 0));
        let LogStoreResult::PageLoaded { rows, .. } = wait_for(&service, |result| {
            matches!(result, LogStoreResult::PageLoaded { .. })
        }) else {
            unreachable!()
        };
        assert_eq!(
            rows.into_iter().map(|row| row.text).collect::<Vec<_>>(),
            vec!["history one", "history two", "live one", "live two"]
        );
    }

    #[test]
    fn stores_complete_lines_and_returns_only_requested_page() {
        let service = LogStoreService::default();
        service.open(9);
        service.append(9, (0..600).map(|index| format!("line {index}")).collect());
        let result = wait_for(&service, |result| {
            matches!(
                result,
                LogStoreResult::Updated {
                    total_lines: 600,
                    ..
                }
            )
        });
        assert!(matches!(
            result,
            LogStoreResult::Updated {
                window_id: 9,
                total_lines: 600,
                ..
            }
        ));

        service.load_page(9, 0, false, 256);
        let LogStoreResult::PageLoaded {
            total_rows, rows, ..
        } = wait_for(&service, |result| {
            matches!(result, LogStoreResult::PageLoaded { .. })
        })
        else {
            unreachable!()
        };
        assert_eq!(total_rows, 600);
        assert_eq!(rows.len(), LOG_PAGE_SIZE);
        assert_eq!(rows[0].text, "line 256");
    }

    #[test]
    fn copy_reads_a_utf8_range_from_the_spool_with_displayed_metadata() {
        let service = LogStoreService::default();
        let first = "2026-08-10T12:34:56Z  alphaé";
        let second = "2026-08-10T12:34:57Z  beta";
        assert!(service.append(8, vec![first.to_owned(), second.to_owned()]));
        let _ = wait_for(&service, |result| {
            matches!(result, LogStoreResult::Updated { window_id: 8, .. })
        });
        let first_text = parse_kubernetes_log_line(first).line.text;
        let second_text = parse_kubernetes_log_line(second).line.text;
        let start = first_text.find('é').expect("utf8 character is present");
        let end = second_text.len();

        assert!(service.copy(8, 3, 0, false, 0, start, 1, end, true, true));
        let LogStoreResult::Copied {
            selection_generation,
            text,
            ..
        } = wait_for(&service, |result| {
            matches!(result, LogStoreResult::Copied { .. })
        })
        else {
            unreachable!()
        };

        assert_eq!(selection_generation, 3);
        assert_eq!(
            text,
            format!("     0  2026-08-10T12:34:56Z  é\n     1  2026-08-10T12:34:57Z  {second_text}")
        );
    }

    #[test]
    fn loads_only_the_pages_requested_while_scrolling() {
        let service = LogStoreService::new(LogStoreConfig {
            page_size: 2,
            ..LogStoreConfig::default()
        });
        service.append(
            3,
            ["line 0", "line 1", "line 2", "line 3", "line 4"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        );
        let _ = wait_for(&service, |result| {
            matches!(result, LogStoreResult::Updated { .. })
        });

        // Simulate the virtual scroll area reaching the second and then third
        // page. No full-log result is ever returned to the caller.
        service.load_page(3, 0, false, 2);
        let LogStoreResult::PageLoaded {
            page_start, rows, ..
        } = wait_for(&service, |result| {
            matches!(result, LogStoreResult::PageLoaded { page_start: 2, .. })
        })
        else {
            unreachable!()
        };
        assert_eq!(page_start, 2);
        assert_eq!(
            rows.iter().map(|row| row.text.as_str()).collect::<Vec<_>>(),
            ["line 2", "line 3"]
        );

        service.load_page(3, 0, false, 4);
        let LogStoreResult::PageLoaded { rows, .. } = wait_for(&service, |result| {
            matches!(result, LogStoreResult::PageLoaded { page_start: 4, .. })
        }) else {
            unreachable!()
        };
        assert_eq!(
            rows.iter().map(|row| row.text.as_str()).collect::<Vec<_>>(),
            ["line 4"]
        );
    }

    #[test]
    fn search_is_async_and_returns_match_ranges() {
        let service = LogStoreService::default();
        service.append(
            5,
            vec![
                "api ready".into(),
                "worker ready".into(),
                "API stopped".into(),
            ],
        );
        let _ = wait_for(&service, |result| {
            matches!(result, LogStoreResult::Updated { .. })
        });
        service.search(5, 1, "api".into(), false);
        let LogStoreResult::SearchCompleted { match_count, .. } = wait_for(&service, |result| {
            matches!(result, LogStoreResult::SearchCompleted { .. })
        }) else {
            unreachable!()
        };
        assert_eq!(match_count, 2);

        service.load_page(5, 1, true, 0);
        let LogStoreResult::PageLoaded { rows, .. } = wait_for(&service, |result| {
            matches!(result, LogStoreResult::PageLoaded { .. })
        }) else {
            unreachable!()
        };
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].match_ranges, vec![(0, 3)]);
    }

    #[test]
    fn search_ignores_ansi_sequences_and_matches_across_style_boundaries() {
        let service = LogStoreService::default();
        service.append(
            5,
            vec!["\u{1b}[31map\u{1b}[0mi ready".into(), "worker ready".into()],
        );
        let _ = wait_for(&service, |result| {
            matches!(result, LogStoreResult::Updated { .. })
        });

        service.search(5, 1, "api".into(), false);
        let LogStoreResult::SearchCompleted { match_count, .. } = wait_for(&service, |result| {
            matches!(
                result,
                LogStoreResult::SearchCompleted { generation: 1, .. }
            )
        }) else {
            unreachable!()
        };
        assert_eq!(match_count, 1);

        service.load_page(5, 1, true, 0);
        let LogStoreResult::PageLoaded { rows, .. } = wait_for(&service, |result| {
            matches!(result, LogStoreResult::PageLoaded { generation: 1, .. })
        }) else {
            unreachable!()
        };
        assert_eq!(rows[0].text, "api ready");
        assert_eq!(rows[0].match_ranges, vec![(0, 3)]);
        assert_eq!(rows[0].style_spans[0].range, (0, 2));

        service.search(5, 2, "a.i".into(), true);
        let LogStoreResult::SearchCompleted { match_count, .. } = wait_for(&service, |result| {
            matches!(
                result,
                LogStoreResult::SearchCompleted { generation: 2, .. }
            )
        }) else {
            unreachable!()
        };
        assert_eq!(match_count, 1);
    }

    #[test]
    fn search_includes_lines_appended_while_the_initial_scan_runs() {
        let service = LogStoreService::default();
        service.append(5, vec!["api starting".into()]);
        let _ = wait_for(&service, |result| {
            matches!(result, LogStoreResult::Updated { .. })
        });

        service.search(5, 1, "api".into(), false);
        service.append(5, vec!["api ready".into()]);

        let LogStoreResult::SearchCompleted { match_count, .. } = wait_for(&service, |result| {
            matches!(result, LogStoreResult::SearchCompleted { .. })
        }) else {
            unreachable!()
        };
        assert_eq!(match_count, 2);
    }

    #[test]
    fn temporary_files_are_removed_when_a_store_is_dropped() {
        let store = LogStore::new();
        let data_path = store
            .data
            .as_ref()
            .expect("data file exists")
            .path()
            .to_owned();
        let offsets_path = store
            .offsets
            .as_ref()
            .expect("offset index exists")
            .path()
            .to_owned();
        assert!(data_path.exists());
        assert!(offsets_path.exists());

        drop(store);

        assert!(!data_path.exists());
        assert!(!offsets_path.exists());
    }

    fn wait_for(
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
            thread::yield_now();
        }
    }
}
