use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

mod ingestion;
mod storage;

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
