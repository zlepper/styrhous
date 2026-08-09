//! Disk-backed storage and search for native pod-log windows.
//!
//! This is deliberately independent from the Kubernetes worker. The worker only
//! owns the API stream; the UI forwards its bounded batches here and consumes
//! this service's paged results.

use crate::ansi::{AnsiStyleSpan, parse_kubernetes_log_line};
use regex::Regex;
use std::collections::HashMap;
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
    commands: mpsc::SyncSender<Command>,
    results: mpsc::Receiver<LogStoreResult>,
}

impl Default for LogStoreService {
    fn default() -> Self {
        Self::new(LogStoreConfig::default())
    }
}

impl LogStoreService {
    pub(crate) fn new(config: LogStoreConfig) -> Self {
        let config = LogStoreConfig {
            page_size: config.page_size.max(1),
            command_channel_capacity: config.command_channel_capacity.max(1),
            result_channel_capacity: config.result_channel_capacity.max(1),
            search_progress_interval: config.search_progress_interval.max(1),
        };
        let (commands, dispatcher_receiver) = mpsc::sync_channel(config.command_channel_capacity);
        let (store_sender, receiver) = mpsc::sync_channel(config.command_channel_capacity);
        let (result_sender, results) = mpsc::sync_channel(config.result_channel_capacity);
        let scan_sender = store_sender.clone();
        thread::Builder::new()
            .name("pod-log-command-bridge".to_owned())
            .spawn(move || {
                while let Ok(command) = dispatcher_receiver.recv() {
                    if store_sender.send(command).is_err() {
                        break;
                    }
                }
            })
            .expect("Failed to start pod log command bridge");
        thread::Builder::new()
            .name("pod-log-store".to_owned())
            .spawn(move || run_store(receiver, scan_sender, result_sender, config))
            .expect("Failed to start pod log store thread");
        Self { commands, results }
    }
    pub(crate) fn open(&self, window_id: u64) -> bool {
        self.send(Command::Open { window_id })
    }

