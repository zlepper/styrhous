use super::storage::LogicalReader;
use super::*;

pub(super) struct SearchScan {
    pub(super) reader: LogicalReader,
    pub(super) scan_lines: usize,
    pub(super) matcher: Regex,
    pub(super) cancellation: Arc<AtomicBool>,
    pub(super) window_id: u64,
    pub(super) generation: u64,
    pub(super) sender: StoreCommandSender,
    pub(super) search_progress_interval: usize,
}

pub(super) fn scan_records(search_scan: SearchScan) {
    let SearchScan {
        mut reader,
        scan_lines,
        matcher,
        cancellation,
        window_id,
        generation,
        sender,
        search_progress_interval,
    } = search_scan;
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
