use super::storage::LogStore;
use super::*;

pub(super) struct StoreThread {
    pub(super) control_receiver: mpsc::Receiver<Command>,
    pub(super) live_receiver: mpsc::Receiver<Command>,
    pub(super) backfill_receiver: mpsc::Receiver<Command>,
    pub(super) scan_receiver: mpsc::Receiver<Command>,
    pub(super) wake_receiver: mpsc::Receiver<()>,
    pub(super) work_pending: Arc<AtomicBool>,
    pub(super) scan_sender: StoreCommandSender,
    pub(super) result_sender: LogStoreResultSender,
    pub(super) live_updates: Arc<LiveUpdates>,
    pub(super) config: LogStoreConfig,
}

pub(super) fn run_store(store_thread: StoreThread) {
    let StoreThread {
        control_receiver,
        live_receiver,
        backfill_receiver,
        scan_receiver,
        wake_receiver,
        work_pending,
        scan_sender,
        result_sender,
        live_updates,
        config,
    } = store_thread;
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

pub(super) fn next_store_command(
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
pub(super) struct LogStoreResultSender {
    pub(super) sender: mpsc::SyncSender<LogStoreResult>,
    pub(super) repaint_context: Option<egui::Context>,
}

impl LogStoreResultSender {
    pub(super) fn new(
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
