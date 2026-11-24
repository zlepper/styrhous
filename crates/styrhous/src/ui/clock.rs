//! The wall clock the UI measures ages and relative times against, and the
//! time zone its absolute times are shown in.
//!
//! [`UiState::clock`](super::state::UiState) is the source: the system clock
//! in the application, a fixed instant in tests so "220d" or "3h" stay
//! deterministic. Each frame publishes its reading to the egui context, so
//! widgets deep in a screen read the same "now" without threading it through
//! every call.
//!
//! Absolute times (a log row, a Helm release, the "Today" notifications) are
//! shown in the user's local time zone. On Unix the `time` crate can only read
//! the machine's offset while the process has a single thread, so
//! [`init_system_zone`] measures it once, at the very start of `main`, for
//! every instant of the years around that moment; [`Zone::System`] then
//! answers from that table. An application that runs for longer than the
//! table spans, or a machine whose zone is changed while it runs, keeps the
//! last offset it measured.

use std::sync::OnceLock;

use time::{OffsetDateTime, UtcOffset};

/// Where the UI's wall-clock time comes from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum WallClock {
    /// The system clock, in the machine's time zone.
    #[default]
    System,
    /// A fixed instant (deterministic tests and design fixtures). Its own
    /// offset is the time zone: UTC unless a test builds it with another.
    #[cfg_attr(not(test), allow(dead_code))]
    Fixed(OffsetDateTime),
}

impl WallClock {
    pub(crate) fn now(self) -> OffsetDateTime {
        match self {
            Self::System => OffsetDateTime::now_utc(),
            Self::Fixed(now) => now,
        }
    }

    /// The time zone absolute times are shown in.
    pub(crate) fn zone(self) -> Zone {
        match self {
            Self::System => Zone::System,
            Self::Fixed(now) => Zone::Fixed(now.offset()),
        }
    }
}

/// A time zone: how far from UTC the wall clock reads at an instant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Zone {
    /// The machine's zone, as [`init_system_zone`] measured it (UTC when it
    /// did not or could not).
    System,
    /// One offset at every instant.
    #[cfg_attr(not(test), allow(dead_code))]
    Fixed(UtcOffset),
}

impl Zone {
    /// The offset from UTC at `instant`.
    pub(crate) fn offset_at(self, instant: OffsetDateTime) -> UtcOffset {
        match self {
            Self::System => SYSTEM_OFFSETS.get().map_or(UtcOffset::UTC, |offsets| {
                offsets.at(instant.unix_timestamp())
            }),
            Self::Fixed(offset) => offset,
        }
    }

    /// `instant` as the wall clock reads in this zone.
    pub(crate) fn local(self, instant: OffsetDateTime) -> OffsetDateTime {
        // A timestamp at the very end of the supported years has no local reading.
        instant
            .checked_to_offset(self.offset_at(instant))
            .unwrap_or(instant)
    }

    /// The unix timestamp `seconds` as the wall clock reads in this zone.
    pub(crate) fn local_unix(self, seconds: i64) -> Option<OffsetDateTime> {
        OffsetDateTime::from_unix_timestamp(seconds)
            .ok()
            .map(|instant| self.local(instant))
    }

    /// Whether `a` and `b` fall on the same calendar day here.
    pub(crate) fn same_day(self, a: OffsetDateTime, b: OffsetDateTime) -> bool {
        self.local(a).date() == self.local(b).date()
    }
}

/// "+02:00": `offset` as it follows a time.
pub(crate) fn format_offset(offset: UtcOffset) -> String {
    let (hours, minutes, _) = offset.as_hms();
    let sign = if offset.is_negative() { '-' } else { '+' };
    format!("{sign}{:02}:{:02}", hours.abs(), minutes.abs())
}

/// "2026-08-08 17:22:17.143 +02:00" (without the fraction when `millis` is
/// off): `local` as its date, time and offset.
pub(crate) fn format_date_time(local: OffsetDateTime, millis: bool) -> String {
    let fraction = if millis {
        format!(".{:03}", local.millisecond())
    } else {
        String::new()
    };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}{fraction} {}",
        local.year(),
        u8::from(local.month()),
        local.day(),
        local.hour(),
        local.minute(),
        local.second(),
        format_offset(local.offset())
    )
}

