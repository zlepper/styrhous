//! The shape of a log: how busy it was over time, how many errors and
//! warnings it holds and where, and how many lines each source wrote.
//!
//! The spool counts every record as it is written (live or backfilled) into
//! per-segment accumulators, so a summary never rescans the log. The volume
//! is kept per second of log time and re-binned into as many bars as the
//! strip has room for when a summary is taken.

use super::storage::LogStore;
use super::*;
use crate::ansi::parse_kubernetes_log_line;
use crate::log_level::parse_log_level;
use components::LogLevel;
use std::collections::BTreeMap;
use std::io::Read;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

/// The most problem ticks a summary carries; a log with more problems than
/// this merges neighbouring ones, the worse level winning.
const MAX_PROBLEM_TICKS: usize = 4096;

/// A line that needs attention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Problem {
    Warning,
    Error,
}

impl Problem {
    fn of(level: Option<LogLevel>) -> Option<Self> {
        match level? {
            LogLevel::Error => Some(Self::Error),
            LogLevel::Warn => Some(Self::Warning),
            LogLevel::Info | LogLevel::Debug => None,
        }
    }
}

/// What one record adds to a summary.
pub(super) struct RecordFacts {
    second: Option<i64>,
    problem: Option<Problem>,
    source: Option<String>,
}

impl RecordFacts {
    pub(super) fn new(source: Option<&str>, timestamp: Option<&str>, message: &str) -> Self {
        Self {
            second: timestamp
                .and_then(|timestamp| OffsetDateTime::parse(timestamp, &Rfc3339).ok())
                .map(OffsetDateTime::unix_timestamp),
            problem: Problem::of(parse_log_level(message)),
            source: source.map(str::to_owned),
        }
    }

    /// The facts of a raw spooled record.
    pub(super) fn of_record(record: &LogRecord) -> Self {
        let parsed = parse_kubernetes_log_line(&record.text);
        Self::new(
            record.source.as_deref(),
            parsed.timestamp.as_deref(),
            &parsed.line.text,
        )
    }
}

#[derive(Clone, Copy, Default)]
struct SecondCounts {
    lines: i64,
    errors: i64,
    warnings: i64,
}

/// Counts that can be added and taken away again (a rebase removes the
/// records the history and the live tail share).
#[derive(Clone, Default)]
pub(super) struct SummaryCounts {
    seconds: BTreeMap<i64, SecondCounts>,
    lines: i64,
    errors: i64,
    warnings: i64,
    sources: HashMap<String, i64>,
}

impl SummaryCounts {
    fn apply(&mut self, facts: &RecordFacts, sign: i64) {
        self.lines += sign;
        let (errors, warnings) = match facts.problem {
            Some(Problem::Error) => (sign, 0),
            Some(Problem::Warning) => (0, sign),
            None => (0, 0),
        };
        self.errors += errors;
        self.warnings += warnings;
        if let Some(second) = facts.second {
            let counts = self.seconds.entry(second).or_default();
            counts.lines += sign;
            counts.errors += errors;
            counts.warnings += warnings;
        }
        if let Some(source) = &facts.source {
            *self.sources.entry(source.clone()).or_default() += sign;
        }
    }

    /// Count `records`, as they are read back from the spool.
    pub(super) fn record_all(
        &mut self,
        records: impl Iterator<Item = anyhow::Result<LogRecord>>,
    ) -> anyhow::Result<()> {
        for record in records {
            self.apply(&RecordFacts::of_record(&record?), 1);
        }
        Ok(())
    }

    fn add(&mut self, other: &Self, sign: i64) {
        self.lines += sign * other.lines;
        self.errors += sign * other.errors;
        self.warnings += sign * other.warnings;
        for (second, counts) in &other.seconds {
            let mine = self.seconds.entry(*second).or_default();
            mine.lines += sign * counts.lines;
            mine.errors += sign * counts.errors;
            mine.warnings += sign * counts.warnings;
        }
        for (source, count) in &other.sources {
            *self.sources.entry(source.clone()).or_default() += sign * count;
        }
    }
}

/// The summary of one spool segment: its counts, and the segment-local
/// index of every error and warning line.
#[derive(Clone, Default)]
pub(crate) struct SegmentSummary {
    counts: SummaryCounts,
    problems: Vec<(usize, Problem)>,
}

