use super::blade_text::{blade_caption, gap};
use super::global_blade::{
    GlobalBladeContent, GlobalBladeEffect, GlobalBladeEffectContext, GlobalBladeNavigation,
    GlobalBladeRenderContext, GlobalBladeRenderResult,
};
use super::operation_outcome::status_tile;
use super::state::OperationHistoryDisplayEntry;
use components::design::theme::{ThemeExt, Tone};
use components::design::{size, space, stroke, type_scale};
use components::icons::Icon;
use components::tooltip::elided_label;
use components::{
    BladeLayer, Button, ButtonSize, Card, Segmented, SegmentedSize, TileSize, paint_status_dot,
};
use egui::{Align, Rect, Sense, pos2, vec2};
use time::{Duration, OffsetDateTime};

/// A history entry's status tile (A12 `.tile.sm`).
const STATUS_TILE: TileSize = TileSize::Sm;
/// An entry's padding (`padding: 14px 16px`) and the gap to its words.
const ROW_PADDING: egui::Vec2 = vec2(space::S16, 14.0);
const ROW_GAP: f32 = space::S12;
/// The unread lamp (`.dot.acc`, 6 points) and where it sits in the row.
const UNREAD_DOT: f32 = 6.0;
const UNREAD_DOT_OFFSET: egui::Vec2 = vec2(6.0, 24.0);

/// Which entries the blade lists.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum HistoryFilter {
    #[default]
    All,
    /// Warnings and failures.
    Problems,
}

/// The session-scoped record of resource operation outcomes. It has no
/// destructive controls: reconnecting a cluster starts a new session.
/// Entries stay unread, marked by a lamp, until "Mark all read".
#[derive(Debug, Default)]
pub(crate) struct NotificationHistoryBlade {
    filter: HistoryFilter,
    mark_all_read: bool,
}

/// Mark every outcome read.
#[derive(Debug)]
struct MarkAllRead;

impl GlobalBladeEffect for MarkAllRead {
    fn apply(
        self: Box<Self>,
        context: &mut GlobalBladeEffectContext<'_>,
        _navigation: &mut GlobalBladeNavigation<'_>,
    ) {
        context.ui_state.mark_operation_history_read();
    }
}

impl GlobalBladeContent for NotificationHistoryBlade {
    fn description(&self) -> components::BladeDescription {
        components::BladeDescription::named("Notifications")
    }

    fn sidebar_destination(&self) -> Option<crate::ui::global_blade::SidebarDestination> {
        Some(crate::ui::global_blade::SidebarDestination::Notifications)
    }

    fn take_effect(&mut self) -> Option<Box<dyn GlobalBladeEffect>> {
        std::mem::take(&mut self.mark_all_read)
            .then(|| Box::new(MarkAllRead) as Box<dyn GlobalBladeEffect>)
    }

    fn render_header(
        &mut self,
        ui: &mut egui::Ui,
        _layer: BladeLayer,
        context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult {
        let has_unread = context.unread_operation_count() > 0;
        self.mark_all_read |=
            components::blade_title_with_actions(ui, None, "Notifications", |ui| {
                Button::ghost("Mark all read")
                    .size(ButtonSize::Sm)
                    .leading_icon(Icon::Check)
                    .enabled(has_unread)
                    .show(ui)
                    .clicked()
            });
        GlobalBladeRenderResult::default()
    }

    fn render_body(
        &mut self,
        ui: &mut egui::Ui,
        _layer: BladeLayer,
        context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult {
        let theme = ui.tokens();
        ui.allocate_ui_with_layout(
            vec2(ui.available_width(), size::CONTROL_SM),
            egui::Layout::right_to_left(Align::Center),
            |ui| {
                Segmented::new(&mut self.filter)
                    .size(SegmentedSize::Sm)
                    .label("Show")
                    .option(HistoryFilter::All, "All")
                    .option(HistoryFilter::Problems, "Problems")
                    .show(ui);
                // The sentence takes the rest of the row, centred on the control.
                ui.with_layout(egui::Layout::left_to_right(Align::Center), |ui| {
                    type_scale::BODY.label(
                        ui,
                        "Outcomes of actions from this session.",
                        theme.text.secondary,
                    );
                });
            },
        );
        gap(ui, space::S16);

        let now = super::clock::now(ui.ctx());
        let all_entries = context.operation_history_entries();
        let entries = all_entries
            .iter()
            .filter(|display| match self.filter {
                HistoryFilter::All => true,
                HistoryFilter::Problems => display.entry.outcome.is_problem(),
            })
            .collect::<Vec<_>>();
        if entries.is_empty() {
            let message = match self.filter {
                _ if all_entries.is_empty() => "Resource operation outcomes will appear here.",
                _ => "No problems in this session.",
            };
            type_scale::BODY.label(ui, message, theme.text.muted);
            return GlobalBladeRenderResult::default();
        }

        let offset = context.day_offset();
        let (recent, earlier): (Vec<_>, Vec<_>) = entries
            .into_iter()
            .partition(|display| is_today(now, display.entry.wall_time, offset));
        for (caption, group, first) in [("Today", recent, true), ("Earlier", earlier, false)] {
            if group.is_empty() {
                continue;
            }
            if !first {
                gap(ui, space::S24);
            }
            blade_caption(ui, caption);
            show_group(ui, &group, now);
        }
        GlobalBladeRenderResult::default()
    }
}

/// The entries of one group in a card, a hairline between neighbours.
fn show_group(ui: &mut egui::Ui, group: &[&OperationHistoryDisplayEntry], now: OffsetDateTime) {
    Card::new().padding(0).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for (index, display) in group.iter().enumerate() {
            if index > 0 {
                let y = ui.cursor().top();
                ui.painter().hline(
                    ui.max_rect().x_range(),
                    y + size::HAIRLINE / 2.0,
                    stroke::subtle_divider(&ui.tokens()),
                );
                ui.add_space(size::HAIRLINE);
            }
            notification_row(ui, display, now);
        }
    });
}

