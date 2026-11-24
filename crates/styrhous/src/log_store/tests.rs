use super::*;
use crate::ansi::parse_kubernetes_log_line;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

mod export;
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

#[test]
fn a_panic_while_holding_the_live_update_lock_does_not_poison_later_callers() {
    let updates = std::sync::Arc::new(LiveUpdates::new(None));
    let poisoner = updates.clone();
    let _ = std::thread::spawn(move || {
        let _guard = poisoner.locked();
        panic!("a publisher panics while holding the lock");
    })
    .join();

    updates.publish_backfill(1, 10, 4);

    assert!(matches!(
        updates.take_next(),
        Some(LogStoreResult::Updated {
            window_id: 1,
            backfill_lines: Some(4),
            ..
        })
    ));
    updates.remove(1);
}
