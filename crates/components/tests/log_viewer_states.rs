//! The states of the log viewer parts the real log windows need beyond
//! the C20 board: every stream pill, the source pill states, a log line with
//! long numbers, the empty states on a code surface and the draggable
//! problem ticks. The board (`c20_log_parts`) checks the parts' layout
//! against its oracle.

use std::cell::RefCell;
use std::rc::Rc;

use components::design::size;
use components::design::theme::{ThemeExt, Tone};
use components::test_support::{UiHarnessSnapshot, setup_egui};
use components::{
    CodeColor, CodeSpan, EmptyState, FloatingToolbar, LogHeader, LogLevel, LogLine, LogStream,
    ProblemTick, ProblemTicks, SourcePill, ToggleButton, VolumeBar, VolumeBars, VolumeStrip,
    icons::Icon,
};
use egui::{Ui, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

fn harness<'a>(size: egui::Vec2, scene: impl FnMut(&mut Ui) + 'a) -> Harness<'a> {
    let mut harness = Harness::builder().with_size(size).build_ui(scene);
    setup_egui(&mut harness);
    harness.set_size(size);
    harness
}

#[test]
fn every_stream_pill_has_its_words_tone_and_spinner() {
    let cases = [
        (LogStream::Following, "Following", Tone::Success, false),
        (LogStream::Spooling, "Spooling…", Tone::Neutral, true),
        (
            LogStream::Reconnecting { down: 1, total: 2 },
            "1 of 2 reconnecting",
            Tone::Warning,
            false,
        ),
        (
            LogStream::Reconnecting { down: 2, total: 2 },
            "Reconnecting…",
            Tone::Warning,
            false,
        ),
        (
            LogStream::Working("Connecting…".to_owned()),
            "Connecting…",
            Tone::Neutral,
            true,
        ),
        (
            LogStream::Custom("Stream failed".to_owned(), Tone::Danger),
            "Stream failed",
            Tone::Danger,
            false,
        ),
    ];
    for (stream, words, tone, busy) in cases {
        assert_eq!(stream.pill(), (words.to_owned(), tone), "{stream:?}");
        assert_eq!(stream.is_busy(), busy, "{stream:?}");
    }
}

#[test]
fn log_stream_states_snapshot() {
    let mut harness = harness(vec2(760.0, 420.0), |ui| {
        let theme = ui.tokens();
        ui.ctx()
            .layer_painter(egui::LayerId::background())
            .rect_filled(egui::Rect::EVERYTHING, 0, theme.surface.canvas);
        ui.set_width(720.0);
        ui.spacing_mut().item_spacing = vec2(8.0, 12.0);
        let streams = [
            LogStream::Spooling,
            LogStream::Reconnecting { down: 1, total: 2 },
            LogStream::Custom("Previous logs unavailable".to_owned(), Tone::Danger),
            LogStream::Working("Searching…".to_owned()),
        ];
        for (index, stream) in streams.into_iter().enumerate() {
            ui.push_id(index, |ui| {
                LogHeader::new("Pod", "api-0", "container api · kube-system", stream)
                    .show(ui, |_| {});
            });
        }
        let bars = [VolumeBar {
            height: 0.4,
            tone: None,
        }; 20];
        let _ = VolumeStrip::new("Volume · spooling", VolumeBars::new(&bars))
            .meta("backfill 2.1k · UTC")
            .height(size::VOLUME_STRIP)
            .source(SourcePill::spooling("api-0", 18_400))
            .source(SourcePill::offline("worker-0").detail("no data"))
            .show(ui);
    });
    harness.run_steps(3);
    harness.ui_harness("log_viewer/log_stream_states");
}

#[test]
fn an_unbuilt_source_pill_never_acts_and_says_so() {
    let clicked = Rc::new(RefCell::new(false));
    let seen = clicked.clone();
    let mut harness = harness(vec2(400.0, 120.0), move |ui| {
        let bars = [VolumeBar {
            height: 0.5,
            tone: None,
        }];
        let response = VolumeStrip::new("Volume", VolumeBars::new(&bars))
            .source(SourcePill::live("api-0", 3).not_implemented())
            .show(ui);
        *seen.borrow_mut() |= response.source_clicked.is_some();
    });
    harness.run();
    harness.get_by_label("api-0 · 3").click();
    harness.run();
    assert!(!*clicked.borrow());
}

#[test]
fn a_log_line_fits_long_numbers_and_starts_its_message_where_it_says() {
    let measured = Rc::new(RefCell::new(None));
    let seen = measured.clone();
    let mut harness = harness(vec2(900.0, 120.0), move |ui| {
        ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
        let wide = 8.0 * size::LOG_LINE_NUMBER / 5.0;
        let line = LogLine::new([CodeSpan::new("message", CodeColor::Text)])
            .number(12_345_678)
            .number_width(wide)
            .time("15:22:10.000")
            .source("worker-0", 1)
            .source_width(120.0)
            .level(LogLevel::Warn)
            .message_offset(30.0);
        let prefix = line.prefix_width(ui);
        let shown = line.show(ui);
        *seen.borrow_mut() = Some((prefix, shown.response.rect, shown.message_left, wide));
    });
    harness.run();
    let (prefix, rect, message_left, wide) = measured.borrow().expect("the line was shown");
    assert_eq!(message_left, rect.left() + prefix + 30.0);
    let number = harness.get_by_label("12345678").rect();
    assert!(
        number.width() <= wide + 0.5,
        "the number fits the widened column"
    );
    assert!(
        number.left() >= rect.left() + size::LOG_ROW_PADDING_X - 0.5,
        "and does not spill into the padding"
    );
    let time = harness.get_by_label("15:22:10.000").rect();
    assert!(
        number.right() < time.left(),
        "a long number stops before the time"
    );
    harness.ui_harness("log_viewer/log_line_long_number");
}

