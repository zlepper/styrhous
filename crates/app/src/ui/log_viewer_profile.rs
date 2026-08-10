//! CPU-only benchmark support for the pod-log viewer.
//!
//! The Criterion target uses this module to run the production disk store and
//! egui layout path. GPU submission is deliberately outside this benchmark;
//! it should be profiled with a native renderer separately when needed.

use super::log_windows::show_log_window;
use super::state::{LogDisplayOptions, LogPageKey, PodLogStatus, PodLogWindowState};
use crate::log_store::{LOG_PAGE_SIZE, LogStoreConfig, LogStoreResult, LogStoreService};
use crate::minimal_resource::PodLogContainer;
use crate::resource_table::ContainerKind;
use std::time::{Duration, Instant};

const DEFAULT_TOTAL_LINES: usize = 100_000;
const PAGE_SIZE: usize = LOG_PAGE_SIZE;
const DEFAULT_PAYLOAD_BYTES: usize = 36;
const APPEND_BATCH_SIZE: usize = 1_024;

#[derive(Debug, Clone, Copy)]
pub struct PageTransitionTimings {
    pub request_frame: Duration,
    pub store_wait: Duration,
    pub loaded_frame: Duration,
}

/// Reusable state for Criterion's large-log scenarios.
pub struct LogViewerProfile {
    context: egui::Context,
    input: egui::RawInput,
    window: PodLogWindowState,
    display_options: LogDisplayOptions,
    log_store: LogStoreService,
    close_requested: bool,
    cached_page_start: usize,
    cached_row: usize,
    next_page: usize,
    total_pages: usize,
}

impl LogViewerProfile {
    /// Creates a deterministic 100,000-line ANSI log and warms one page for
    /// the cached-scroll benchmark.
    pub fn new() -> Result<Self, String> {
        Self::with_total_lines(DEFAULT_TOTAL_LINES)
    }

    /// Creates a deterministic ANSI log with the supplied number of lines.
    ///
    /// Keeping the viewport and page size fixed while varying this count
    /// verifies that virtualized rendering does not grow with log length.
    pub fn with_total_lines(total_lines: usize) -> Result<Self, String> {
        Self::with_total_lines_and_payload_bytes(total_lines, DEFAULT_PAYLOAD_BYTES)
    }

