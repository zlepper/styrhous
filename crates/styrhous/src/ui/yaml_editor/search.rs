use super::*;

pub(super) fn toolbar_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(TOOLBAR_BACKGROUND)
        .stroke(egui::Stroke::new(1.0, TABLE_BORDER))
        .inner_margin(egui::Margin::symmetric(
            spacing::XL as i8,
            spacing::SM as i8,
        ))
}

pub(super) fn show_search_controls(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    editor: &mut YamlEditorWindowState,
    search_matches: &Result<Vec<Range<usize>>, String>,
) {
    let focus_search =
        ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::F));
    let search_input_salt = egui::Id::new(("yaml-editor-search", editor.id));
    // Consume Enter before the single-line TextEdit sees it. This keeps the
    // shortcut from adding a newline to YAML or being swallowed by the input.
    let keyboard_navigation = editor.search.input_focused.then(|| {
        ctx.input_mut(|input| {
            (
                input.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
                input.consume_key(egui::Modifiers::SHIFT, egui::Key::Enter),
            )
        })
    });
    let invalid = search_matches.is_err();
    let response = ui
        .allocate_ui_with_layout(
            egui::vec2(SEARCH_CONTROL_WIDTH, 36.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                TailwindSearchInput::new(&mut editor.search.query, &mut editor.search.regex_mode)
                    .hint_text("Search...")
                    .id_salt(search_input_salt)
                    .accessibility_label("Search YAML")
                    .invalid(invalid)
                    .focus(focus_search)
                    .show(ui)
            },
        )
        .inner;
    if response.text.changed() || response.regex.changed() {
        clear_active_match(editor);
    }
    editor.search.input_focused = response.text.has_focus();

    let matches = search_matches.as_ref().ok();
    let match_count = matches.map_or(0, Vec::len);

    ui.add_space(spacing::SM);
    ui.label(
        egui::RichText::new(match_count_label(editor, match_count))
            .font(typography::metadata())
            .color(if invalid { status::DANGER } else { gray::_600 }),
    );
    ui.add_space(spacing::XS);
    let (previous_clicked, next_clicked) = ui
        .allocate_ui_with_layout(
            egui::vec2(
                2.0 * SEARCH_NAVIGATION_BUTTON_SIZE,
                SEARCH_NAVIGATION_BUTTON_SIZE,
            ),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.add_enabled_ui(match_count > 0, |ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    let previous =
                        search_navigation_button(ui, icons::arrow_left_icon(), "Previous match");
                    let next =
                        search_navigation_button(ui, icons::arrow_right_icon(), "Next match");
                    (previous, next)
                })
                .inner
            },
        )
        .inner;

    let previous_requested =
        previous_clicked || keyboard_navigation.is_some_and(|(_, previous)| previous);
    let next_requested = next_clicked || keyboard_navigation.is_some_and(|(next, _)| next);
    if previous_requested {
        advance_search_match(editor, match_count, false);
    }
    if next_requested {
        advance_search_match(editor, match_count, true);
    }
}

pub(super) fn match_count_label(editor: &YamlEditorWindowState, match_count: usize) -> String {
    if editor.search.query.is_empty() {
        return "0 matches".to_owned();
    }
    match editor.search.active_match {
        Some(active_match) => format!("{} of {match_count}", active_match + 1),
        None => format!("0 of {match_count}"),
    }
}

pub(super) fn clear_active_match(editor: &mut YamlEditorWindowState) {
    editor.search.active_match = None;
    editor.search.scroll_to_match = None;
}

pub(super) fn advance_search_match(
    editor: &mut YamlEditorWindowState,
    match_count: usize,
    forward: bool,
) {
    if match_count == 0 {
        return;
    }
    let next = match (editor.search.active_match, forward) {
        (Some(current), true) => (current + 1) % match_count,
        (Some(current), false) => (current + match_count - 1) % match_count,
        (None, true) => 0,
        (None, false) => match_count - 1,
    };
    editor.search.active_match = Some(next);
    editor.search.scroll_to_match = Some(next);
}

pub(super) fn reconcile_search_state(
    editor: &mut YamlEditorWindowState,
    search_matches: Option<&Vec<Range<usize>>>,
) {
    let Some(search_matches) = search_matches else {
        clear_active_match(editor);
        return;
    };
    if editor
        .search
        .active_match
        .is_some_and(|index| index >= search_matches.len())
    {
        clear_active_match(editor);
    }
}

pub(super) fn yaml_search_matches(
    editor: &YamlEditorWindowState,
) -> Result<Vec<Range<usize>>, String> {
    find_matches(
        &editor.edited_yaml,
        &editor.search.query,
        editor.search.regex_mode,
    )
}

pub(super) fn find_matches(
    text: &str,
    query: &str,
    regex_mode: bool,
) -> Result<Vec<Range<usize>>, String> {
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let pattern = if regex_mode {
        query.to_owned()
    } else {
        regex::escape(query)
    };
    let matcher = regex::RegexBuilder::new(&pattern)
        .case_insensitive(true)
        .build()
        .map_err(|error| error.to_string())?;
    Ok(matcher
        .find_iter(text)
        .filter(|matched| !matched.is_empty())
        .map(|matched| matched.start()..matched.end())
        .collect())
}

pub(super) fn resource_scope(editor: &YamlEditorWindowState) -> String {
    editor.namespace.as_deref().map_or_else(
        || format!("{} · Cluster-wide", editor.api_resource.kind),
        |namespace| format!("{} · {namespace}", editor.api_resource.kind),
    )
}

pub(super) fn status_indicator(ui: &mut egui::Ui, color: egui::Color32, label: &str) {
    ui.label(
        egui::RichText::new("●")
            .font(typography::body())
            .color(color),
    );
    ui.label(
        egui::RichText::new(label)
            .font(typography::body())
            .color(gray::_600),
    );
}

pub(super) fn error_strip(ui: &mut egui::Ui, error: &str) {
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(69, 10, 10))
        .stroke(egui::Stroke::new(1.0, status::DANGER))
        .inner_margin(egui::Margin::symmetric(
            spacing::LG as i8,
            spacing::SM as i8,
        ))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(
                egui::RichText::new(error)
                    .font(typography::body())
                    .color(egui::Color32::from_rgb(254, 202, 202)),
            );
        });
    ui.add_space(spacing::SM);
}

pub(super) fn editor_error(ui: &mut egui::Ui, error: &str) {
    ui.centered_and_justified(|ui| {
        ui.label(
            egui::RichText::new(error)
                .font(typography::body())
                .color(egui::Color32::from_rgb(254, 202, 202)),
        );
    });
}
