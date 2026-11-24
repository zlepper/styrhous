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
        && !window.filter_is_active()
        && !matches!(window.status, PodLogStatus::Failed(_))
}

pub(super) fn request_page_for_display_row(
    window: &mut PodLogWindowState,
    log_store: &LogStoreService,
    display_row: usize,
) {
    let page_start = display_row / window.page_size * window.page_size;
    let key = LogPageKey {
        generation: window.search.generation,
        filter_matches: window.filter_is_active(),
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

/// Seconds a typed query settles before the search starts.
const SEARCH_DEBOUNCE: f64 = 0.15;
/// Seconds before a search the store was too busy to take is offered again.
const SEARCH_RETRY: f64 = 0.1;

/// What the toolbar asked for this frame.
#[derive(Default)]
struct ToolbarActions {
    search: Option<components::SearchFieldResponse>,
    filter: bool,
    line_numbers: bool,
    timestamps: bool,
    ansi: bool,
    source_labels: bool,
    follow: bool,
}

/// The floating toolbar: search, the view toggles and Follow. In a narrow
/// window the toggles fold into a "View options" menu.
pub(super) fn show_log_toolbar(
    ctx: &egui::Context,
    window: &mut PodLogWindowState,
    display_options: &mut LogDisplayOptions,
    log_store: &LogStoreService,
) {
    let focus_search =
        ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::F));
    let narrow = ctx.content_rect().width() < NARROW_WINDOW_WIDTH;
    let status = log_search_status(&window.search);
    let query = window.search.query.clone();
    let toggles = ViewToggles {
        filter: window.filter_is_active(),
        line_numbers: display_options.show_line_numbers,
        timestamps: display_options.show_timestamps,
        ansi: display_options.render_ansi,
        source_labels: window.show_source_labels,
        multiple_sources: window.has_multiple_sources(),
    };
    let following = window.tail.is_following();
    let unavailable = log_is_unavailable(window);
    let window_id = window.id;
    let search = &mut window.search;
    let (width, layout) = if narrow {
        (
            size::LOG_TOOLBAR_NARROW,
            egui::Layout::left_to_right(egui::Align::Center),
        )
    } else if toggles.multiple_sources {
        (
            size::LOG_TOOLBAR_INTERLEAVED,
            egui::Layout::right_to_left(egui::Align::Center),
        )
    } else {
        (
            size::LOG_TOOLBAR,
            egui::Layout::right_to_left(egui::Align::Center),
        )
    };
    let actions = FloatingToolbar::show_floating_sized(ctx, window_id, width, layout, |ui| {
        let mut actions = ToolbarActions::default();
        let mut search_field = |ui: &mut egui::Ui, width: f32, actions: &mut ToolbarActions| {
            ui.add_enabled_ui(!unavailable, |ui| {
                actions.search = Some(
                    SearchField::pill(&mut search.query)
                        .navigation(true)
                        .height(size::CONTROL_LG)
                        .width(width)
                        .regex(&mut search.regex_mode)
                        .placeholder("Search logs…")
                        .id_salt(("pod-log-search", window_id))
                        .accessibility_label("Search logs")
                        .status(status.clone())
                        .request_focus(focus_search)
                        .show(ui),
                );
            });
        };
        let follow = |ui: &mut egui::Ui| {
            ToggleButton::text("Follow", following)
                .leading_small_icon(icons::Icon::ChevronDoubleDown)
                .accessibility_label("Lock to bottom")
                .tooltip("Lock to bottom")
                .enabled(!unavailable)
                .show(ui)
                .clicked()
        };
        if narrow {
            search_field(ui, size::LOG_SEARCH_NARROW, &mut actions);
            view_options_menu(ui, &toggles, unavailable, &mut actions);
            actions.follow = follow(ui);
        } else {
            // Laid out from the end, so the search field takes the rest.
            actions.follow = follow(ui);
            view_toggles(ui, &toggles, unavailable, &mut actions);
            let room = ui.available_width() - ui.spacing().item_spacing.x;
            search_field(ui, search_width(ui, room, &query, &status), &mut actions);
        }
        actions
    });
    apply_toolbar_actions(ctx, window, display_options, log_store, actions);
}

/// The width of the search field: the room the toggles leave it, or more when
/// the query and the count would not fit (up to its 300 point design width).
fn search_width(ui: &egui::Ui, room: f32, query: &str, status: &SearchStatus) -> f32 {
    let measure = |role: components::design::type_scale::TextRole, text: &str| {
        role.galley(ui.ctx(), text, egui::Color32::PLACEHOLDER, f32::INFINITY)
            .rect
            .width()
    };
    let count = status
        .count_text()
        .map_or(0.0, |count| measure(type_scale::MONO_COUNT, &count));
    let needed = size::LOG_SEARCH_CHROME + count + measure(type_scale::UI, query);
    room.max(needed.min(300.0))
}

/// The view toggles' state.
struct ViewToggles {
    filter: bool,
    line_numbers: bool,
    timestamps: bool,
    ansi: bool,
    source_labels: bool,
    multiple_sources: bool,
}