    /// Creates a deterministic ANSI log with a payload of the requested width
    /// in every row.
    pub fn with_total_lines_and_payload_bytes(
        total_lines: usize,
        payload_bytes: usize,
    ) -> Result<Self, String> {
        if total_lines < PAGE_SIZE * 2 {
            return Err(format!(
                "benchmark needs at least {} log lines",
                PAGE_SIZE * 2
            ));
        }
        let log_store = LogStoreService::new(LogStoreConfig {
            page_size: PAGE_SIZE,
            ..LogStoreConfig::default()
        });
        for start in (0..total_lines).step_by(APPEND_BATCH_SIZE) {
            let end = (start + APPEND_BATCH_SIZE).min(total_lines);
            if !log_store.append(1, synthetic_log_lines(start, end - start, payload_bytes)) {
                return Err(
                    "log-store command queue was full while creating the benchmark".to_owned(),
                );
            }
            wait_for_store_result(
                &log_store,
                |result| matches!(result, LogStoreResult::Updated { total_lines: updated_lines, .. } if *updated_lines == end),
            )?;
        }

        let total_pages = total_lines.div_ceil(PAGE_SIZE);
        let cached_page_start = (total_lines / 2 / PAGE_SIZE) * PAGE_SIZE;

        let mut profile = Self {
            context: egui::Context::default(),
            input: egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 900.0),
                )),
                ..Default::default()
            },
            window: empty_log_window(total_lines),
            display_options: LogDisplayOptions::default(),
            log_store,
            close_requested: false,
            cached_page_start,
            cached_row: cached_page_start,
            next_page: total_pages / 3,
            total_pages,
        };
        profile.window.search.scroll_to_display_row = Some(profile.cached_row);
        profile.run_frame();
        profile.insert_loaded_page(wait_for_store_result(&profile.log_store, |result| {
            matches!(result, LogStoreResult::PageLoaded { page_start, .. } if *page_start == profile.cached_row)
        })?);
        profile.run_frame();
        Ok(profile)
    }

    /// Measures the real egui CPU layout work while scrolling within a loaded
    /// page. The result prevents benchmark optimizations from eliding the run.
    pub fn scroll_cached_rows(&mut self) -> usize {
        self.cached_row += 1;
        if self.cached_row - self.cached_page_start > PAGE_SIZE - 48 {
            self.cached_row = self.cached_page_start;
        }
        self.window.search.scroll_to_display_row = Some(self.cached_row);
        self.run_frame();
        self.cached_row
    }

    /// Crosses into a new page with a one-page cache, measuring the complete
    /// visible cache-miss sequence: request frame, store read/parse, eviction,
    /// and the frame that lays out the newly-loaded rows.
    pub fn load_and_render_next_page(&mut self) -> Result<usize, String> {
        self.load_and_render_next_page_timed()
            .map(|(_, page_start)| page_start)
    }

    /// Returns the latency components of a page transition. This is intended
    /// for diagnosing perceived scroll responsiveness rather than throughput.
    pub fn load_and_render_next_page_timed(
        &mut self,
    ) -> Result<(PageTransitionTimings, usize), String> {
        self.window.page_cache_limit = 1;
        self.window.clear_pages();
        let page_start = self.next_page * PAGE_SIZE;
        self.next_page = (self.next_page + 1) % self.total_pages;
        self.window.search.scroll_to_display_row = Some(page_start);
        let start = Instant::now();
        self.run_frame();
        let request_frame = start.elapsed();

        let start = Instant::now();
        let result = wait_for_store_result(
            &self.log_store,
            |result| matches!(result, LogStoreResult::PageLoaded { page_start: loaded, .. } if *loaded == page_start),
        )?;
        let store_wait = start.elapsed();
        self.insert_loaded_page(result);

        let start = Instant::now();
        self.run_frame();
        Ok((
            PageTransitionTimings {
                request_frame,
                store_wait,
                loaded_frame: start.elapsed(),
            },
            page_start,
        ))
    }

    fn run_frame(&mut self) {
        let _ = self.context.run(self.input.clone(), |context| {
            show_log_window(
                context,
                &mut self.window,
                &mut self.display_options,
                &self.log_store,
                &mut self.close_requested,
            );
        });
    }

    fn insert_loaded_page(&mut self, result: LogStoreResult) {
        let LogStoreResult::PageLoaded {
            generation,
            filter_matches,
            page_start,
            rows,
            ..
        } = result
        else {
            unreachable!("benchmark waited for a loaded log page")
        };
        self.window.insert_page(
            LogPageKey {
                generation,
                filter_matches,
                page_start,
            },
            rows,
        );
    }
}

fn empty_log_window(total_lines: usize) -> PodLogWindowState {
    PodLogWindowState {
        id: 1,
        cluster_key: 1,
        namespace: "default".to_owned(),
        pod_name: "profiled-pod".to_owned(),
        container: PodLogContainer {
            name: "api".to_owned(),
            kind: ContainerKind::App,
        },
        total_lines,
        pages: Default::default(),
        page_order: Default::default(),
        page_cache_bytes: 0,
        page_cache_limit: PodLogWindowState::DEFAULT_PAGE_CACHE_LIMIT,
        page_size: PAGE_SIZE,
        pending_pages: Default::default(),
        store_opened: true,
        status: PodLogStatus::Finished,
        close_requested: false,
        search: Default::default(),
    }
}

fn synthetic_log_lines(start: usize, count: usize, payload_bytes: usize) -> Vec<String> {
    let payload = "x".repeat(payload_bytes);
    (start..start + count)
        .map(|line| {
            format!(
                "2026-08-10T12:34:{:02}.123Z  \u{1b}[{}m{}\u{1b}[0m request_id={line:08} route=/v1/widgets latency={}ms payload={payload}",
                line % 60,
                if line % 20 == 0 { 33 } else { 32 },
                if line % 20 == 0 { "WARN" } else { "INFO" },
                line % 500,
            )
        })
        .collect()
}

fn wait_for_store_result(
    service: &LogStoreService,
    matches: impl Fn(&LogStoreResult) -> bool,
) -> Result<LogStoreResult, String> {
    let start = Instant::now();
    loop {
        if let Some(result) = service.try_next_result()
            && matches(&result)
        {
            return Ok(result);
        }
        if start.elapsed() >= Duration::from_secs(10) {
            return Err("timed out waiting for the log store".to_owned());
        }
        std::thread::yield_now();
    }
}