impl SegmentSummary {
    pub(super) fn record(&mut self, line: usize, facts: &RecordFacts) {
        self.counts.apply(facts, 1);
        if let Some(problem) = facts.problem {
            self.problems.push((line, problem));
        }
    }
}

/// How busy one bar's stretch of time was.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct VolumeBucket {
    pub(crate) lines: usize,
    pub(crate) errors: usize,
    pub(crate) warnings: usize,
}

/// What a window's volume strip and problem ticks show.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct LogSummary {
    /// The lines the log held when the summary was taken.
    pub(crate) total_lines: usize,
    pub(crate) errors: usize,
    pub(crate) warnings: usize,
    /// Seconds between the first and last timestamp; `None` without any.
    pub(crate) span_seconds: Option<u64>,
    /// Lines per stretch of time, oldest first (empty without timestamps).
    pub(crate) bars: Vec<VolumeBucket>,
    /// Lines per source, by source name.
    pub(crate) sources: Vec<(String, usize)>,
    /// The error and warning rows, as `(display row, level)` in order. For a
    /// filtered window these are rows of the matches, otherwise of the log.
    pub(crate) problems: Vec<(usize, Problem)>,
    /// The search generation and match count `problems` are the matches of.
    pub(crate) filtered_by: Option<(u64, usize)>,
}

/// The problems among the matches of a search, found so far.
///
/// A tailing log only ever adds matches (and problems) at its end, so a
/// summary reads just the matches it has not seen yet. A rebase renumbers the
/// lines, which starts the scan over.
pub(crate) struct ProblemCache {
    generation: u64,
    /// The rebase (`history_lines`, `live_start`) the line numbers are under.
    basis: Option<(usize, usize)>,
    /// How many match rows were read, and how far through the problems.
    scanned: usize,
    next_problem: usize,
    /// `(match row, level)` of each problem line, in match order.
    rows: Vec<(usize, Problem)>,
    /// Match rows read over this cache's life, for the tests.
    #[cfg(test)]
    rows_read: usize,
}

impl ProblemCache {
    fn new(generation: u64, basis: Option<(usize, usize)>) -> Self {
        Self {
            generation,
            basis,
            scanned: 0,
            next_problem: 0,
            rows: Vec::new(),
            #[cfg(test)]
            rows_read: 0,
        }
    }

    /// Take in match `row`, which is line `line` of the log.
    fn read_match(&mut self, row: usize, line: usize, problems: &[(usize, Problem)]) {
        while self.next_problem < problems.len() && problems[self.next_problem].0 < line {
            self.next_problem += 1;
        }
        if let Some((problem_line, problem)) = problems.get(self.next_problem)
            && *problem_line == line
        {
            self.rows.push((row, *problem));
        }
        self.scanned = row + 1;
        #[cfg(test)]
        {
            self.rows_read += 1;
        }
    }
}

/// The problems among the matches of a search.
struct FilteredProblems {
    /// `(match row, level)` of each problem line, in match order.
    rows: Vec<(usize, Problem)>,
    match_count: usize,
    generation: u64,
}

impl LogStore {
    /// The summary of the log as it is now, its volume in at most `bars`
    /// bars. With `filter` (a search generation) the problems are those of
    /// the matching lines, in match order.
    pub(crate) fn summary(
        &mut self,
        bars: usize,
        filter: Option<u64>,
    ) -> anyhow::Result<LogSummary> {
        let mut counts = self.live_summary.counts.clone();
        let mut problems = self.live_summary.problems.clone();
        if let (Some(rebase), Some(backfill)) = (self.rebase, &self.backfill) {
            counts.add(&backfill.summary.counts, 1);
            if let Some(overlap) = &self.overlap {
                counts.add(overlap, -1);
            }
            // The live records before `live_start` are the history's last
            // ones; the rest follow the history.
            let shifted = problems
                .iter()
                .filter(|(line, _)| *line >= rebase.live_start)
                .map(|(line, problem)| (line - rebase.live_start + rebase.history_lines, *problem));
            problems = backfill
                .summary
                .problems
                .iter()
                .copied()
                .chain(shifted)
                .collect();
        }
        let total_lines = self.visible_total_lines();
        let filtered = match filter {
            Some(generation) => self.matching_problems(&problems, generation)?,
            None => None,
        };
        let (rows, row_count, filtered_by) = match filtered {
            Some(FilteredProblems {
                rows,
                match_count,
                generation,
            }) => (rows, match_count, Some((generation, match_count))),
            None => (problems, total_lines, None),
        };
        let first = counts.seconds.keys().next().copied();
        let last = counts.seconds.keys().next_back().copied();
        let span_seconds = first.zip(last).map(|(first, last)| (last - first) as u64);
        let mut sources: Vec<(String, usize)> = counts
            .sources
            .iter()
            .map(|(source, count)| (source.clone(), (*count).max(0) as usize))
            .collect();
        sources.sort();
        Ok(LogSummary {
            total_lines,
            errors: counts.errors.max(0) as usize,
            warnings: counts.warnings.max(0) as usize,
            span_seconds,
            bars: volume_bars(&counts, first.zip(last), bars),
            sources,
            problems: merge_problem_ticks(rows, row_count),
            filtered_by,
        })
    }