/// One entry: its status tile, then the title with its age, the resource it
/// concerns and the first line of its details.
fn notification_row(
    ui: &mut egui::Ui,
    display: &OperationHistoryDisplayEntry,
    now: OffsetDateTime,
) {
    let entry = &display.entry;
    let theme = ui.tokens();
    let title_role = type_scale::UI_STRONG;
    let line_role = type_scale::META;
    let details_gap = if entry.details.is_some() {
        space::S4 + line_role.line_height()
    } else {
        0.0
    };
    let text_height = title_role.line_height() + line_role.line_height() + details_gap;
    let height = ROW_PADDING.y * 2.0 + text_height.max(STATUS_TILE.side());
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());

    if entry.unread {
        paint_status_dot(
            ui.painter(),
            &theme,
            Rect::from_min_size(row.min + UNREAD_DOT_OFFSET, vec2(UNREAD_DOT, UNREAD_DOT)),
            Tone::Accent,
        );
    }
    status_tile(entry.outcome).size(STATUS_TILE).paint(
        ui,
        Rect::from_min_size(
            row.min + ROW_PADDING,
            vec2(STATUS_TILE.side(), STATUS_TILE.side()),
        ),
    );

    let text_left = row.left() + ROW_PADDING.x + STATUS_TILE.side() + ROW_GAP;
    let text_right = row.right() - ROW_PADDING.x;
    let mut top = row.top() + ROW_PADDING.y;
    let line = |ui: &mut egui::Ui,
                role: type_scale::TextRole,
                text: &str,
                color,
                top: f32,
                left: f32,
                right: f32| {
        let rect = Rect::from_min_size(
            pos2(left, top),
            vec2((right - left).max(0.0), role.line_height()),
        );
        let mut line_ui = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        elided_label(&mut line_ui, role, text, color);
    };

    // The title, with how long ago it happened at the row's end, centred on
    // the title's line.
    let age = relative_time(now, entry.wall_time);
    let age_width = type_scale::META
        .galley(ui.ctx(), &age, theme.text.muted, f32::INFINITY)
        .rect
        .width();
    let age_left = text_right - age_width;
    let age_top = top + (title_role.line_height() - type_scale::META.line_height()) / 2.0;
    line(
        ui,
        title_role,
        &entry.title,
        theme.text.primary,
        top,
        text_left,
        age_left - space::S8,
    );
    line(
        ui,
        type_scale::META,
        &age,
        theme.text.muted,
        age_top,
        age_left,
        text_right,
    );
    top += title_role.line_height();

    let mut subject = vec![entry.resource.as_str()];
    subject.extend(entry.namespace.as_deref());
    subject.push(display.cluster_name.as_str());
    line(
        ui,
        line_role,
        &subject.join(" · "),
        theme.text.secondary,
        top,
        text_left,
        text_right,
    );
    top += line_role.line_height();

    if let Some(details) = &entry.details {
        top += space::S4;
        let summary = details.lines().next().unwrap_or_default();
        line(
            ui,
            line_role,
            summary,
            theme.text.muted,
            top,
            text_left,
            text_right,
        );
    }
}

/// Whether `then` falls on the same calendar day as `now`, where days start
/// by `offset`.
fn is_today(now: OffsetDateTime, then: OffsetDateTime, offset: time::UtcOffset) -> bool {
    now.to_offset(offset).date() == then.to_offset(offset).date()
}

/// How long ago `then` was: "Just now", "2 min", "1 h", "3 d".
fn relative_time(now: OffsetDateTime, then: OffsetDateTime) -> String {
    let elapsed = now - then;
    if elapsed < Duration::minutes(1) {
        "Just now".to_owned()
    } else if elapsed < Duration::hours(1) {
        format!("{} min", elapsed.whole_minutes())
    } else if elapsed < Duration::days(1) {
        format!("{} h", elapsed.whole_hours())
    } else {
        format!("{} d", elapsed.whole_days())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn today_is_the_calendar_day_not_the_last_day() {
        let utc = time::UtcOffset::UTC;
        let now = time::Date::from_calendar_date(2026, time::Month::June, 1)
            .unwrap()
            .with_hms(0, 30, 0)
            .unwrap()
            .assume_utc();
        assert!(is_today(now, now, utc));
        assert!(
            is_today(now, now - Duration::minutes(30), utc),
            "00:00 is today"
        );
        assert!(
            !is_today(now, now - Duration::minutes(31), utc),
            "23:59 is yesterday"
        );
        assert!(!is_today(now, now - Duration::days(1), utc));
        // The same instants fall on other days an hour east.
        let east = time::UtcOffset::from_hms(1, 0, 0).unwrap();
        assert!(is_today(now, now - Duration::minutes(90), east));
    }

    #[test]
    fn relative_times_use_the_largest_whole_unit() {
        let now = OffsetDateTime::UNIX_EPOCH + Duration::days(10);
        for (ago, expected) in [
            (Duration::seconds(5), "Just now"),
            (Duration::seconds(59), "Just now"),
            (Duration::minutes(2), "2 min"),
            (Duration::minutes(59), "59 min"),
            (Duration::minutes(60), "1 h"),
            (Duration::hours(23), "23 h"),
            (Duration::hours(24), "1 d"),
            (Duration::days(3), "3 d"),
            // A clock that went backwards reads as just now.
            (Duration::seconds(-30), "Just now"),
        ] {
            assert_eq!(relative_time(now, now - ago), expected, "{ago}");
        }
    }
}
