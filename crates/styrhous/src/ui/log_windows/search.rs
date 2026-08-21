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
        && !filter_is_active(window)
        && !matches!(window.status, PodLogStatus::Failed(_))
}

pub(super) fn show_initial_spool_state(ui: &mut egui::Ui, total_lines: usize) {
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
                ui.add(egui::Spinner::new().size(20.0).color(gray::_200));
                ui.add_space(spacing::SM);
                ui.label(
                    egui::RichText::new("Spooling logs…")
                        .font(typography::section_heading())
                        .color(gray::_200),
                );
            });
            ui.add_space(spacing::SM);
            ui.label(
                egui::RichText::new(format!(
                    "{total_lines} {} received",
                    if total_lines == 1 { "line" } else { "lines" }
                ))
                .font(typography::body())
                .color(gray::_500),
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
    if !window.pages.contains_key(&key)
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
    let invalid = window.search.error.is_some();
    let focus_search =
        ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::F));
    let search_response = ui
        .allocate_ui_with_layout(
            egui::vec2(212.0, 36.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                TailwindSearchInput::new(&mut window.search.query, &mut window.search.regex_mode)
                    .hint_text("Search logs...")
                    .id_salt(("pod-log-search", window.id))
                    .accessibility_label("Search logs")
                    .invalid(invalid)
                    .focus(focus_search)
                    .show(ui)
            },
        )
        .inner;
    if search_response.text.changed() || search_response.regex.changed() {
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

    ui.add_space(spacing::SM);
    let navigation = ui
        .allocate_ui_with_layout(
            egui::vec2(158.0, 34.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                egui::Frame::new()
                    .fill(egui::Color32::WHITE)
                    .stroke(egui::Stroke::new(1.0, gray::_300))
                    .corner_radius(radius::control())
                    .inner_margin(egui::Margin::symmetric(2, 2))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            let previous_line = search_navigation_button(
                                ui,
                                icons::arrow_up_icon(),
                                "Previous displayed line",
                            );
                            ui.separator();
                            let previous_match = search_navigation_button(
                                ui,
                                icons::arrow_left_icon(),
                                "Previous matching line",
                            );
                            ui.separator();
                            let next_match = search_navigation_button(
                                ui,
                                icons::arrow_right_icon(),
                                "Next matching line",
                            );
                            ui.separator();
                            let next_line = search_navigation_button(
                                ui,
                                icons::arrow_down_icon(),
                                "Next displayed line",
                            );
                            ui.separator();
                            let filter = filter_button(ui, filter_is_active(window));
                            (previous_line, previous_match, next_match, next_line, filter)
                        })
                        .inner
                    })
                    .inner
            },
        )
        .inner;
    ui.add_space(spacing::SM);
    let display_controls = ui
        .allocate_ui_with_layout(
            egui::vec2(96.0, 34.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                egui::Frame::new()
                    .fill(egui::Color32::WHITE)
                    .stroke(egui::Stroke::new(1.0, gray::_300))
                    .corner_radius(radius::control())
                    .inner_margin(egui::Margin::symmetric(2, 2))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            let line_numbers = log_display_toggle_button(
                                ui,
                                icons::numbered_list_icon(),
                                display_options.show_line_numbers,
                                "Show log line numbers",
                            );
                            ui.separator();
                            let timestamps = log_display_toggle_button(
                                ui,
                                icons::calendar_days_icon(),
                                display_options.show_timestamps,
                                "Show Kubernetes log timestamps",
                            );
                            ui.separator();
                            let ansi = log_display_toggle_button(
                                ui,
                                icons::swatch_icon(),
                                display_options.render_ansi,
                                "Render ANSI styling",
                            );
                            (line_numbers, timestamps, ansi)
                        })
                        .inner
                    })
                    .inner
            },
        )
        .inner;
    if navigation.1 {
        advance_log_match(window, log_store, false);
    }
    if navigation.2 {
        advance_log_match(window, log_store, true);
    }
    if navigation.0 {
        advance_log_line(window, false);
    }
    if navigation.3 {
        advance_log_line(window, true);
    }
    if navigation.4 {
        window.search.filter_matches = !window.search.filter_matches;
        window.search.active_display_row = None;
        window.search.scroll_to_display_row = None;
        window.clear_pages();
    }
    if display_controls.0 {
        display_options.show_line_numbers = !display_options.show_line_numbers;
    }
    if display_controls.1 {
        display_options.show_timestamps = !display_options.show_timestamps;
    }
    if display_controls.2 {
        display_options.render_ansi = !display_options.render_ansi;
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
    let pattern = if window.search.regex_mode {
        window.search.query.clone()
    } else {
        regex::escape(&window.search.query)
    };
    if let Err(error) = regex::RegexBuilder::new(&pattern)
        .case_insensitive(true)
        .build()
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

pub(super) fn filter_button(ui: &mut egui::Ui, active: bool) -> bool {
    log_display_toggle_button(ui, icons::funnel_icon(), active, "Filter to matching lines")
}

pub(super) fn log_display_toggle_button(
    ui: &mut egui::Ui,
    icon: egui::Image<'static>,
    active: bool,
    label: &str,
) -> bool {
    let response = ui
        .add_sized(
            egui::Vec2::splat(28.0),
            egui::Button::image(
                icon.fit_to_exact_size(egui::Vec2::splat(14.0))
                    .tint(gray::_700),
            )
            .frame(false),
        )
        .with_pointing_hand();
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Checkbox, ui.is_enabled(), label)
    });
    if active {
        let center = response.rect.right_bottom() - egui::vec2(3.5, 3.5);
        ui.painter()
            .circle_filled(center, 5.5, components::colors::SUCCESS);
        ui.painter().text(
            center,
            egui::Align2::CENTER_CENTER,
            "✓",
            egui::FontId::proportional(8.0),
            egui::Color32::WHITE,
        );
    }
    response.on_hover_text(label).clicked()
}

pub(super) fn filter_is_active(window: &PodLogWindowState) -> bool {
    window.search.filter_matches && !window.search.query.is_empty()
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