/// The view toggles side by side, added from the end of a right-to-left
/// toolbar: Colours (Source in an interleaved window), Time, Line no. and
/// Only matches read left to right.
fn view_toggles(
    ui: &mut egui::Ui,
    toggles: &ViewToggles,
    unavailable: bool,
    actions: &mut ToolbarActions,
) {
    if toggles.multiple_sources {
        actions.source_labels = ToggleButton::text("Source", toggles.source_labels)
            .accessibility_label("Show log source labels")
            .tooltip("Show log source labels")
            .show(ui)
            .clicked();
    } else {
        actions.ansi = ToggleButton::text("Colours", toggles.ansi)
            .accessibility_label("Render ANSI styling")
            .tooltip("Render ANSI styling")
            .show(ui)
            .clicked();
    }
    actions.timestamps = ToggleButton::text("Time", toggles.timestamps)
        .accessibility_label("Show Kubernetes log timestamps")
        .tooltip("Show Kubernetes log timestamps")
        .show(ui)
        .clicked();
    actions.line_numbers = ToggleButton::text("Line no.", toggles.line_numbers)
        .accessibility_label("Show log line numbers")
        .tooltip("Show log line numbers")
        .show(ui)
        .clicked();
    actions.filter = ToggleButton::text("Only matches", toggles.filter)
        .accessibility_label("Filter to matching lines")
        .tooltip("Filter to matching lines")
        .enabled(!unavailable)
        .show(ui)
        .clicked();
}

/// The view toggles folded into one "View options" menu, for a narrow
/// window.
fn view_options_menu(
    ui: &mut egui::Ui,
    toggles: &ViewToggles,
    unavailable: bool,
    actions: &mut ToolbarActions,
) {
    let trigger_id = ui.next_auto_id();
    let open = egui::Popup::is_id_open(ui.ctx(), trigger_id.with("popup"));
    let trigger = ToggleButton::text("View options", open)
        .leading_small_icon(icons::Icon::AdjustmentsHorizontal)
        .trailing_icon(icons::Icon::ChevronDown)
        .show(ui);
    let state = |on| {
        if on {
            components::CheckState::On
        } else {
            components::CheckState::Off
        }
    };
    components::MoreButton::show_menu_below(&trigger, "View options", |menu| {
        if !unavailable
            && menu
                .checkbox("Only matches", state(toggles.filter))
                .clicked()
        {
            actions.filter = true;
        }
        if menu
            .checkbox("Line numbers", state(toggles.line_numbers))
            .clicked()
        {
            actions.line_numbers = true;
        }
        if menu.checkbox("Time", state(toggles.timestamps)).clicked() {
            actions.timestamps = true;
        }
        if toggles.multiple_sources {
            if menu
                .checkbox("Source", state(toggles.source_labels))
                .clicked()
            {
                actions.source_labels = true;
            }
        } else if menu.checkbox("Colours", state(toggles.ansi)).clicked() {
            actions.ansi = true;
        }
    });
}

fn apply_toolbar_actions(
    ctx: &egui::Context,
    window: &mut PodLogWindowState,
    display_options: &mut LogDisplayOptions,
    log_store: &LogStoreService,
    actions: ToolbarActions,
) {
    let (changed, previous, next) = actions.search.map_or((false, false, false), |response| {
        (response.changed, response.previous, response.next)
    });
    if changed {
        let now = ctx.input(|input| input.time);
        window.search.generation += 1;
        window.search.match_count = 0;
        window.search.scanned_lines = 0;
        window.search.search_complete = window.search.query.is_empty();
        window.search.error = None;
        window.search.active_display_row = None;
        window.search.active_match = None;
        window.search.scroll_to_display_row = None;
        window.clear_pages();
        window.search.search_deadline = Some(now + SEARCH_DEBOUNCE);
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(SEARCH_DEBOUNCE));
    }
    let mut navigated = false;
    // Shift+Enter and Enter in the search field move like the match buttons.
    if previous {
        navigated |= advance_log_match(window, log_store, false);
    }
    if next {
        navigated |= advance_log_match(window, log_store, true);
    }
    if navigated && window.tail.release() {
        // The active indicator was rendered before the navigation click was
        // handled. Keep it truthful even when the requested row was already
        // visible and no physical scroll was needed.
        ctx.request_repaint();
    }
    if actions.filter {
        window.search.filter_matches = !window.search.filter_matches;
        window.search.active_display_row = None;
        window.search.scroll_to_display_row = None;
        window.clear_pages();
    }
    if actions.line_numbers {
        display_options.show_line_numbers = !display_options.show_line_numbers;
    }
    if actions.timestamps {
        display_options.show_timestamps = !display_options.show_timestamps;
    }
    if actions.ansi {
        display_options.render_ansi = !display_options.render_ansi;
    }
    if actions.source_labels {
        window.show_source_labels = !window.show_source_labels;
    }
    if actions.follow {
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
    let now = ctx.input(|input| input.time);
    if now < deadline {
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(deadline - now));
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
        window.search.search_deadline = Some(now + SEARCH_RETRY);
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(SEARCH_RETRY));
    }
}

pub(super) fn displayed_line_count(window: &PodLogWindowState) -> usize {
    if window.filter_is_active() {
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
        window.filter_is_active(),
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
