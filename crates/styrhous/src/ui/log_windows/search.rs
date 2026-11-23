use super::*;

pub(super) fn display_row_is_visible(
    display_row: usize,
    row_step: f32,
    output: &egui::scroll_area::ScrollAreaOutput<()>,
) -> bool {
    let row_top = display_row as f32 * row_step;
    let row_bottom = row_top + row_step;
    row_bottom > output.state.offset.y
        && row_top < output.state.offset.y + output.inner_rect.height()
}

pub(super) fn initial_spool_is_pending(window: &PodLogWindowState) -> bool {
    window.total_lines > 0
        && !window.initial_page_loaded
        && window.live_rows.is_empty()
        && !filter_is_active(window)
        && !matches!(window.status, PodLogStatus::Failed(_))
}

pub(super) fn show_initial_spool_state(ui: &mut egui::Ui, total_lines: usize) {
    let theme = ui.tokens();
    let state_rect = egui::Rect::from_center_size(
        ui.available_rect_before_wrap().center(),
        egui::vec2(280.0, 64.0),
    );
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(state_rect)
            .layout(egui::Layout::top_down(egui::Align::Center)),
        |ui| {
            ui.horizontal_centered(|ui| {
                components::Spinner::new()
                    .size(components::design::size::SPINNER_LG)
                    .neutral(theme.code.text_muted)
                    .show(ui);
                ui.add_space(space::S8);
                ui.label(
                    egui::RichText::new("Spooling logs…")
                        .font(type_scale::TITLE.font_id(ui.ctx()))
                        .color(theme.code.text_muted),
                );
            });
            ui.add_space(space::S8);
            ui.label(
                egui::RichText::new(format!(
                    "{total_lines} {} received",
                    if total_lines == 1 { "line" } else { "lines" }
                ))
                .font(type_scale::BODY.font_id(ui.ctx()))
                .color(theme.text.muted),
            );
        },
    );
}

pub(super) fn request_page_for_display_row(
    window: &mut PodLogWindowState,
    log_store: &LogStoreService,
    display_row: usize,
) {
    let page_start = display_row / window.page_size * window.page_size;
    let key = LogPageKey {
        generation: window.search.generation,
        filter_matches: filter_is_active(window),
        page_start,
    };
    if (!window.pages.contains_key(&key) || window.pages_needing_refresh.contains(&key))
        && window.pending_pages.insert(key)
        && !log_store.load_page(
            window.id,
            key.generation,
            key.filter_matches,
            key.page_start,
        )
    {
        window.pending_pages.remove(&key);
    }
}