/// Every offset change of the machine's zone in a span of years around the
/// moment it was measured.
#[derive(Debug)]
struct SystemOffsets {
    /// The offset before the first change.
    initial: UtcOffset,
    /// `(unix second the offset starts, offset)`, oldest first.
    changes: Vec<(i64, UtcOffset)>,
}

/// How many years either side of the measurement the table reaches.
const MEASURED_YEARS: i64 = 3;
const DAY: i64 = 24 * 60 * 60;

impl SystemOffsets {
    /// Measure `offset_at` (the offset at a unix second, if it can be told)
    /// a day at a time from [`MEASURED_YEARS`] before `now` to as many after,
    /// and the second of each change by bisection. A zone that cannot be
    /// read is UTC throughout.
    fn measure(now: i64, offset_at: impl Fn(i64) -> Option<UtcOffset>) -> Self {
        let start = now - MEASURED_YEARS * 366 * DAY;
        let end = now + MEASURED_YEARS * 366 * DAY;
        let Some(initial) = offset_at(start) else {
            return Self::utc();
        };
        let mut changes = Vec::new();
        let (mut at, mut offset) = (start, initial);
        while at < end {
            let next = (at + DAY).min(end);
            let Some(next_offset) = offset_at(next) else {
                return Self::utc();
            };
            if next_offset != offset {
                // The first second that reads `next_offset`.
                let (mut before, mut after) = (at, next);
                while after - before > 1 {
                    let middle = before + (after - before) / 2;
                    match offset_at(middle) {
                        Some(found) if found == offset => before = middle,
                        _ => after = middle,
                    }
                }
                changes.push((after, next_offset));
                offset = next_offset;
            }
            at = next;
        }
        Self { initial, changes }
    }

    fn utc() -> Self {
        Self {
            initial: UtcOffset::UTC,
            changes: Vec::new(),
        }
    }

    /// The offset at unix second `at`; before or after the measured span,
    /// that of its nearest end.
    fn at(&self, at: i64) -> UtcOffset {
        match self.changes.partition_point(|(start, _)| *start <= at) {
            0 => self.initial,
            index => self.changes[index - 1].1,
        }
    }
}

static SYSTEM_OFFSETS: OnceLock<SystemOffsets> = OnceLock::new();

/// Measure the machine's time zone for [`Zone::System`]. Call it first thing
/// in `main`: the `time` crate refuses to read the offset once a second
/// thread exists (another thread may be changing the environment it reads
/// from), so before that is the only sound moment.
pub(crate) fn init_system_zone() {
    SYSTEM_OFFSETS.get_or_init(|| {
        SystemOffsets::measure(OffsetDateTime::now_utc().unix_timestamp(), |seconds| {
            let instant = OffsetDateTime::from_unix_timestamp(seconds).ok()?;
            UtcOffset::local_offset_at(instant).ok()
        })
    });
}

/// The context key of the frame's published time and zone.
#[derive(Clone, Copy)]
struct FrameTime(OffsetDateTime, Zone);

/// Publish `clock`'s reading as the time of the frame being drawn.
pub(crate) fn publish(ctx: &egui::Context, clock: WallClock) {
    ctx.data_mut(|data| data.insert_temp(egui::Id::NULL, FrameTime(clock.now(), clock.zone())));
}

/// The time of the frame being drawn: what [`publish`] stored, or the system
/// clock outside a published frame.
pub(crate) fn now(ctx: &egui::Context) -> OffsetDateTime {
    ctx.data(|data| data.get_temp::<FrameTime>(egui::Id::NULL))
        .map_or_else(OffsetDateTime::now_utc, |time| time.0)
}

