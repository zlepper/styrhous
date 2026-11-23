use super::*;
use components::test_support::distinct_role_theme;
use std::sync::Arc;

#[test]
fn highlight_cache_recolours_when_a_new_theme_is_installed() {
    let ctx = egui::Context::default();
    let distinct = distinct_role_theme();
    components::design::theme::switch(&ctx, Arc::clone(&distinct));
    let mut cache = YamlEditorHighlightCache::default();
    let key_color = |job: &egui::text::LayoutJob| {
        job.sections
            .first()
            .expect("the key has a section")
            .format
            .color
    };
    let mut highlight = |ctx: &egui::Context| {
        let mut job = None;
        let mut output = ctx.run_ui(Default::default(), |ui| {
            job = Some(yaml_editor_layout_job(
                ui,
                &mut cache,
                "kind: ConfigMap",
                "",
                false,
                None,
            ));
        });
        output.textures_delta.clear();
        job.expect("the editor laid out the YAML")
    };

    let default_job = highlight(&ctx);
    assert_eq!(key_color(&default_job), distinct.syntax.key);

    let recoloured = Arc::new(
        Theme::from_file_str(
            r#"
            name = "Teal keys"
            [terminal]
            syntax-key = "teal-400"
            "#,
        )
        .expect("the theme file is valid"),
    );
    components::design::theme::switch(&ctx, Arc::clone(&recoloured));
    let switched_job = highlight(&ctx);

    assert_eq!(key_color(&switched_job), recoloured.syntax.key);
    assert_ne!(key_color(&switched_job), key_color(&default_job));
}

#[test]
fn search_highlights_use_match_roles_and_keep_fonts() {
    let theme = distinct_role_theme();
    let mut job = egui::text::LayoutJob::default();
    let key_format = egui::TextFormat {
        font_id: egui::FontId::monospace(13.0),
        color: theme.syntax.key,
        ..Default::default()
    };
    let value_format = egui::TextFormat {
        font_id: egui::FontId::monospace(13.0),
        color: theme.syntax.string,
        ..Default::default()
    };
    job.append("kind: ", 0.0, key_format);
    job.append("ConfigMap", 0.0, value_format.clone());

    let matches = [0..4, 6..15];
    apply_search_highlights(&mut job, &matches, matches.get(1), &theme);

    let section = |start: usize, end: usize| {
        job.sections
            .iter()
            .find(|section| section.byte_range.start.0 == start && section.byte_range.end.0 == end)
            .expect("matching section is present")
    };
    let active = section(6, 15);
    assert_eq!(active.format.font_id, value_format.font_id);
    assert_eq!(active.format.color, theme.code.match_active_text);
    assert_eq!(active.format.background, theme.code.match_active_bg);
    let inactive = section(0, 4);
    assert_eq!(inactive.format.color, theme.code.match_text);
    assert_eq!(inactive.format.background, theme.code.match_bg);
    let unmatched = section(4, 6);
    assert_eq!(unmatched.format.color, theme.syntax.key);
    assert_eq!(unmatched.format.background, egui::Color32::TRANSPARENT);
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

    let theme = distinct_role_theme();
    let ctx = egui::Context::default();
    ctx.run_ui(Default::default(), |_| {})
        .textures_delta
        .clear();
    let job = line_number_layout_job(&ctx, 12, &diagnostics, &theme);

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
    assert_eq!(diagnostic_section.format.color, theme.code.error);

    let normal_section = job
        .sections
        .iter()
        .find(|section| section.byte_range.start.0 == 0)
        .expect("line 1 has a layout section");
    assert_eq!(normal_section.format.color, theme.code.gutter);
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
    assert_eq!(
        harness.state().editor.edited_yaml,
        "name: first\nname: second",
        "Enter in the search field never reaches the YAML"
    );
}
