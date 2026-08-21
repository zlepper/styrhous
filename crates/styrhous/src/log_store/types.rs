use crate::ansi::AnsiStyleSpan;

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