pub(super) fn show_log_search_controls(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    window: &mut PodLogWindowState,
    display_options: &mut LogDisplayOptions,
    log_store: &LogStoreService,
) {
    let focus_search =
        ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::F));
    let status = log_search_status(&window.search);
    let filter_active = filter_is_active(window);
    let following = window.tail.is_following();
    let show_source_labels = window.show_source_labels;
    let window_id = window.id;
    let search = &mut window.search;
    // Search → filter | view options … navigation → Follow (C14 toolbar order).
    let (
        (search_response, filter, [line_numbers, timestamps, ansi, source_labels]),
        ([previous_line, previous_match, next_match, next_line], tail_lock),
    ) = Toolbar::new(("pod-log", window_id)).show(
        ui,
        |ui| {
            // Match navigation stays on the toolbar's line navigation group.
            let search_response = SearchField::new(&mut search.query)
                .regex(&mut search.regex_mode)
                .placeholder("Search logs…")
                .id_salt(("pod-log-search", window_id))
                .accessibility_label("Search logs")
                .status(status)
                .navigation(false)
                .request_focus(focus_search)
                .show(ui);
            let filter = filter_button(ui, filter_active);
            toolbar_divider(ui);
            let view_options = ButtonGroup::new("View options")
                .toggle(ToggleButton::icon(
                    icons::Icon::NumberedList,
                    "Show log line numbers",
                    display_options.show_line_numbers,
                ))
                .toggle(ToggleButton::icon(
                    icons::Icon::Clock,
                    "Show Kubernetes log timestamps",
                    display_options.show_timestamps,
                ))
                .toggle(ToggleButton::icon(
                    icons::Icon::Swatch,
                    "Render ANSI styling",
                    display_options.render_ansi,
                ))
                .toggle(ToggleButton::icon(
                    icons::Icon::Tag,
                    "Show log source labels",
                    show_source_labels,
                ))
                .show(ui);
            (search_response, filter, clicked::<4>(&view_options))
        },
        |ui| {
            let navigation = ButtonGroup::new("Log navigation")
                .toggle(ToggleButton::action(
                    icons::Icon::ArrowUp,
                    "Previous displayed line",
                ))
                .toggle(ToggleButton::action(
                    icons::Icon::ArrowLeft,
                    "Previous matching line",
                ))
                .toggle(ToggleButton::action(
                    icons::Icon::ArrowRight,
                    "Next matching line",
                ))
                .toggle(ToggleButton::action(
                    icons::Icon::ArrowDown,
                    "Next displayed line",
                ))
                .show(ui);
            let tail_lock = ToggleButton::text("Follow", following)
                .leading_icon(icons::Icon::ChevronDoubleDown)
                .accessibility_label("Lock to bottom")
                .tooltip("Lock to bottom")
                .show(ui)
                .clicked();
            (clicked::<4>(&navigation), tail_lock)
        },
    );
    if search_response.changed {
        window.search.generation += 1;
        window.search.match_count = 0;
        window.search.scanned_lines = 0;
        window.search.search_complete = window.search.query.is_empty();
        window.search.error = None;
        window.search.active_display_row = None;
        window.search.active_match = None;
        window.search.scroll_to_display_row = None;
        window.clear_pages();
        window.search.search_deadline = Some(Instant::now() + Duration::from_millis(150));
        ctx.request_repaint_after(Duration::from_millis(150));
    }
    let mut navigated = false;
    // Shift+Enter and Enter in the search field move like the match buttons.
    if previous_match || search_response.previous {
        navigated |= advance_log_match(window, log_store, false);
    }
    if next_match || search_response.next {
        navigated |= advance_log_match(window, log_store, true);
    }
    if previous_line {
        navigated |= advance_log_line(window, false);
    }
    if next_line {
        navigated |= advance_log_line(window, true);
    }
    if navigated && window.tail.release() {
        // The active indicator was rendered before the navigation click was
        // handled. Keep it truthful even when the requested row was already
        // visible and no physical scroll was needed.
        ctx.request_repaint();
    }
    if filter {
        window.search.filter_matches = !window.search.filter_matches;
        window.search.active_display_row = None;
        window.search.scroll_to_display_row = None;
        window.clear_pages();
    }
    if line_numbers {
        display_options.show_line_numbers = !display_options.show_line_numbers;
    }
    if timestamps {
        display_options.show_timestamps = !display_options.show_timestamps;
    }
    if ansi {
        display_options.render_ansi = !display_options.render_ansi;
    }
    if source_labels {
        window.show_source_labels = !window.show_source_labels;
    }
    if tail_lock {
        if window.tail.toggle() {
            window.search.scroll_to_display_row = None;
            window.search.rebase_scroll_row_delta = None;
        }
        // The active indicator was rendered before the click was handled.
        // Repaint immediately so it reflects the new state even if no more
        // log records arrive this frame.
        ctx.request_repaint();
    }
    sync_search(ctx, window, log_store);
}

