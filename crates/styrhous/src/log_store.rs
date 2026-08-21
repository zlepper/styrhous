//! Disk-backed storage and search for native pod-log windows.
//!
//! The Kubernetes worker streams directly into this spool through a bounded
//! ingress handle. The UI only consumes coalesced progress and paged results.

use crate::ansi::parse_kubernetes_log_line;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;

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
                run_store(StoreThread {
                    control_receiver,
                    live_receiver,
                    backfill_receiver,
                    scan_receiver,
                    wake_receiver,
                    work_pending,
                    scan_sender,
                    result_sender,
                    live_updates: store_live_updates,
                    config,
                })
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

mod runner;
mod scan;
mod storage;
mod types;

#[cfg(test)]
use runner::next_store_command;
use runner::{LogStoreResultSender, StoreThread, run_store};
use storage::AppendSummary;
#[cfg(test)]
use storage::LogStore;
pub(crate) use types::*;

#[cfg(test)]
mod tests;