#[test]
fn a_line_without_a_level_can_keep_the_badge_column() {
    let lefts = Rc::new(RefCell::new(Vec::new()));
    let seen = lefts.clone();
    let mut harness = harness(vec2(400.0, 120.0), move |ui| {
        ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
        let with = LogLine::new([CodeSpan::text("with")])
            .level(LogLevel::Info)
            .show(ui);
        let without = LogLine::new([CodeSpan::text("without")])
            .reserve_level(true)
            .show(ui);
        seen.borrow_mut()
            .extend([with.message_left, without.message_left]);
    });
    harness.run();
    let lefts = lefts.borrow();
    assert_eq!(lefts[0], lefts[1], "both messages start in one column");
}

#[test]
fn empty_states_on_a_code_surface_snapshot() {
    let mut harness = harness(vec2(720.0, 360.0), |ui| {
        let theme = ui.tokens();
        ui.painter()
            .rect_filled(ui.max_rect(), 0, theme.code.surface);
        let half = ui.max_rect().width() / 2.0;
        let (left, right) = (
            egui::Rect::from_min_size(ui.max_rect().min, vec2(half, 360.0)),
            egui::Rect::from_min_size(ui.max_rect().min + vec2(half, 0.0), vec2(half, 360.0)),
        );
        let mut left_ui = ui.new_child(egui::UiBuilder::new().max_rect(left));
        EmptyState::error("No previous logs for api-0")
            .description("The container hasn’t restarted, so there are none.")
            .action_with_icon(Icon::ArrowUturnLeft, "Show current logs")
            .on_code_surface(40.0)
            .show(&mut left_ui);
        let mut right_ui = ui.new_child(egui::UiBuilder::new().max_rect(right));
        EmptyState::loading("Spooling logs…")
            .description("18,400 lines received · newest shown when complete")
            .spinner(components::Spinner::new().rotation(0.0))
            .on_code_surface(60.0)
            .show(&mut right_ui);
    });
    harness.run_steps(2);
    harness.ui_harness("log_viewer/empty_states_on_a_code_surface");
}

#[test]
fn a_draggable_track_reports_where_it_was_dragged_to() {
    let scrubbed = Rc::new(RefCell::new(None));
    let seen = scrubbed.clone();
    let mut harness = harness(vec2(120.0, 240.0), move |ui| {
        let ticks = [ProblemTick {
            position: 0.1,
            tone: Tone::Danger,
        }];
        let response = ProblemTicks::new(&ticks)
            .id_salt("log")
            .draggable()
            .thumb(0.0, 0.2)
            .show(ui, 200.0);
        if let Some(fraction) = response.scrubbed_to {
            *seen.borrow_mut() = Some(fraction);
        }
    });
    harness.run();
    let track = harness.get_by_label("1 problem").rect();
    let start = track.center_top() + vec2(0.0, 150.0);
    harness.event(egui::Event::PointerMoved(start));
    harness.event(egui::Event::PointerButton {
        pos: start,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run_steps(2);
    let moved = start + vec2(0.0, 10.0);
    harness.event(egui::Event::PointerMoved(moved));
    harness.run_steps(2);
    let fraction = scrubbed.borrow().expect("dragging reports a position");
    assert!((fraction - 160.0 / 200.0).abs() < 0.02, "got {fraction}");
}

#[test]
fn a_sized_floating_toolbar_gives_the_last_control_the_room_left() {
    let widths = Rc::new(RefCell::new(None));
    let seen = widths.clone();
    let mut harness = harness(vec2(900.0, 200.0), move |ui| {
        let seen = seen.clone();
        FloatingToolbar::show_floating(
            ui.ctx(),
            "sized",
            640.0,
            egui::Layout::right_to_left(egui::Align::Center),
            move |ui| {
                ToggleButton::text("Follow", false).show(ui);
                let room = ui.available_width();
                let (rect, _) = ui.allocate_exact_size(vec2(room, 36.0), egui::Sense::hover());
                *seen.borrow_mut() = Some(rect.width());
            },
        );
    });
    harness.run_steps(3);
    let room = widths.borrow().expect("the toolbar was shown");
    assert!(room > 400.0 && room < 640.0, "got {room}");
    let follow = harness.get_by_label("Follow").rect();
    assert!(
        follow.right() > 450.0 + 300.0,
        "Follow sits at the end of the bar"
    );
}
