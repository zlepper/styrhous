use super::*;

#[test]
fn horizontal_fragment_uses_byte_boundaries_for_utf8_text() {
    let text = "aé日z";

    assert_eq!(character_column_range(text, 1, 3), 1..6);
    assert_eq!(&text[character_column_range(text, 1, 3)], "é日");
    assert_eq!(character_column_range(text, 3, 4), 6..7);
}

#[test]
fn pointer_position_excludes_metadata_prefix_and_restores_fragment_offset() {
    let text = "abcdef";

    assert_eq!(
        byte_offset_at_response_x(text, 70.0, 30.0, 0.0, 10.0),
        4,
        "line numbers and timestamps are before the text, not part of its cursor offset",
    );
    assert_eq!(
        byte_offset_at_response_x(text, 30.0, 10.0, 30.0, 10.0),
        5,
        "a horizontally clipped fragment restores its omitted character columns",
    );
}

#[test]
fn caret_origin_does_not_repeat_the_prefix_for_the_first_clipped_fragment() {
    let text = "x".repeat(512);
    let fragment = visible_text_fragment(&text, 0.0, 50.0, 10.0);
    let response_rect = egui::Rect::from_min_size(egui::pos2(100.0, 0.0), egui::vec2(50.0, 16.0));

    assert_eq!(fragment.byte_range.start, 0);
    assert!(fragment.byte_range.end < text.len());

    assert_eq!(
        rendered_log_text_left(response_rect, &fragment.byte_range, text.len(), 80.0),
        response_rect.left(),
        "a clipped fragment already rendered its line-number prefix separately",
    );
}

#[test]
fn caret_vertical_scroll_moves_only_when_the_caret_leaves_the_viewport() {
    let mut window = log_window(&["zero", "one", "two", "three", "four"]);
    select_log_position(&mut window, 2, 0);

    assert_eq!(caret_vertical_offset(&window, 20.0, 20.0, 10.0), 20.0);

    select_log_position(&mut window, 1, 0);
    assert_eq!(caret_vertical_offset(&window, 20.0, 20.0, 10.0), 10.0);

    select_log_position(&mut window, 4, 0);
    assert_eq!(caret_vertical_offset(&window, 20.0, 20.0, 10.0), 30.0);
}

#[test]
fn horizontal_fragment_limits_ascii_layout_to_the_visible_columns() {
    let text = "x".repeat(4_096);
    let fragment = visible_text_fragment(&text, 1_000.0, 120.0, 10.0);

    assert_eq!(fragment.byte_range, 88..124);
    assert_eq!(fragment.start_x, 880.0);
}

#[test]
fn wide_log_window_exposes_a_horizontal_scroll_range() {
    let context = egui::Context::default();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(320.0, 200.0),
        )),
        ..Default::default()
    };
    let mut window = log_window(&[&"x".repeat(4 * 1024)]);
    let mut display_options = LogDisplayOptions::default();
    let log_store = LogStoreService::default();
    let mut close_requested = false;

    let mut output = context.run_ui(input, |context| {
        show_log_window(
            context,
            &mut window,
            &mut display_options,
            &log_store,
            &mut close_requested,
        );
    });
    output.textures_delta.clear();

    assert!(window.horizontal_content_width > 320.0);
}

#[test]
fn layout_highlights_only_matching_segments() {
    let job = log_line_layout_job(
        4,
        None,
        "http http",
        &[],
        &[(0, 4), (5, 9)],
        LogDisplayOptions {
            show_line_numbers: true,
            ..Default::default()
        },
    );
    assert_eq!(job.sections.len(), 4);
    assert_eq!(job.text, "     4  http http");
}

#[test]
fn layout_preserves_ansi_style_while_highlighting_matches() {
    let style = Style::new()
        .fg_color(Some(AnsiColor::Red.into()))
        .underline();
    let job = log_line_layout_job(
        0,
        None,
        "error",
        &[AnsiStyleSpan {
            range: (0, 5),
            style,
        }],
        &[(1, 4)],
        LogDisplayOptions {
            show_line_numbers: true,
            ..Default::default()
        },
    );

    assert_eq!(job.text, "     0  error");
    assert_eq!(job.sections.len(), 4);
    assert_eq!(
        job.sections[1].format.color,
        ansi_palette_color(AnsiColor::Red)
    );
    assert!(!job.sections[1].format.underline.is_empty());
    assert_eq!(
        job.sections[2].format.background,
        egui::Color32::from_rgb(120, 53, 15)
    );
}

#[test]
fn wide_log_rows_are_not_wrapped_and_need_horizontal_scrolling() {
    let wide_line = "x".repeat(4 * 1024);
    let context = egui::Context::default();
    let input = || egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(320.0, 200.0),
        )),
        ..Default::default()
    };
    let mut first_scroll_output = None;
    let mut first_output_frame = context.run_ui(input(), |context| {
        egui::CentralPanel::default().show(context, |ui| {
            first_scroll_output = Some(show_wide_test_scroll_area(ui, &wide_line));
        });
    });
    first_output_frame.textures_delta.clear();

    let first_output = first_scroll_output.expect("scroll area was rendered");
    assert!(first_output.content_size.x > first_output.inner_rect.width());

    let mut scroll_input = input();
    scroll_input.events = vec![
        egui::Event::PointerMoved(first_output.inner_rect.center()),
        egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(-120.0, 0.0),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::default(),
        },
    ];
    let mut second_scroll_output = None;
    let mut second_output_frame = context.run_ui(scroll_input, |context| {
        egui::CentralPanel::default().show(context, |ui| {
            second_scroll_output = Some(show_wide_test_scroll_area(ui, &wide_line));
        });
    });
    second_output_frame.textures_delta.clear();

    assert!(
        second_scroll_output
            .expect("scroll area was rendered")
            .state
            .offset
            .x
            > 0.0
    );
}
