use super::*;
use kube::runtime::watcher::DefaultBackoff;
use std::time::Duration;
use tokio::time::Instant;

/// How long a watcher may keep retrying before the user is told about it.
/// Short blips (sleep/wake, Wi-Fi roaming) recover well within this window and
/// must stay invisible.
pub(crate) const WATCH_RETRY_GRACE: Duration = Duration::from_secs(10);

/// Build a watcher that never terminates on transient failures: errors are
/// still yielded to the consumer, but the next poll is delayed with kube's
/// default exponential backoff and the underlying state machine resumes or
/// relists.
pub(crate) fn resilient_watcher<K>(
    api: Api<K>,
    config: watcher::Config,
) -> impl futures_util::Stream<Item = std::result::Result<Event<K>, watcher::Error>>
where
    K: Resource + Clone + Debug + Send + 'static + for<'de> k8s_openapi::serde::Deserialize<'de>,
    K::DynamicType: Clone + Debug + Eq + std::hash::Hash,
{
    backoff_until_data(watcher(api, config), DefaultBackoff::default)
}

/// Whether an event proves the watcher is healthy. A failing list yields
/// `Err, Init, Err, Init, ...` and a list failing mid-pagination yields
/// `InitApply` pages before each error, so neither `Init` nor `InitApply` is
/// evidence of recovery.
pub(crate) fn is_healthy_event<T>(event: &Event<T>) -> bool {
    matches!(event, Event::InitDone | Event::Apply(_) | Event::Delete(_))
}

/// Delay sequence that only restarts when the stream delivers healthy data.
/// kube's `StreamBackoff` resets on any `Ok` item, which would keep retries at
/// the minimum delay while a list keeps failing after its `Init`.
pub(crate) struct ErrorBackoff<B> {
    new_backoff: fn() -> B,
    backoff: B,
}

impl<B: Iterator<Item = Duration>> ErrorBackoff<B> {
    pub(crate) fn new(new_backoff: fn() -> B) -> Self {
        Self {
            backoff: new_backoff(),
            new_backoff,
        }
    }

    /// The delay to apply before polling again after this item, if any.
    pub(crate) fn observe<T, E>(
        &mut self,
        item: &std::result::Result<Event<T>, E>,
    ) -> Option<Duration> {
        match item {
            Err(_) => self.backoff.next(),
            Ok(event) => {
                if is_healthy_event(event) {
                    self.backoff = (self.new_backoff)();
                }
                None
            }
        }
    }
}

/// Sleep the backoff after each error before polling the inner stream again.
pub(crate) fn backoff_until_data<S, T, E, B>(
    stream: S,
    new_backoff: fn() -> B,
) -> impl futures_util::Stream<Item = std::result::Result<Event<T>, E>>
where
    S: futures_util::Stream<Item = std::result::Result<Event<T>, E>>,
    B: Iterator<Item = Duration>,
{
    futures_util::stream::unfold(
        (
            Box::pin(stream),
            ErrorBackoff::new(new_backoff),
            None::<Duration>,
        ),
        |(mut stream, mut backoff, delay)| async move {
            if let Some(delay) = delay {
                tokio::time::sleep(delay).await;
            }
            let item = stream.next().await?;
            let delay = backoff.observe(&item);
            Some((item, (stream, backoff, delay)))
        },
    )
}

/// What a consumer should tell the UI after observing a stream item.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum RetryTransition {
    Unchanged,
    /// The watcher has been failing for longer than the grace period.
    Degraded,
    /// The watcher produced data again after having been reported as degraded.
    Recovered,
}

/// Tracks one watcher's failure streak and decides when it becomes visible.
#[derive(Debug, Clone)]
pub(crate) struct RetryTracker {
    grace: Duration,
    streak_started: Option<Instant>,
    reported: bool,
}

impl RetryTracker {
    pub(crate) fn new() -> Self {
        Self::with_grace(WATCH_RETRY_GRACE)
    }

    pub(crate) fn with_grace(grace: Duration) -> Self {
        Self {
            grace,
            streak_started: None,
            reported: false,
        }
    }

    /// Classify one stream item; only healthy events (see
    /// `is_healthy_event`) end a streak.
    pub(crate) fn observe<T, E: Debug>(
        &mut self,
        item: &std::result::Result<Event<T>, E>,
        label: &str,
    ) -> RetryTransition {
        match item {
            Ok(event) if is_healthy_event(event) => self.success(),
            Ok(_) => RetryTransition::Unchanged,
            Err(error) => {
                warn!("{label} watcher error, retrying: {error:?}");
                self.failure()
            }
        }
    }

    fn failure(&mut self) -> RetryTransition {
        self.failure_at(Instant::now())
    }

    /// Errors arrive at backoff intervals, so the notice is emitted by the
    /// first error observed at or after the grace period.
    pub(crate) fn failure_at(&mut self, now: Instant) -> RetryTransition {
        let started = *self.streak_started.get_or_insert(now);
        if !self.reported && now.saturating_duration_since(started) >= self.grace {
            self.reported = true;
            return RetryTransition::Degraded;
        }
        RetryTransition::Unchanged
    }