    /// The problems among the matches of the search `generation`, as rows of
    /// the filtered view, with the match count they were found among.
    fn matching_problems(
        &mut self,
        problems: &[(usize, Problem)],
        generation: u64,
    ) -> anyhow::Result<Option<FilteredProblems>> {
        let Some(search) = self
            .search
            .as_ref()
            .filter(|search| search.generation == generation)
        else {
            return Ok(None);
        };
        let Some(offsets) = &search.match_offsets else {
            return Ok(None);
        };
        let match_count = search.match_count;
        let basis = self
            .rebase
            .map(|rebase| (rebase.history_lines, rebase.live_start));
        let mut cache = self
            .problem_cache
            .take()
            .filter(|cache| {
                cache.generation == generation
                    && cache.basis == basis
                    && cache.scanned <= match_count
            })
            .unwrap_or_else(|| ProblemCache::new(generation, basis));
        if cache.scanned < match_count {
            let mut reader = std::io::BufReader::new(offsets.reopen()?);
            reader.seek_relative(cache.scanned as i64 * 8)?;
            for row in cache.scanned..match_count {
                let mut bytes = [0_u8; 8];
                reader.read_exact(&mut bytes)?;
                cache.read_match(row, u64::from_le_bytes(bytes) as usize, problems);
            }
        }
        let rows = cache.rows.clone();
        self.problem_cache = Some(cache);
        Ok(Some(FilteredProblems {
            rows,
            match_count,
            generation,
        }))
    }
}

/// Re-bin the per-second counts into at most `bars` bars covering the log's
/// time span; no bar is shorter than a second.
fn volume_bars(
    counts: &SummaryCounts,
    bounds: Option<(i64, i64)>,
    bars: usize,
) -> Vec<VolumeBucket> {
    let Some((first, last)) = bounds else {
        return Vec::new();
    };
    let range = (last - first + 1) as u128;
    let bar_count = (bars as u128).min(range);
    if bar_count == 0 {
        return Vec::new();
    }
    let mut buckets = vec![VolumeBucket::default(); bar_count as usize];
    for (second, second_counts) in &counts.seconds {
        let index = ((second - first) as u128 * bar_count / range) as usize;
        let bucket = &mut buckets[index];
        bucket.lines += second_counts.lines.max(0) as usize;
        bucket.errors += second_counts.errors.max(0) as usize;
        bucket.warnings += second_counts.warnings.max(0) as usize;
    }
    buckets
}

