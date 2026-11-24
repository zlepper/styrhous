//! The wall clock the UI measures ages and relative times against.
//!
//! [`UiState::clock`](super::state::UiState) is the source: the system clock
//! in the application, a fixed instant in tests so "220d" or "3h" stay
//! deterministic. Each frame publishes its reading to the egui context, so
//! widgets deep in a screen read the same "now" without threading it through
//! every call.

use time::OffsetDateTime;

/// Where the UI's wall-clock time comes from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum WallClock {
    /// The system clock.
    #[default]
    System,
    /// A fixed instant (deterministic tests and design fixtures).
    #[cfg_attr(not(test), allow(dead_code))]
    Fixed(OffsetDateTime),
}

impl WallClock {
    /// The offset that decides where a calendar day starts. UTC for now: the
    /// `time` crate's `local-offset` feature is not enabled, so the machine's
    /// own offset cannot be read. Callers take the offset from here so that
    /// changes in one place.
    pub(crate) fn day_offset(self) -> time::UtcOffset {
        time::UtcOffset::UTC
    }

    pub(crate) fn now(self) -> OffsetDateTime {
        match self {
            Self::System => OffsetDateTime::now_utc(),
            Self::Fixed(now) => now,
        }
    }
}

/// The context key of the frame's published time.
#[derive(Clone, Copy)]
struct FrameTime(OffsetDateTime);

/// Publish `clock`'s reading as the time of the frame being drawn.
pub(crate) fn publish(ctx: &egui::Context, clock: WallClock) {
    ctx.data_mut(|data| data.insert_temp(egui::Id::NULL, FrameTime(clock.now())));
}

/// The time of the frame being drawn: what [`publish`] stored, or the system
/// clock outside a published frame.
pub(crate) fn now(ctx: &egui::Context) -> OffsetDateTime {
    ctx.data(|data| data.get_temp::<FrameTime>(egui::Id::NULL))
        .map_or_else(OffsetDateTime::now_utc, |time| time.0)
}

/// A resource's age at the time of the frame being drawn ("220d", "3h"), or "Unknown".
pub(crate) fn frame_age(ctx: &egui::Context, created: Option<OffsetDateTime>) -> String {
    crate::minimal_resource::format_age(created, now(ctx))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_published_clock_is_what_widgets_read_as_now() {
        let ctx = egui::Context::default();
        let fixed = OffsetDateTime::UNIX_EPOCH + time::Duration::days(220);
        publish(&ctx, WallClock::Fixed(fixed));
        assert_eq!(now(&ctx), fixed);
    }

    #[test]
    fn frame_age_reads_the_published_clock() {
        let ctx = egui::Context::default();
        let created = OffsetDateTime::UNIX_EPOCH;
        publish(&ctx, WallClock::Fixed(created + time::Duration::days(220)));
        assert_eq!(frame_age(&ctx, Some(created)), "220d");
        assert_eq!(frame_age(&ctx, None), "Unknown");
    }

    #[test]
    fn an_unpublished_context_reads_the_system_clock() {
        let before = OffsetDateTime::now_utc();
        let read = now(&egui::Context::default());
        assert!(read >= before && read <= OffsetDateTime::now_utc());
    }
}