    fn success(&mut self) -> RetryTransition {
        self.streak_started = None;
        if std::mem::take(&mut self.reported) {
            RetryTransition::Recovered
        } else {
            RetryTransition::Unchanged
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failures_are_invisible_until_the_grace_period_elapses_and_clear_on_success() {
        let start = Instant::now();
        let mut tracker = RetryTracker::with_grace(Duration::from_secs(10));

        assert_eq!(tracker.failure_at(start), RetryTransition::Unchanged);
        assert_eq!(
            tracker.failure_at(start + Duration::from_secs(9)),
            RetryTransition::Unchanged
        );
        assert_eq!(tracker.success(), RetryTransition::Unchanged);

        assert_eq!(
            tracker.failure_at(start + Duration::from_secs(20)),
            RetryTransition::Unchanged,
            "a success restarts the streak"
        );
        assert_eq!(
            tracker.failure_at(start + Duration::from_secs(30)),
            RetryTransition::Degraded
        );
        assert_eq!(
            tracker.failure_at(start + Duration::from_secs(40)),
            RetryTransition::Unchanged,
            "a streak is reported once"
        );
        assert_eq!(tracker.success(), RetryTransition::Recovered);
        assert_eq!(tracker.success(), RetryTransition::Unchanged);
    }

    #[test]
    fn the_grace_boundary_is_inclusive_and_each_streak_reports_again() {
        let start = Instant::now();
        let grace = Duration::from_secs(10);

        let mut tracker = RetryTracker::with_grace(grace);
        tracker.failure_at(start);
        assert_eq!(
            tracker.failure_at(start + grace - Duration::from_nanos(1)),
            RetryTransition::Unchanged
        );
        assert_eq!(tracker.failure_at(start + grace), RetryTransition::Degraded);

        assert_eq!(tracker.success(), RetryTransition::Recovered);
        let second = start + Duration::from_secs(100);
        assert_eq!(tracker.failure_at(second), RetryTransition::Unchanged);
        assert_eq!(
            tracker.failure_at(second + grace),
            RetryTransition::Degraded,
            "a new streak is reported again"
        );
    }

    #[tokio::test]
    async fn init_is_neutral_so_a_failing_relist_loop_still_degrades() {
        let mut tracker = RetryTracker::with_grace(Duration::ZERO);
        let init = || Ok::<Event<()>, &str>(Event::Init);
        let error = || Err::<Event<()>, &str>("list failed");

        assert_eq!(tracker.observe(&error(), "test"), RetryTransition::Degraded);
        assert_eq!(tracker.observe(&init(), "test"), RetryTransition::Unchanged);
        assert_eq!(
            tracker.observe(&error(), "test"),
            RetryTransition::Unchanged
        );
        assert_eq!(tracker.observe(&init(), "test"), RetryTransition::Unchanged);
        assert_eq!(
            tracker.observe(&Ok::<Event<()>, &str>(Event::InitDone), "test"),
            RetryTransition::Recovered,
            "only real data ends the streak"
        );

        let mut patient = RetryTracker::with_grace(Duration::from_secs(3600));
        patient.observe(&error(), "test");
        patient.observe(&init(), "test");
        assert_eq!(
            patient.observe(&Ok::<Event<()>, &str>(Event::InitDone), "test"),
            RetryTransition::Unchanged
        );
    }

    #[test]
    fn init_and_init_apply_do_not_end_a_streak() {
        let mut tracker = RetryTracker::with_grace(Duration::ZERO);
        let error = Err::<Event<()>, &str>("list failed");
        assert_eq!(tracker.observe(&error, "test"), RetryTransition::Degraded);
        assert_eq!(
            tracker.observe(&Ok::<Event<()>, &str>(Event::InitApply(())), "test"),
            RetryTransition::Unchanged
        );
        assert_eq!(
            tracker.observe(&Ok::<Event<()>, &str>(Event::Apply(())), "test"),
            RetryTransition::Recovered
        );
    }

    fn counting_backoff() -> impl Iterator<Item = Duration> {
        (1..).map(Duration::from_millis)
    }

    #[test]
    fn backoff_keeps_growing_through_failing_relists_and_resets_on_healthy_data() {
        let mut backoff = ErrorBackoff::new(counting_backoff);
        let error = || Err::<Event<()>, &str>("list failed");
        let ok = |event| Ok::<Event<()>, &str>(event);
        let ms = |value| Some(Duration::from_millis(value));

        assert_eq!(backoff.observe(&error()), ms(1));
        assert_eq!(backoff.observe(&ok(Event::Init)), None);
        assert_eq!(backoff.observe(&error()), ms(2));
        assert_eq!(backoff.observe(&ok(Event::Init)), None);
        assert_eq!(backoff.observe(&ok(Event::InitApply(()))), None);
        assert_eq!(
            backoff.observe(&error()),
            ms(3),
            "Init and InitApply do not reset"
        );

        assert_eq!(backoff.observe(&ok(Event::InitDone)), None);
        assert_eq!(
            backoff.observe(&error()),
            ms(1),
            "healthy data restarts the sequence"
        );
    }

    #[tokio::test]
    async fn the_backoff_adapter_forwards_every_item_and_sleeps_only_after_errors() {
        let items: Vec<std::result::Result<Event<()>, &str>> =
            vec![Err("a"), Ok(Event::Init), Err("b"), Ok(Event::InitDone)];
        let started = std::time::Instant::now();
        let seen = backoff_until_data(futures_util::stream::iter(items), counting_backoff)
            .collect::<Vec<_>>()
            .await;

        assert_eq!(seen.len(), 4);
        assert!(started.elapsed() >= Duration::from_millis(1 + 2));
    }
}