pub(super) fn sync_search(
    ctx: &egui::Context,
    window: &mut PodLogWindowState,
    log_store: &LogStoreService,
) {
    let Some(deadline) = window.search.search_deadline else {
        return;
    };
    if Instant::now() < deadline {
        ctx.request_repaint_after(deadline.saturating_duration_since(Instant::now()));
        return;
    }
    window.search.search_deadline = None;
    if window.search.query.is_empty() {
        window.search.search_complete = true;
        return;
    }
    if let Err(error) =
        crate::log_store::log_matcher(&window.search.query, window.search.regex_mode)
    {
        window.search.error = Some(error.to_string());
        window.search.search_complete = true;
        return;
    }
    window.search.search_complete = false;
    if !log_store.search(
        window.id,
        window.search.generation,
        window.search.query.clone(),
        window.search.regex_mode,
    ) {
        window.search.search_deadline = Some(Instant::now() + Duration::from_millis(100));
        ctx.request_repaint_after(Duration::from_millis(100));
    }
}

/// The standalone "Filter to matching lines" toggle; true when clicked.
pub(super) fn filter_button(ui: &mut egui::Ui, active: bool) -> bool {
    ToggleButton::text("Matches only", active)
        .leading_icon(icons::Icon::Funnel)
        .accessibility_label("Filter to matching lines")
        .tooltip("Filter to matching lines")
        .show(ui)
        .clicked()
}

/// Whether each of the first `N` responses of a button group was clicked.
fn clicked<const N: usize>(responses: &[egui::Response]) -> [bool; N] {
    std::array::from_fn(|index| responses.get(index).is_some_and(egui::Response::clicked))
}

pub(super) fn filter_is_active(window: &PodLogWindowState) -> bool {
    window.filter_is_active()
}

pub(super) fn displayed_line_count(window: &PodLogWindowState) -> usize {
    if filter_is_active(window) {
        window.search.match_count
    } else {
        window.total_lines
    }
}

pub(super) fn request_copy(
    ctx: &egui::Context,
    window: &PodLogWindowState,
    log_store: &LogStoreService,
) {
    let copy_requested = ctx.input(|input| {
        input
            .events
            .iter()
            .any(|event| matches!(event, egui::Event::Copy))
    });
    let Some(selection) = copy_requested.then_some(window.selection).flatten() else {
        return;
    };
    let (start, end) = selection.normalized();
    if start == end {
        return;
    }
    let _ = log_store.copy(
        window.id,
        window.selection_generation,
        window.search.generation,
        filter_is_active(window),
        start.display_row,
        start.byte_offset,
        end.display_row,
        end.byte_offset,
    );
}

/// What the log search field shows for `search`.
fn log_search_status(search: &crate::ui::log_state::LogSearchState) -> SearchStatus {
    if let Some(error) = &search.error {
        SearchStatus::Invalid(error.clone())
    } else if search.query.is_empty() {
        SearchStatus::Idle
    } else if !search.search_complete {
        SearchStatus::Searching {
            found: search.match_count,
        }
    } else {
        SearchStatus::Matches {
            active: search.active_match,
            total: search.match_count,
        }
    }
}

#[cfg(test)]
mod status_tests {
    use super::*;
    use crate::ui::log_state::LogSearchState;

    #[test]
    fn the_search_field_status_follows_the_log_search() {
        let search = |change: fn(&mut LogSearchState)| {
            let mut search = LogSearchState {
                query: "http".to_owned(),
                ..LogSearchState::default()
            };
            change(&mut search);
            log_search_status(&search)
        };
        assert_eq!(search(|search| search.query.clear()), SearchStatus::Idle);
        assert_eq!(
            search(|search| search.error = Some("unclosed group".to_owned())),
            SearchStatus::Invalid("unclosed group".to_owned())
        );
        assert_eq!(
            search(|search| {
                search.search_complete = false;
                search.match_count = 4;
            }),
            SearchStatus::Searching { found: 4 }
        );
        assert_eq!(
            search(|search| {
                search.match_count = 12;
                search.active_match = Some(2);
            }),
            SearchStatus::Matches {
                active: Some(2),
                total: 12
            }
        );
    }
}
