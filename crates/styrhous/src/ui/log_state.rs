use crate::log_store::{LOG_PAGE_SIZE, LogPageRow, LogStoreResult};
use crate::minimal_resource::PodLogContainer;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::time::Instant;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub(super) struct LogDisplayOptions {
    pub(super) show_line_numbers: bool,
    pub(super) show_timestamps: bool,
    pub(super) render_ansi: bool,
}

impl Default for LogDisplayOptions {
    fn default() -> Self {
        Self {
            show_line_numbers: false,
            show_timestamps: false,
            render_ansi: true,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct PodLogWindowState {
    pub(super) id: u64,
    pub(super) cluster_key: i32,
    pub(super) namespace: String,
    pub(super) pod_name: String,
    pub(super) container: PodLogContainer,
    pub(super) total_lines: usize,
    /// Older records written by the background history request but not yet
    /// merged into the logical log stream.
    pub(super) backfill_lines: Option<usize>,
    /// Recent tail rows sent with the store notification. These bridge the
    /// small gap between accepting a live record and serving its disk page.
    pub(super) live_rows: BTreeMap<usize, LogPageRow>,
    pub(super) following_bottom: bool,
    pub(super) pages: HashMap<LogPageKey, LogPage>,
    pub(super) page_order: VecDeque<LogPageKey>,
    pub(super) page_cache_bytes: usize,
    pub(super) page_cache_limit: usize,
    pub(super) page_size: usize,
    pub(super) pending_pages: HashSet<LogPageKey>,
    /// The viewer keeps its initial surface quiet until a disk-backed page is
    /// available, rather than rendering a moving viewport of placeholders.
    pub(super) initial_page_loaded: bool,
    /// The first visible logical row, captured by the renderer so a spool
    /// rebase can preserve the rendered record rather than its old row number.
    pub(super) visible_top_display_row: usize,
    pub(super) store_opened: bool,
    pub(super) status: PodLogStatus,
    pub(super) close_requested: bool,
    pub(super) search: LogSearchState,
    pub(super) horizontal_content_width: f32,
    pub(super) selection: Option<LogTextSelection>,
    pub(super) selection_generation: u64,
    /// The character column retained when moving the caret vertically.
    pub(super) caret_preferred_column: Option<usize>,
    /// A keyboard move whose destination page has not arrived from the log store yet.
    pub(super) pending_caret: Option<PendingLogCaret>,
    /// Make the next rendered caret visible in the horizontal scroll viewport.
    pub(super) ensure_caret_visible: bool,
    pub(super) copied_text: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) enum PodLogStatus {
    Connecting,
    Following,
    Finished,
    Failed(String),
}

#[derive(Debug, Clone)]
pub(super) struct LogSearchState {
    pub(super) query: String,
    pub(super) regex_mode: bool,
    pub(super) generation: u64,
    pub(super) match_count: usize,
    pub(super) search_complete: bool,
    pub(super) scanned_lines: usize,
    pub(super) search_deadline: Option<Instant>,
    pub(super) active_display_row: Option<usize>,
    pub(super) active_match: Option<usize>,
    pub(super) scroll_to_display_row: Option<usize>,
    /// When present, preserve a rebased viewport by adding this many row
    /// heights to the currently persisted vertical offset.
    pub(super) rebase_scroll_row_delta: Option<usize>,
    pub(super) error: Option<String>,
    pub(super) filter_matches: bool,
}

impl Default for LogSearchState {
    fn default() -> Self {
        Self {
            query: String::new(),
            regex_mode: false,
            generation: 0,
            match_count: 0,
            search_complete: true,
            scanned_lines: 0,
            search_deadline: None,
            active_display_row: None,
            active_match: None,
            scroll_to_display_row: None,
            rebase_scroll_row_delta: None,
            error: None,
            filter_matches: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub(super) struct LogPageKey {
    pub(super) generation: u64,
    pub(super) filter_matches: bool,
    pub(super) page_start: usize,
}

#[derive(Debug, Clone)]
pub(super) struct LogPage {
    pub(super) rows: Vec<LogPageRow>,
    pub(super) bytes: usize,
    pub(super) max_text_columns: usize,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(super) struct LogTextPosition {
    pub(super) display_row: usize,
    pub(super) byte_offset: usize,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(super) struct LogTextSelection {
    pub(super) anchor: LogTextPosition,
    pub(super) focus: LogTextPosition,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(super) struct PendingLogCaret {
    pub(super) display_row: usize,
    pub(super) character_column: usize,
    /// `None` collapses the selection at the destination; otherwise the
    /// existing anchor is retained for a Shift-extended selection.
    pub(super) anchor: Option<LogTextPosition>,
}

impl LogTextSelection {
    pub(super) fn normalized(self) -> (LogTextPosition, LogTextPosition) {
        if (self.anchor.display_row, self.anchor.byte_offset)
            <= (self.focus.display_row, self.focus.byte_offset)
        {
            (self.anchor, self.focus)
        } else {
            (self.focus, self.anchor)
        }
    }

    pub(super) fn range_for_row(
        self,
        display_row: usize,
        line_len: usize,
    ) -> Option<(usize, usize)> {
        let (start, end) = self.normalized();
        if !(start.display_row..=end.display_row).contains(&display_row) {
            return None;
        }
        let start = if display_row == start.display_row {
            start.byte_offset.min(line_len)
        } else {
            0
        };
        let end = if display_row == end.display_row {
            end.byte_offset.min(line_len)
        } else {
            line_len
        };
        (start != end).then_some((start, end))
    }
}

impl PodLogWindowState {
    pub(super) const DEFAULT_PAGE_CACHE_LIMIT: usize = 128 * 1024 * 1024;

    pub(super) fn new(
        id: u64,
        cluster_key: i32,
        namespace: String,
        pod_name: String,
        container: PodLogContainer,
    ) -> Self {
        Self {
            id,
            cluster_key,
            namespace,
            pod_name,
            container,
            total_lines: 0,
            backfill_lines: None,
            live_rows: BTreeMap::new(),
            following_bottom: true,
            pages: HashMap::new(),
            page_order: VecDeque::new(),
            page_cache_bytes: 0,
            page_cache_limit: Self::DEFAULT_PAGE_CACHE_LIMIT,
            page_size: LOG_PAGE_SIZE,
            pending_pages: HashSet::new(),
            initial_page_loaded: false,
            visible_top_display_row: 0,
            store_opened: false,
            status: PodLogStatus::Connecting,
            close_requested: false,
            search: LogSearchState::default(),
            horizontal_content_width: 0.0,
            selection: None,
            selection_generation: 0,
            caret_preferred_column: None,
            pending_caret: None,
            ensure_caret_visible: false,
            copied_text: None,
        }
    }

    pub(super) fn clear_pages(&mut self) {
        self.pages.clear();
        self.page_order.clear();
        self.page_cache_bytes = 0;
        self.pending_pages.clear();
        self.live_rows.clear();
        self.horizontal_content_width = 0.0;
        self.set_selection(None);
        self.caret_preferred_column = None;
        self.pending_caret = None;
        self.ensure_caret_visible = false;
    }

    /// Updates selection state and invalidates any in-flight copy result for
    /// the previous selection.
    pub(super) fn set_selection(&mut self, selection: Option<LogTextSelection>) {
        self.selection = selection;
        self.selection_generation = self.selection_generation.wrapping_add(1);
    }

    pub(super) fn insert_page(&mut self, key: LogPageKey, rows: Vec<LogPageRow>) {
        self.pending_pages.remove(&key);
        if !key.filter_matches && !rows.is_empty() {
            self.initial_page_loaded = true;
            let page_end = key.page_start.saturating_add(rows.len());
            self.live_rows
                .retain(|display_row, _| *display_row < key.page_start || *display_row >= page_end);
        }
        if let Some(previous) = self.pages.remove(&key) {
            self.page_cache_bytes = self.page_cache_bytes.saturating_sub(previous.bytes);
            self.page_order.retain(|existing| *existing != key);
        }
        let bytes = rows
            .iter()
            .map(|row| {
                row.text.len()
                    + row.style_spans.len() * std::mem::size_of::<crate::ansi::AnsiStyleSpan>()
                    + row.match_ranges.len() * 2 * std::mem::size_of::<usize>()
            })
            .sum();
        let max_text_columns = rows
            .iter()
            .map(|row| row.text.chars().count())
            .max()
            .unwrap_or_default();
        self.page_cache_bytes += bytes;
        self.pages.insert(
            key,
            LogPage {
                rows,
                bytes,
                max_text_columns,
            },
        );
        self.page_order.push_back(key);
        while self.page_cache_bytes > self.page_cache_limit && self.page_order.len() > 1 {
            let oldest = self
                .page_order
                .pop_front()
                .expect("cache order is non-empty");
            if let Some(page) = self.pages.remove(&oldest) {
                self.page_cache_bytes = self.page_cache_bytes.saturating_sub(page.bytes);
            }
        }
    }
}

pub(super) fn apply_store_result(
    windows: &mut BTreeMap<u64, PodLogWindowState>,
    result: LogStoreResult,
) {
    match result {
        LogStoreResult::Updated {
            window_id,
            total_lines,
            completed_search,
            appended_rows,
            backfill_lines,
        } => {
            if let Some(window) = windows.get_mut(&window_id) {
                window.total_lines = total_lines;
                if let Some(backfill_lines) = backfill_lines {
                    window.backfill_lines = Some(backfill_lines);
                }
                if window.following_bottom && !window.search.filter_matches {
                    for row in appended_rows {
                        window.live_rows.insert(row.display_row, row);
                    }
                    while window.live_rows.len() > 2 * LOG_PAGE_SIZE {
                        let oldest = *window
                            .live_rows
                            .first_key_value()
                            .expect("live row cache is non-empty")
                            .0;
                        window.live_rows.remove(&oldest);
                    }
                }
                if let Some((generation, match_count)) = completed_search
                    && window.search.generation == generation
                {
                    window.search.match_count = match_count;
                    window.clear_pages();
                }
            }
        }
        LogStoreResult::Rebased {
            window_id,
            total_lines,
            live_start,
            history_lines,
        } => {
            if let Some(window) = windows.get_mut(&window_id) {
                let old_visible_row = window.visible_top_display_row;
                let rebased_visible_row =
                    rebase_display_row(old_visible_row, history_lines, live_start);
                // The scroll area clamps its horizontal offset against the
                // content width before the newly requested page is laid out.
                // Keep the previous width for that one frame so a wide-log
                // rebase cannot snap back to the left edge.
                let horizontal_content_width = window.horizontal_content_width;
                window.total_lines = total_lines;
                window.backfill_lines = None;
                window.initial_page_loaded = true;
                window.clear_pages();
                window.horizontal_content_width = horizontal_content_width;
                window.search.scroll_to_display_row =
                    Some(rebased_visible_row.min(total_lines.saturating_sub(1)));
                window.search.rebase_scroll_row_delta =
                    Some(rebased_visible_row.saturating_sub(old_visible_row));
                if !window.search.query.is_empty() {
                    window.search.generation = window.search.generation.wrapping_add(1);
                    window.search.match_count = 0;
                    window.search.search_complete = false;
                    window.search.scanned_lines = 0;
                    window.search.search_deadline = Some(Instant::now());
                }
            }
        }
        LogStoreResult::SearchProgress {
            window_id,
            generation,
            scanned_lines,
            total_lines,
            match_count,
        } => {
            if let Some(window) = windows.get_mut(&window_id)
                && window.search.generation == generation
            {
                window.total_lines = total_lines;
                window.search.scanned_lines = scanned_lines;
                window.search.match_count = match_count;
                window.search.search_complete = false;
                window.clear_pages();
            }
        }
        LogStoreResult::SearchCompleted {
            window_id,
            generation,
            match_count,
        } => {
            if let Some(window) = windows.get_mut(&window_id)
                && window.search.generation == generation
            {
                window.search.scanned_lines = window.total_lines;
                window.search.match_count = match_count;
                window.search.search_complete = true;
                window.clear_pages();
            }
        }
        LogStoreResult::PageLoaded {
            window_id,
            generation,
            filter_matches,
            page_start,
            total_rows,
            rows,
        } => {
            if let Some(window) = windows.get_mut(&window_id) {
                let key = LogPageKey {
                    generation,
                    filter_matches,
                    page_start,
                };
                if generation == window.search.generation {
                    if filter_matches {
                        window.search.match_count = total_rows;
                    } else {
                        window.total_lines = total_rows;
                    }
                    window.insert_page(key, rows);
                }
            }
        }
        LogStoreResult::MatchResolved {
            window_id,
            generation,
            match_row,
            line_index,
        } => {
            if let Some(window) = windows.get_mut(&window_id)
                && window.search.generation == generation
                && window.search.active_match == Some(match_row)
            {
                let display_row = if window.search.filter_matches {
                    match_row
                } else {
                    line_index
                };
                window.search.active_display_row = Some(display_row);
                window.search.scroll_to_display_row = Some(display_row);
            }
        }
        LogStoreResult::Copied {
            window_id,
            selection_generation,
            text,
        } => {
            if let Some(window) = windows.get_mut(&window_id)
                && window.selection_generation == selection_generation
            {
                window.copied_text = Some(text);
            }
        }
        LogStoreResult::Failed { window_id, error } => {
            if let Some(window) = windows.get_mut(&window_id) {
                window.status = PodLogStatus::Failed(format!("Log storage failed: {error}"));
            }
        }
    }
}

/// Translate a row in the initial tail segment to its logical position after
/// the completed history segment takes over. Its overlap is retained at the
/// end of history, while later records remain after that segment.
pub(super) fn rebase_display_row(
    live_row: usize,
    history_lines: usize,
    live_start: usize,
) -> usize {
    history_lines.saturating_sub(live_start) + live_row
}