    pub(crate) fn append(&self, window_id: u64, lines: Vec<String>) -> bool {
        if !lines.is_empty() {
            self.send(Command::Append { window_id, lines })
        } else {
            true
        }
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

    pub(crate) fn try_next_result(&self) -> Option<LogStoreResult> {
        self.results.try_recv().ok()
    }

    fn send(&self, command: Command) -> bool {
        self.commands.try_send(command).is_ok()
    }
}

fn run_store(
    receiver: mpsc::Receiver<Command>,
    scan_sender: mpsc::SyncSender<Command>,
    result_sender: mpsc::SyncSender<LogStoreResult>,
    config: LogStoreConfig,
) {
    let mut stores = HashMap::<u64, LogStore>::new();
    while let Ok(command) = receiver.recv() {
        match command {
            Command::Open { window_id } => {
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
                let store = stores.entry(window_id).or_insert_with(LogStore::new);
                match store.append(lines) {
                    Ok(summary) => send_result(
                        &result_sender,
                        LogStoreResult::Updated {
                            window_id,
                            total_lines: summary.total_lines,
                            completed_search: summary.completed_search,
                        },
                    ),
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
            Command::Close { window_id } => {
                stores.remove(&window_id);
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
                            total_lines: store.total_lines,
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
                            total_lines: store.total_lines,
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

fn send_result(sender: &mpsc::SyncSender<LogStoreResult>, result: LogStoreResult) {
    let _ = sender.send(result);
}

fn send_failure(
    sender: &mpsc::SyncSender<LogStoreResult>,
    window_id: u64,
    error: impl std::fmt::Display,
) {
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
    total_lines: usize,
    search: Option<SearchState>,
    initialization_error: Option<String>,
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
}

impl LogStore {
    fn new() -> Self {
        match (NamedTempFile::new(), NamedTempFile::new()) {
            (Ok(data), Ok(offsets)) => Self {
                data: Some(data),
                offsets: Some(offsets),
                total_lines: 0,
                search: None,
                initialization_error: None,
            },
            (data, offsets) => Self {
                data: None,
                offsets: None,
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

    fn append(&mut self, lines: Vec<String>) -> anyhow::Result<AppendSummary> {
        self.init_error()
            .map_err(|error| anyhow::Error::msg(error.to_owned()))?;
        let mut data = self.data()?.reopen()?;
        let mut offsets = self.offsets()?.reopen()?;
        let mut next_offset = data.seek(SeekFrom::End(0))?;
        let first_line_index = self.total_lines;
        let completed_matcher = self
            .search
            .as_ref()
            .filter(|search| search.complete)
            .map(|search| search.matcher.clone());
        let mut line_offsets = Vec::with_capacity(lines.len());
        let mut matching_line_indices = Vec::new();

        for (relative_line_index, line) in lines.iter().enumerate() {
            let bytes = line.as_bytes();
            let length = u32::try_from(bytes.len())
                .map_err(|_| anyhow::anyhow!("A log line exceeds 4 GiB"))?;
            line_offsets.push(next_offset);
            data.write_all(&length.to_le_bytes())?;
            data.write_all(bytes)?;
            next_offset += u64::from(length) + 4;
            let visible_line = parse_kubernetes_log_line(line);
            if completed_matcher
                .as_ref()
                .is_some_and(|matcher| matcher.is_match(&visible_line.line.text))
            {
                matching_line_indices.push(first_line_index + relative_line_index);
            }
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
            total_lines: self.total_lines,
            completed_search: self
                .search
                .as_ref()
                .filter(|search| search.complete)
                .map(|search| (search.generation, search.match_count)),
        })
    }

    fn start_search(
        &mut self,
        window_id: u64,
        generation: u64,
        query: String,
        regex_mode: bool,
        sender: mpsc::SyncSender<Command>,
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
        let scan_lines = self.total_lines;
        let scan_end = self.data()?.reopen()?.metadata()?.len();
        let reader = self.data()?.reopen()?;
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
                    scan_end,
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
        for line_index in scanned_lines..self.total_lines {
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
                self.total_lines,
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

    fn read_line(&self, line_index: usize) -> anyhow::Result<String> {
        let mut data = self.data()?.reopen()?;
        let offset = read_u64_at(&mut self.offsets()?.reopen()?, line_index)?;
        data.seek(SeekFrom::Start(offset))?;
        let mut length = [0_u8; 4];
        data.read_exact(&mut length)?;
        let mut bytes = vec![0; u32::from_le_bytes(length) as usize];
        data.read_exact(&mut bytes)?;
        String::from_utf8(bytes).map_err(anyhow::Error::from)
    }
}

fn read_u64_at(file: &mut File, index: usize) -> anyhow::Result<u64> {
    file.seek(SeekFrom::Start((index * std::mem::size_of::<u64>()) as u64))?;
    let mut bytes = [0_u8; 8];
    file.read_exact(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

fn scan_records(
    mut data: File,
    scan_end: u64,
    scan_lines: usize,
    matcher: Regex,
    cancellation: Arc<AtomicBool>,
    window_id: u64,
    generation: u64,
    sender: mpsc::SyncSender<Command>,
    search_progress_interval: usize,
) {
    let mut scanned_lines = 0;
    let mut match_lines = Vec::new();
    while data.stream_position().unwrap_or(scan_end) < scan_end && scanned_lines < scan_lines {
        if cancellation.load(Ordering::Relaxed) {
            return;
        }
        let mut length = [0_u8; 4];
        if data.read_exact(&mut length).is_err() {
            return;
        }
        let mut bytes = vec![0; u32::from_le_bytes(length) as usize];
        if data.read_exact(&mut bytes).is_err() {
            return;
        }
        if matcher.is_match(
            &parse_kubernetes_log_line(&String::from_utf8_lossy(&bytes))
                .line
                .text,
        ) {
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
    use std::time::Duration;

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