/// Merge ticks of neighbouring rows so at most [`MAX_PROBLEM_TICKS`] remain,
/// the worse problem (and its first row) standing for each group.
fn merge_problem_ticks(rows: Vec<(usize, Problem)>, row_count: usize) -> Vec<(usize, Problem)> {
    if rows.len() <= MAX_PROBLEM_TICKS {
        return rows;
    }
    let group_of = |row: usize| row * MAX_PROBLEM_TICKS / row_count.max(1);
    let mut merged: Vec<(usize, Problem)> = Vec::with_capacity(MAX_PROBLEM_TICKS);
    let mut current_group = None;
    for (row, problem) in rows {
        let group = group_of(row);
        match merged.last_mut() {
            Some(last) if current_group == Some(group) => {
                if problem > last.1 {
                    *last = (row, problem);
                }
            }
            _ => {
                merged.push((row, problem));
                current_group = Some(group);
            }
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(second: Option<i64>, problem: Option<Problem>, source: Option<&str>) -> RecordFacts {
        RecordFacts {
            second,
            problem,
            source: source.map(str::to_owned),
        }
    }

    #[test]
    fn bars_rebin_seconds_over_the_span() {
        let mut counts = SummaryCounts::default();
        for second in 100..110 {
            counts.apply(&facts(Some(second), None, None), 1);
        }
        counts.apply(&facts(Some(109), Some(Problem::Error), None), 1);
        let bounds = Some((100, 109));

        let bars = volume_bars(&counts, bounds, 2);
        assert_eq!(bars.len(), 2);
        assert_eq!((bars[0].lines, bars[1].lines), (5, 6));
        assert_eq!((bars[0].errors, bars[1].errors), (0, 1));

        let capped = volume_bars(&counts, bounds, 100);
        assert_eq!(capped.len(), 10, "no bar is shorter than one second");
        assert!(volume_bars(&counts, None, 10).is_empty());
    }

    #[test]
    fn counts_can_be_taken_away_again() {
        let mut live = SummaryCounts::default();
        let mut shared = SummaryCounts::default();
        for second in [1, 2] {
            let record = facts(Some(second), Some(Problem::Warning), Some("api-0"));
            live.apply(&record, 1);
            shared.apply(&record, 1);
        }
        live.add(&shared, -1);
        assert_eq!((live.lines, live.warnings), (0, 0));
        assert_eq!(live.sources["api-0"], 0);
    }

    #[test]
    fn crowded_problems_merge_to_the_worst_of_each_group() {
        let rows: Vec<_> = (0..20_000)
            .map(|row| {
                (
                    row,
                    if row == 2 {
                        Problem::Error
                    } else {
                        Problem::Warning
                    },
                )
            })
            .collect();
        let merged = merge_problem_ticks(rows, 20_000);
        assert!(merged.len() <= MAX_PROBLEM_TICKS);
        assert_eq!(
            merged[0],
            (2, Problem::Error),
            "the error stands for its group"
        );
        assert!(merged.windows(2).all(|pair| pair[0].0 < pair[1].0));

        let few = vec![(3, Problem::Error)];
        assert_eq!(merge_problem_ticks(few.clone(), 10), few);
    }
}

#[cfg(test)]
mod incremental_tests {
    use super::super::storage::SearchState;
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use tempfile::NamedTempFile;

    fn record(text: &str) -> LogRecord {
        LogRecord::from_source(format!("2026-08-10T10:00:00Z {text}"), "api-0".to_owned())
    }

    fn rows_read(store: &LogStore) -> usize {
        store
            .problem_cache
            .as_ref()
            .map_or(0, |cache| cache.rows_read)
    }

    /// Every line matches; the search is already complete.
    fn search_everything(store: &mut LogStore, generation: u64) {
        store.search = Some(SearchState {
            generation,
            matcher: crate::log_store::log_matcher("api", false).unwrap(),
            cancellation: Arc::new(AtomicBool::new(false)),
            match_offsets: Some(NamedTempFile::new().unwrap()),
            match_count: 0,
            complete: true,
        });
        let lines = store.total_lines;
        store.append_search_matches((0..lines).collect()).unwrap();
    }

    #[test]
    fn a_tailing_filtered_summary_only_reads_the_new_matches() {
        let mut store = LogStore::new();
        store
            .append(vec![
                record("INFO api start"),
                record("ERROR api down"),
                record("INFO api up"),
            ])
            .unwrap();
        search_everything(&mut store, 5);

        let first = store.summary(4, Some(5)).unwrap();
        assert_eq!(first.problems, vec![(1, Problem::Error)]);
        assert_eq!(rows_read(&store), 3);

        let unchanged = store.summary(4, Some(5)).unwrap();
        assert_eq!(unchanged.problems, first.problems);
        assert_eq!(rows_read(&store), 3, "nothing new, nothing read");

        store
            .append(vec![record("WARN api slow"), record("INFO api fine")])
            .unwrap();
        let tailed = store.summary(4, Some(5)).unwrap();
        assert_eq!(
            tailed.problems,
            vec![(1, Problem::Error), (3, Problem::Warning)]
        );
        assert_eq!(tailed.filtered_by, Some((5, 5)));
        assert_eq!(rows_read(&store), 5, "only the two new matches were read");

        store.append(vec![record("INFO api more")]).unwrap();
        search_everything(&mut store, 6);
        let restarted = store.summary(4, Some(6)).unwrap();
        assert_eq!(restarted.problems, tailed.problems);
        assert_eq!(
            rows_read(&store),
            6,
            "a new search scans all of its matches from the start"
        );
    }
}
