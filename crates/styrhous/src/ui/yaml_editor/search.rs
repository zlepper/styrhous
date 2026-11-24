use super::*;

pub(super) fn show_search_controls(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    editor: &mut YamlEditorWindowState,
    search_matches: &Result<Vec<Range<usize>>, String>,
) {
    let focus_search =
        ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::F));
    let status = match search_matches {
        Err(error) => SearchStatus::Invalid(error.clone()),
        Ok(_) if editor.search.query.is_empty() => SearchStatus::Idle,
        Ok(matches) => SearchStatus::Matches {
            active: editor.search.active_match,
            total: matches.len(),
        },
    };
    let match_count = search_matches.as_ref().map_or(0, Vec::len);
    let response = SearchField::new(&mut editor.search.query)
        .regex(&mut editor.search.regex_mode)
        .placeholder("Find in document…")
        .id_salt(("yaml-editor-search", editor.id))
        .accessibility_label("Search YAML")
        .status(status)
        .request_focus(focus_search)
        .show(ui);
    if response.changed {
        clear_active_match(editor);
    }
    if response.previous {
        advance_search_match(editor, match_count, false);
    }
    if response.next {
        advance_search_match(editor, match_count, true);
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

/// An error above the code, inset from the window like the code surface's own content.
pub(super) fn error_strip(ui: &mut egui::Ui, error: &str) {
    egui::Frame::new()
        .inner_margin(egui::Margin {
            left: space::S16 as i8,
            right: space::S16 as i8,
            top: space::S16 as i8,
            bottom: space::S8 as i8,
        })
        .show(ui, |ui| {
            components::Callout::new(Tone::Danger, error)
                .alert()
                .show(ui);
        });
}

pub(super) fn editor_error(ui: &mut egui::Ui, error: &str) {
    let color = ui.tokens().danger.text;
    ui.centered_and_justified(|ui| {
        ui.label(
            egui::RichText::new(error)
                .font(type_scale::BODY.font_id(ui.ctx()))
                .color(color),
        );
    });
}