/// The time zone of the frame being drawn: what [`publish`] stored, or the
/// system's outside a published frame.
pub(crate) fn zone(ctx: &egui::Context) -> Zone {
    ctx.data(|data| data.get_temp::<FrameTime>(egui::Id::NULL))
        .map_or(Zone::System, |time| time.1)
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

    fn plus(hours: i8) -> UtcOffset {
        UtcOffset::from_hms(hours, 0, 0).unwrap()
    }

    #[test]
    fn a_fixed_clock_is_in_the_zone_of_its_own_offset() {
        let utc = WallClock::Fixed(OffsetDateTime::UNIX_EPOCH);
        let east = WallClock::Fixed(OffsetDateTime::UNIX_EPOCH.to_offset(plus(2)));
        assert_eq!(utc.zone(), Zone::Fixed(UtcOffset::UTC));
        assert_eq!(east.zone(), Zone::Fixed(plus(2)));

        let ctx = egui::Context::default();
        publish(&ctx, east);
        assert_eq!(zone(&ctx), Zone::Fixed(plus(2)));
        let local = zone(&ctx).local(OffsetDateTime::UNIX_EPOCH);
        assert_eq!((local.hour(), local.offset()), (2, plus(2)));
    }

    #[test]
    fn a_timestamp_at_the_end_of_time_stays_as_it_is() {
        let last = OffsetDateTime::parse(
            "9999-12-31T23:59:59Z",
            &time::format_description::well_known::Rfc3339,
        )
        .unwrap();
        assert_eq!(Zone::Fixed(plus(2)).local(last), last);
    }

    #[test]
    fn dates_and_times_read_with_their_offset() {
        let zone = Zone::Fixed(plus(2));
        let instant = OffsetDateTime::parse(
            "2026-08-08T15:22:17.143Z",
            &time::format_description::well_known::Rfc3339,
        )
        .unwrap();
        assert_eq!(
            format_date_time(zone.local(instant), true),
            "2026-08-08 17:22:17.143 +02:00"
        );
        assert_eq!(
            format_date_time(instant, false),
            "2026-08-08 15:22:17 +00:00"
        );
        let west = UtcOffset::from_hms(-3, -30, 0).unwrap();
        assert_eq!(format_offset(west), "-03:30");
    }

    #[test]
    fn a_calendar_day_starts_at_local_midnight() {
        let zone = Zone::Fixed(plus(2));
        let now = OffsetDateTime::UNIX_EPOCH + time::Duration::days(10);
        // 00:30 local is 22:30 UTC the day before.
        let now = now - time::Duration::minutes(90);
        assert!(zone.same_day(now, now - time::Duration::minutes(30)));
        assert!(!zone.same_day(now, now - time::Duration::minutes(31)));
        assert!(Zone::Fixed(UtcOffset::UTC).same_day(now, now + time::Duration::hours(1)));
    }

    /// A zone at +1 that is at +2 from 1 April to 1 October of every year.
    fn summer_time(seconds: i64) -> Option<UtcOffset> {
        let day_of_year = OffsetDateTime::from_unix_timestamp(seconds).ok()?.ordinal();
        Some(plus(if (91..274).contains(&day_of_year) {
            2
        } else {
            1
        }))
    }

    #[test]
    fn measured_offsets_follow_each_change_of_the_zone() {
        let now = OffsetDateTime::parse(
            "2026-11-24T12:00:00Z",
            &time::format_description::well_known::Rfc3339,
        )
        .unwrap()
        .unix_timestamp();
        let offsets = SystemOffsets::measure(now, summer_time);
        let at = |text: &str| {
            offsets.at(
                OffsetDateTime::parse(text, &time::format_description::well_known::Rfc3339)
                    .unwrap()
                    .unix_timestamp(),
            )
        };
        assert_eq!(at("2026-11-24T12:00:00Z"), plus(1), "now: winter");
        assert_eq!(at("2026-08-08T12:00:00Z"), plus(2), "months ago: summer");
        assert_eq!(at("2025-07-01T00:00:00Z"), plus(2));
        assert_eq!(at("2027-06-01T00:00:00Z"), plus(2), "ahead: summer");
        // The change is found to the second.
        assert_eq!(at("2026-03-31T23:59:59Z"), plus(1));
        assert_eq!(at("2026-04-01T00:00:00Z"), plus(2));
        // Beyond the measured years the nearest end holds.
        assert_eq!(at("2000-01-01T00:00:00Z"), plus(1));
        assert_eq!(at("2100-08-01T00:00:00Z"), plus(1));
    }

    #[test]
    fn an_unreadable_zone_is_utc() {
        let offsets = SystemOffsets::measure(0, |_| None);
        assert_eq!(offsets.at(1_000_000), UtcOffset::UTC);
    }

    #[test]
    fn an_unpublished_context_reads_the_system_clock() {
        let before = OffsetDateTime::now_utc();
        let read = now(&egui::Context::default());
        assert!(read >= before && read <= OffsetDateTime::now_utc());
    }
}
