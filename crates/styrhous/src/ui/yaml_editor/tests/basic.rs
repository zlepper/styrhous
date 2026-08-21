use super::*;

#[test]
fn yaml_highlighting_uses_the_yaml_language() {
    let ctx = egui::Context::default();
    let theme = CodeTheme::dark(typography::MONOSPACE_SIZE);
    let job = highlight(
        &ctx,
        &egui::Style::default(),
        &theme,
        "kind: ConfigMap",
        "yaml",
    );

    assert!(!job.sections.is_empty());
}

#[test]
fn search_highlights_preserve_syntax_formatting() {
    let mut job = egui::text::LayoutJob::default();
    let key_format = egui::TextFormat {
        color: gray::_400,
        ..Default::default()
    };
    let value_format = egui::TextFormat {
        color: gray::_100,
        ..Default::default()
    };
    job.append("kind: ", 0.0, key_format);
    job.append("ConfigMap", 0.0, value_format.clone());

    let matches = [0..4, 6..15];
    apply_search_highlights(&mut job, &matches, matches.get(1));

    let matched = job
        .sections
        .iter()
        .find(|section| section.byte_range.start.0 == 6 && section.byte_range.end.0 == 15)
        .expect("matching section is present");
    assert_eq!(matched.format.color, value_format.color);
    assert_eq!(matched.format.background, search::ACTIVE_MATCH_BACKGROUND);
}

#[test]
fn literal_search_is_case_insensitive_and_returns_utf8_byte_ranges() {
    let text = "kind: ConfigMap\nmetadata:\n  name: CØNFIGMAP";

    assert_eq!(
        find_matches(text, "configmap", false).expect("literal search is valid"),
        vec![6..15]
    );
    assert_eq!(
        find_matches(text, "cønfigmap", false).expect("literal search is valid"),
        vec![34..44]
    );
}

#[test]
fn regex_search_can_match_across_yaml_lines() {
    assert_eq!(
        find_matches("kind: ConfigMap\nmetadata:", "ConfigMap\\nmetadata", true)
            .expect("regex search is valid"),
        vec![6..24]
    );
}

#[test]
fn line_number_layout_job_numbers_and_marks_diagnostic_lines() {
    let diagnostics = vec![YamlDiagnostic {
        path: "/spec/containers/0/image".into(),
        message: "image is required".into(),
        line: Some(10),
        range: None,
    }];

    let job = line_number_layout_job(12, &diagnostics);

    assert_eq!(job.text, "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n11\n12");
    let diagnostic_index = job.text.find("10\n").expect("line 10 is present");
    let diagnostic_section = job
        .sections
        .iter()
        .find(|section| {
            section.byte_range.start.0 <= diagnostic_index
                && diagnostic_index < section.byte_range.end.0
        })
        .expect("line 10 has a layout section");
    assert_eq!(diagnostic_section.format.color, status::DANGER);

    let normal_section = job
        .sections
        .iter()
        .find(|section| section.byte_range.start.0 == 0)
        .expect("line 1 has a layout section");
    assert_eq!(normal_section.format.color, gray::_500);
}

#[test]
fn invalid_regex_returns_an_error_without_matches() {
    let error = find_matches("kind: ConfigMap", "[", true).expect_err("regex is invalid");

    assert!(error.starts_with("regex parse error:"));
}

#[test]
fn search_navigation_wraps_and_requests_one_scroll() {
    let mut editor = editor("name: first\nname: second");

    advance_search_match(&mut editor, 2, true);
    assert_eq!(editor.search.active_match, Some(0));
    assert_eq!(editor.search.scroll_to_match, Some(0));

    advance_search_match(&mut editor, 2, false);
    assert_eq!(editor.search.active_match, Some(1));
    assert_eq!(editor.search.scroll_to_match, Some(1));
}

#[test]
fn command_f_focuses_yaml_search_and_enter_navigates_matches() {
    let mut harness = editor_harness(editor("name: first\nname: second"));

    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::F);
    harness.step();
    harness.event(egui::Event::Text("name".into()));
    harness.step();
    harness.key_press(egui::Key::Enter);
    harness.step();

    assert_eq!(harness.state().editor.search.query, "name");
    assert_eq!(harness.state().editor.search.active_match, Some(0));

    harness.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::Enter);
    harness.step();

    assert_eq!(harness.state().editor.search.active_match, Some(1));
}
