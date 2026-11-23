use super::*;

impl Combobox<NoFilter> {
    /// A combobox with an explicit ID.
    pub fn new(id_salt: impl std::hash::Hash + std::fmt::Debug) -> Self {
        Combobox {
            id_salt: Id::new(id_salt),
            label: None,
            accessibility_label: None,
            placeholder: None,
            search_placeholder: None,
            search_accessibility_label: None,
            selected_text: None,
            lead: None,
            more: 0,
            width: None,
            size: FieldSize::Md,
            select_all: None,
            item_height: size::OPTION,
            searchable: true,
            multi_select: false,
            noun: "options".to_owned(),
            enabled: true,
            disabled_reason: None,
            forced_state: None,
            preview: None,
            read_only: false,
            filter: NoFilter,
        }
    }

    /// A combobox labelled `label` above it, which is also its ID source.
    pub fn from_label(label: impl Into<WidgetText>) -> Self {
        let label = label.into();
        Combobox {
            id_salt: Id::new(label.text()),
            label: Some(label),
            ..Self::new(())
        }
    }

    /// Configure the text used for fuzzy filtering.
    pub fn filter_by<T, F>(self, filter_fn: F) -> Combobox<WithFilter<F>>
    where
        F: Fn(&T) -> &str,
    {
        Combobox {
            id_salt: self.id_salt,
            label: self.label,
            accessibility_label: self.accessibility_label,
            placeholder: self.placeholder,
            search_placeholder: self.search_placeholder,
            search_accessibility_label: self.search_accessibility_label,
            selected_text: self.selected_text,
            lead: self.lead,
            more: self.more,
            width: self.width,
            size: self.size,
            select_all: self.select_all,
            item_height: self.item_height,
            searchable: self.searchable,
            multi_select: self.multi_select,
            noun: self.noun,
            enabled: self.enabled,
            disabled_reason: self.disabled_reason,
            forced_state: self.forced_state,
            preview: self.preview,
            read_only: self.read_only,
            filter: WithFilter(filter_fn),
        }
    }
}

impl<Filter> Combobox<Filter> {
    /// Set the accessible name for the combobox and its options popup.
    pub fn accessibility_label(mut self, label: impl Into<String>) -> Self {
        self.accessibility_label = Some(label.into());
        self
    }

    /// The `text-disabled` text the trigger shows while nothing is selected,
    /// and the search row's placeholder unless [`Self::search_placeholder`]
    /// sets one.
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    /// The search row's placeholder ("Search namespaces…").
    pub fn search_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.search_placeholder = Some(placeholder.into());
        self
    }

    /// Sets the accessible name for the popup's filter input.
    ///
    /// Use this with [`Self::new`] when the combobox label is rendered by the
    /// caller rather than by the component itself.
    pub fn search_accessibility_label(mut self, label: impl Into<String>) -> Self {
        self.search_accessibility_label = Some(label.into());
        self
    }

    /// The value the trigger shows: the selection, or its first values.
    /// Truncated with an ellipsis; the full value is the tooltip then.
    pub fn selected_text(mut self, text: impl Into<String>) -> Self {
        self.selected_text = Some(text.into());
        self
    }

    /// Lead the trigger's value with a status lamp: `Some(true)` lit,
    /// `Some(false)` neutral, `None` none (no grey lamp when the status is
    /// unknown).
    pub fn selected_status(mut self, status: Option<bool>) -> Self {
        self.lead = status.map(Lead::Status).or(match self.lead {
            Some(icon @ Lead::Icon { .. }) => Some(icon),
            _ => None,
        });
        self
    }

    /// Lead the trigger's value with a `text-muted` icon (the globe of
    /// "All namespaces").
    pub fn selected_icon(mut self, icon: Icon) -> Self {
        self.lead = Some(Lead::Icon { icon, muted: true });
        self
    }

    // C16
    /// Lead the trigger's value with an icon in the value's colour (the
    /// globe of the scope pill).
    pub fn selected_value_icon(mut self, icon: Icon) -> Self {
        self.lead = Some(Lead::Icon { icon, muted: false });
        self
    }

    /// Show a `+N` chip after the trigger's value for `count` more selected
    /// values (none for 0).
    pub fn selected_more(mut self, count: usize) -> Self {
        self.more = count;
        self
    }

    /// The width of the trigger and its popover.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width.max(0.0));
        self
    }

    /// The trigger's size: 32 points by default, 28 with 12 px text for
    /// dense toolbars and table rows.
    pub fn size(mut self, size: FieldSize) -> Self {
        self.size = size;
        self
    }

    /// Show a select-all row labelled `label` ("All namespaces") in
    /// `state` above the options while the search is empty. Its activation
    /// is reported by [`ComboboxResponse::select_all_clicked`].
    pub fn select_all(mut self, label: impl Into<String>, state: CheckState) -> Self {
        self.select_all = Some(SelectAll {
            label: label.into(),
            state,
        });
        self
    }

    /// Use two-line rows (44 points) for options with a detail line.
    pub fn multiline_items(mut self) -> Self {
        self.item_height = size::OPTION_TWO_LINE;
        self
    }

    /// Show the search row (on by default). A plain list suits short fixed
    /// choices.
    pub fn searchable(mut self, searchable: bool) -> Self {
        self.searchable = searchable;
        self
    }

    /// Lead every option with a checkbox and show the key-hint footer. A
    /// click on a checkbox, Ctrl-click and Space toggle an option; a click
    /// elsewhere on a row and Enter replace the selection.
    pub fn multi_select(mut self) -> Self {
        self.multi_select = true;
        self
    }

    /// What the options are, for the empty state: "No {noun} match “q”".
    pub fn noun(mut self, noun: impl Into<String>) -> Self {
        self.noun = noun.into();
        self
    }

    /// Enable or disable the trigger.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// The tooltip of the disabled trigger, saying why ("No cluster").
    pub fn disabled_reason(mut self, reason: impl Into<String>) -> Self {
        self.disabled_reason = Some(reason.into());
        self
    }

    /// Paint the trigger in `state` whatever the pointer and keyboard do
    /// (design boards and snapshots).
    pub fn force_state(mut self, state: ControlState) -> Self {
        self.forced_state = Some(state);
        self
    }

    // C16
    /// Show the value without a popover: the trigger keeps its enabled look
    /// but cannot be opened (the scope of a cluster-scoped kind).
    pub fn read_only(mut self) -> Self {
        self.read_only = true;
        self
    }

    /// Show the popover open, `gap` points under the trigger in the layout
    /// (not as a popup), with `query` in its search row and the `highlight`
    /// row highlighted, for design boards and snapshots.
    #[doc(hidden)]
    pub fn preview_open(
        mut self,
        query: impl Into<String>,
        highlight: Option<usize>,
        gap: f32,
    ) -> Self {
        self.preview = Some(Preview {
            query: query.into(),
            highlight,
            gap,
            trigger: true,
        });
        self
    }

    /// With [`Self::preview_open`], show only the popover.
    #[doc(hidden)]
    pub fn preview_without_trigger(mut self) -> Self {
        if let Some(preview) = &mut self.preview {
            preview.trigger = false;
            preview.gap = 0.0;
        }
        self
    }
}

impl<F> Combobox<WithFilter<F>> {
    /// Show the combobox and call `render` for every filtered option.
    pub fn show_items<'a, T, I, R>(self, ui: &mut Ui, items: I, mut render: R) -> ComboboxResponse
    where
        F: Fn(&T) -> &str,
        I: IntoIterator<Item = &'a T>,
        T: 'a,
        R: FnMut(&mut ComboboxUi<'_>, &T),
    {
        let Combobox {
            id_salt,
            label,
            accessibility_label,
            placeholder,
            search_placeholder,
            search_accessibility_label,
            selected_text,
            lead,
            more,
            width,
            size,
            select_all,
            item_height,
            searchable,
            multi_select,
            noun,
            enabled,
            disabled_reason,
            forced_state,
            preview,
            read_only,
            filter: WithFilter(filter_fn),
        } = self;

        let state_id = ui.make_persistent_id(id_salt);
        let mut state = ui.ctx().memory_mut(|mem| {
            mem.data
                .get_temp::<ComboboxState>(state_id)
                .unwrap_or_default()
        });
        if let Some(preview) = &preview {
            state.filter_text.clone_from(&preview.query);
        }

        if let Some(label) = &label {
            // A field label (`.field > .lab`), 6 points above the trigger.
            type_scale::FIELD_LABEL.label(ui, label.text(), ui.tokens().text.secondary);
            ui.add_space((space::LABEL_TO_CONTROL - ui.spacing().item_spacing.y).max(0.0));
        }

        let width = width.unwrap_or(DEFAULT_WIDTH);
        let is_open = state.was_open || preview.is_some();
        let keyboard = if preview.is_none() {
            handle_keyboard(ui, is_open, multi_select)
        } else {
            KeyboardInput::default()
        };
        let enabled = enabled && ui.is_enabled();
        let label_text = label.as_ref().map(|label| label.text().to_owned());
        let accessibility_label = accessibility_label
            .or_else(|| label_text.clone())
            .unwrap_or_else(|| "Combobox".to_owned());
        let trigger = if preview.as_ref().is_none_or(|preview| preview.trigger) {
            render_trigger(
                ui,
                TriggerInput {
                    width,
                    is_open,
                    placeholder: placeholder.as_deref(),
                    selected_text: selected_text.as_deref(),
                    lead,
                    more,
                    size,
                    enabled,
                    interactive: !read_only,
                    forced_state,
                },
            )
        } else {
            ui.allocate_exact_size(Vec2::ZERO, Sense::hover()).1
        };
        let trigger = match (&disabled_reason, enabled) {
            (Some(reason), false) => trigger.disabled_tooltip(reason),
            _ => trigger,
        };
        // The value the trigger shows, as it is read ("kube-system +1").
        let value = selected_text
            .as_deref()
            .filter(|text| !text.is_empty())
            .map(|text| match more {
                0 => text.to_owned(),
                more => format!("{text} +{more}"),
            });
        trigger.widget_info(|| egui::WidgetInfo {
            current_text_value: value.clone(),
            ..egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, enabled, &accessibility_label)
        });
        ui.ctx().accesskit_node_builder(trigger.id, |node| {
            if read_only {
                node.set_read_only();
            } else {
                node.set_expanded(is_open);
            }
        });

        let popup_id = Popup::default_response_id(&trigger);
        let items: Vec<&T> = items.into_iter().collect();
        let theme = ui.tokens();
        let inner_width = width - 2.0 * crate::frames::POPOVER_PADDING;
        let mut select_all_clicked = false;
        let popup_label = format!("{accessibility_label} options");
        let search_label = search_accessibility_label.unwrap_or_else(|| {
            label_text.as_ref().map_or_else(
                || "Search combobox options".to_owned(),
                |label| format!("Search {label}"),
            )
        });
        let search_placeholder = search_placeholder.or(placeholder);
        let previous_filter = ui.ctx().memory_mut(|mem| {
            mem.data
                .get_temp::<String>(state_id.with("filter"))
                .unwrap_or_default()
        });
        let highlight = preview.as_ref().and_then(|preview| preview.highlight);
        let mut body = |ui: &mut Ui, state: &mut ComboboxState| {
            ui.set_width(inner_width);
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            theme::scope(ui, ThemeScope::Popover, |ui| {
                // The query is edited first, so typing and Enter in one frame
                // act on the new filter.
                let search = searchable.then(|| {
                    let (text, row) = render_search_editor(
                        ui,
                        state,
                        SearchRow {
                            id: trigger.id.with("input"),
                            placeholder: search_placeholder.as_deref(),
                            accessibility_label: &search_label,
                            total: items.len(),
                            caret: preview.as_ref().is_some_and(|preview| preview.trigger),
                        },
                    );
                    if preview.is_none() && !text.has_focus() {
                        text.request_focus();
                    }
                    row
                });

                state.filter_chars = normalize_for_search(&state.filter_text).collect();
                let mut filtered_items: Vec<_> = items
                    .iter()
                    .filter_map(|item| {
                        fuzzy_match_score(filter_fn(item), &state.filter_chars)
                            .map(|score| (score, *item))
                    })
                    .collect();
                if !state.filter_chars.is_empty() {
                    filtered_items
                        .sort_by(|(left_score, _), (right_score, _)| right_score.cmp(left_score));
                }
                if let Some(row) = search {
                    // No count when nothing matched: the empty state says so.
                    let count = (!state.filter_text.is_empty() && !filtered_items.is_empty())
                        .then_some((filtered_items.len(), items.len()));
                    let trailing = if state.filter_text.is_empty() {
                        Trailing::EscapeKey
                    } else {
                        count.map_or(Trailing::None, |(matches, total)| {
                            Trailing::Count(matches, total)
                        })
                    };
                    paint_search_trailing(ui, row, trailing);
                }

                let select_all_visible = select_all.is_some() && state.filter_text.is_empty();
                let item_count = filtered_items.len() + usize::from(select_all_visible);
                let first_item_row = usize::from(select_all_visible);
                let row_of = |row: &PinnedRow| match row {
                    PinnedRow::SelectAll => select_all_visible.then_some(0),
                    PinnedRow::Item { text, occurrence } => filtered_items
                        .iter()
                        .enumerate()
                        .filter(|(_, (_, item))| filter_fn(item) == text)
                        .nth(*occurrence)
                        .map(|(position, _)| position + first_item_row),
                };
                // Filtering re-ranks the rows (and toggles the select-all
                // row), so the index from the previous frame may point at an
                // unrelated item. Follow an explicitly chosen row by
                // identity; once it no longer exists, or when there is no
                // explicit choice and the filter was edited, restart from
                // the top row like a freshly opened popup.
                let filter_edited = previous_filter != state.filter_text;
                let previous_index = state.focused_index;
                let mut focused_index = match state.pinned_row.as_ref().map(row_of) {
                    Some(Some(row)) => row,
                    Some(None) => {
                        state.pinned_row = None;
                        0
                    }
                    None if filter_edited => 0,
                    None => previous_index,
                };
                let followed_to_new_index =
                    state.pinned_row.is_some() && focused_index != previous_index;
                if item_count > 0 {
                    focused_index = focused_index
                        .saturating_add_signed(keyboard.move_focus_by)
                        .min(item_count - 1);
                    if keyboard.move_focus_by != 0 {
                        state.pinned_row = Some(if select_all_visible && focused_index == 0 {
                            PinnedRow::SelectAll
                        } else {
                            let position = focused_index - first_item_row;
                            let text = filter_fn(filtered_items[position].1);
                            let occurrence = filtered_items[..position]
                                .iter()
                                .filter(|(_, item)| filter_fn(item) == text)
                                .count();
                            PinnedRow::Item {
                                text: text.to_owned(),
                                occurrence,
                            }
                        });
                    }
                }
                state.focused_index = highlight.unwrap_or(focused_index);
                let content_height = item_count as f32 * item_height
                    + if select_all_visible {
                        size::OPTION_SEPARATOR
                    } else {
                        0.0
                    };
                let scroll_to_focused = (keyboard.scroll_to_focused || followed_to_new_index)
                    && content_height > DROPDOWN_MAX_HEIGHT;

                if item_count == 0 {
                    if !state.filter_text.is_empty() {
                        render_empty(ui, &noun, &state.filter_text);
                    }
                } else {
                    // The popup sizes to its contents before they are laid out, so
                    // the list takes its estimated height, or the measured one
                    // while the filter is unchanged.
                    let estimate = match state.measured_height {
                        Some(measured) if !filter_edited => measured,
                        _ => content_height,
                    };
                    let list_height = estimate.min(DROPDOWN_MAX_HEIGHT);
                    let output = crate::scroll::vertical()
                        .id_salt(popup_id.with("options"))
                        .max_height(list_height)
                        .min_scrolled_height(list_height)
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing = Vec2::ZERO;
                            let mut cb = ComboboxUi {
                                ui,
                                focused_index: state.focused_index,
                                current_index: 0,
                                popup_id,
                                enter_pressed: keyboard.enter_pressed,
                                space_pressed: keyboard.space_pressed,
                                scroll_to_focused,
                                item_height,
                                multi_select,
                            };

                            if let Some(select_all) =
                                select_all.as_ref().filter(|_| select_all_visible)
                            {
                                if cb
                                    .select_all_item(&select_all.label, select_all.state)
                                    .clicked()
                                {
                                    select_all_clicked = true;
                                }
                                crate::menu::separator(cb.ui);
                            }

                            for (_, item) in &filtered_items {
                                render(&mut cb, item);
                            }
                        });
                    state.measured_height = Some(output.content_size.y);
                }
                // The key hints show while browsing; a search keeps the room.
                if multi_select && state.filter_text.is_empty() {
                    render_footer(ui);
                }
            });
        };
        if let Some(preview) = &preview {
            ui.add_space(preview.gap);
            let response =
                crate::frames::show_anchored_popover(ui, &theme, |ui| body(ui, &mut state))
                    .response;
            ui.ctx().accesskit_node_builder(response.id, |builder| {
                builder.set_label(popup_label);
            });
        } else if let Some(popup) = Popup::menu(&trigger)
            .width(width)
            .gap(size::POPOVER_GAP)
            .frame(egui::Frame::NONE)
            .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| crate::frames::show_anchored_popover(ui, &theme, |ui| body(ui, &mut state)))
        {
            ui.ctx()
                .accesskit_node_builder(popup.response.id, |builder| {
                    builder.set_label(popup_label);
                });
        }

        if keyboard.close_requested {
            Popup::close_id(ui.ctx(), popup_id);
        }
        let is_open_after = preview.is_some() || Popup::is_id_open(ui.ctx(), popup_id);
        if state.was_open && !is_open_after {
            state.filter_text.clear();
            state.filter_chars.clear();
            state.focused_index = 0;
            state.pinned_row = None;
            state.measured_height = None;
        }
        state.was_open = is_open_after && preview.is_none();
        let filter_text = state.filter_text.clone();
        ui.ctx().memory_mut(|mem| {
            mem.data.insert_temp(state_id.with("filter"), filter_text);
            mem.data.insert_temp(state_id, state);
        });

        ComboboxResponse {
            response: trigger,
            select_all_clicked,
        }
    }
}

/// Paint the trigger (`.sel`): the field well, the lead, the value or
/// placeholder, the `+N` chip and the chevron.
fn render_trigger(ui: &mut Ui, input: TriggerInput<'_>) -> Response {
    let TriggerInput {
        width,
        is_open,
        placeholder,
        selected_text,
        lead,
        more,
        size: field_size,
        enabled,
        interactive,
        forced_state,
    } = input;
    let sense = if enabled && interactive {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(vec2(width, field_size.height()), sense);
    let response = if enabled && interactive {
        response.with_pointing_hand()
    } else {
        response
    };
    let mut interaction = Interaction::of(&response, forced_state, false);
    interaction.focused |= is_open;
    let state = FieldState::new(interaction, false, enabled);
    let theme = ui.tokens();
    let role = field_size.role();
    let center_y = rect.center().y;

    let chevron = Rect::from_min_size(
        pos2(
            rect.right() - field_size.select_padding_right() - size::ICON_SM,
            center_y - size::ICON_SM / 2.0,
        ),
        Vec2::splat(size::ICON_SM),
    );
    let mut left = rect.left() + field_size.select_padding_left();
    let value = selected_text.filter(|text| !text.is_empty());
    let mut right = chevron.left() - space::S8;
    let more_galley = (more > 0 && value.is_some()).then(|| {
        MORE_ROLE.galley(
            ui.ctx(),
            &format!("+{more}"),
            theme.text.secondary,
            f32::INFINITY,
        )
    });
    let more_rect = more_galley.as_ref().map(|galley| {
        let chip_width = galley.rect.width() + 2.0 * size::MORE_CHIP_PADDING_X;
        right -= chip_width + space::S8;
        Rect::from_min_size(
            pos2(right + space::S8, center_y - galley.rect.height() / 2.0),
            vec2(chip_width, galley.rect.height()),
        )
    });

    let mut truncated = false;
    if ui.is_rect_visible(rect.expand(size::FIELD_HALO)) {
        // A disabled trigger keeps full opacity (`.sel.is-disabled`).
        let opacity = crate::control::disabled_fade_compensation(ui);
        crate::control::with_opacity(ui, opacity, |ui| {
            ui.painter()
                .add(field::field_shape(&theme, rect, radius::FULL.into(), state));
            match lead.filter(|_| value.is_some()) {
                Some(Lead::Status(is_active)) => {
                    paint_status_marker(ui, &theme, left, center_y, is_active);
                    left += size::STATUS_DOT + space::S8;
                }
                Some(Lead::Icon { icon, muted }) => {
                    let icon_rect = Rect::from_min_size(
                        pos2(left, center_y - size::ICON_SM / 2.0),
                        Vec2::splat(size::ICON_SM),
                    );
                    let color = if muted {
                        theme.text.muted
                    } else {
                        field::value_color(&theme, enabled)
                    };
                    icon.paint(ui, icon_rect, color);
                    left += size::ICON_SM + space::S8;
                }
                None => {}
            }
            let (text, color) = match value {
                Some(value) => (Some(value), field::value_color(&theme, enabled)),
                None => (placeholder, theme.text.disabled),
            };
            if let Some(text) = text {
                let galley = field::text_galley(ui, role, text, right - left, false);
                truncated = galley.elided;
                let pos = pos2(left, center_y - galley.rect.height() / 2.0);
                type_scale::paint_galley(ui.painter(), pos, galley, color);
            }
            if let (Some(galley), Some(chip)) = (more_galley, more_rect) {
                ui.painter()
                    .rect_filled(chip, radius::FULL, theme.control.fill_strong);
                let pos = pos2(chip.left() + size::MORE_CHIP_PADDING_X, chip.top());
                type_scale::paint_galley(ui.painter(), pos, galley, theme.text.secondary);
            }
            Icon::ChevronUpDown.paint(ui, chevron, theme.text.muted);
        });
    }
    match value {
        Some(value) if truncated => response.tooltip(value),
        _ => response,
    }
}

/// The search row of a popover.
struct SearchRow<'a> {
    id: Id,
    placeholder: Option<&'a str>,
    accessibility_label: &'a str,
    /// How many options there are, to reserve room for the count.
    total: usize,
    /// Draw the design's caret (design boards).
    caret: bool,
}

/// The search row on top of a popover (`.pop-search`), across the whole
/// popover with its padding, above a divider.
fn search_row_rect(slot: Rect) -> Rect {
    let padding = crate::frames::POPOVER_PADDING;
    Rect::from_min_max(
        slot.min - Vec2::splat(padding),
        slot.max + vec2(padding, 0.0),
    )
}

/// The count of matches (`2 of 5`) at the end of the search row.
fn count_galley(ui: &Ui, matches: usize, total: usize) -> std::sync::Arc<egui::Galley> {
    type_scale::META.galley(
        ui.ctx(),
        &format!("{} of {}", format_count(matches), format_count(total)),
        ui.tokens().text.muted,
        f32::INFINITY,
    )
}

/// Add the search row's divider, magnifier and query editor; the end of
/// the row is painted by [`paint_search_trailing`] once the matches are
/// known. Returns the editor and the row.
fn render_search_editor(
    ui: &mut Ui,
    state: &mut ComboboxState,
    row: SearchRow<'_>,
) -> (Response, Rect) {
    let padding = crate::frames::POPOVER_PADDING;
    let (slot, _) = ui.allocate_exact_size(
        vec2(ui.available_width(), size::POPOVER_SEARCH - padding),
        Sense::hover(),
    );
    ui.add_space(padding);
    let rect = search_row_rect(slot);
    let theme = ui.tokens();
    let center_y = rect.center().y;
    ui.painter().hline(
        rect.x_range(),
        rect.bottom() - size::HAIRLINE / 2.0,
        stroke::divider(&theme),
    );
    let mut left = rect.left() + size::POPOVER_SEARCH_PADDING_X;
    Icon::MagnifyingGlass.paint(
        ui,
        Rect::from_min_size(
            pos2(left, center_y - size::ICON / 2.0),
            Vec2::splat(size::ICON),
        ),
        theme.text.muted,
    );
    left += size::ICON + space::S8;
    let trailing = if state.filter_text.is_empty() {
        Keycap::new("Esc").size(ui).x
    } else {
        count_galley(ui, row.total, row.total).rect.width()
    };
    let right = rect.right() - size::POPOVER_SEARCH_PADDING_X - trailing - space::S8;

    // The design's caret (`.caret`, 1 × 16 `text-primary`) stands between the
    // magnifier and the placeholder, or after the query, a gap from each.
    let role = type_scale::UI;
    if row.caret {
        let caret_left = if state.filter_text.is_empty() {
            let caret_left = left;
            left += size::HAIRLINE + space::S8;
            caret_left
        } else {
            left + role
                .galley(
                    ui.ctx(),
                    &state.filter_text,
                    theme.text.primary,
                    f32::INFINITY,
                )
                .rect
                .width()
                + space::S8
        };
        let caret = Rect::from_min_size(
            pos2(caret_left, center_y - size::CARET.y / 2.0),
            vec2(size::HAIRLINE, size::CARET.y),
        );
        ui.painter().rect_filled(caret, 0, theme.text.primary);
    }
    let text_rect = Rect::from_min_max(
        pos2(left, center_y - role.line_height() / 2.0),
        pos2(right, center_y + role.line_height() / 2.0),
    );
    let response = field::show_editor(
        ui,
        text_rect,
        &mut state.filter_text,
        &field::EditorSpec {
            id: row.id,
            role,
            color: theme.text.primary,
            placeholder: row.placeholder,
            placeholder_color: theme.text.disabled,
            accessibility_label: row.accessibility_label,
            enabled: true,
            multiline: false,
            margin: egui::Margin::ZERO,
        },
    );
    (response, rect)
}

/// What ends the search row.
enum Trailing {
    /// The Esc key hint, while the query is empty.
    EscapeKey,
    /// How many of the options match ("2 of 5").
    Count(usize, usize),
    /// Nothing (no option matched; the empty state says so).
    None,
}

fn paint_search_trailing(ui: &Ui, rect: Rect, trailing: Trailing) {
    let right = rect.right() - size::POPOVER_SEARCH_PADDING_X;
    let center_y = rect.center().y;
    match trailing {
        Trailing::None => {}
        Trailing::Count(matches, total) => {
            let galley = count_galley(ui, matches, total);
            let pos = pos2(
                right - galley.rect.width(),
                center_y - galley.rect.height() / 2.0,
            );
            type_scale::paint_galley(ui.painter(), pos, galley, ui.tokens().text.muted);
        }
        Trailing::EscapeKey => {
            let key = Keycap::new("Esc");
            let key_size = key.size(ui);
            key.paint(
                ui,
                Rect::from_min_size(
                    pos2(right - key_size.x, center_y - key_size.y / 2.0),
                    key_size,
                ),
            );
        }
    }
}

/// The empty state of a search that matched nothing (`.pop-empty`).
fn render_empty(ui: &mut Ui, noun: &str, query: &str) {
    let theme = ui.tokens();
    let role = type_scale::UI;
    let text = format!("No {noun} match “{query}”");
    let galley = role.galley(ui.ctx(), &text, theme.text.muted, ui.available_width());
    let height = galley.rect.height() + 2.0 * space::S16;
    let (rect, response) = type_scale::allocate_text(ui, vec2(ui.available_width(), height), &text);
    let pos = pos2(
        rect.center().x - galley.rect.width() / 2.0,
        rect.top() + space::S16,
    );
    type_scale::paint_text(ui, &response, pos, galley, theme.text.muted);
}

/// The key-hint footer of a multi-select popover.
fn render_footer(ui: &mut Ui) {
    crate::popover_footer::popover_footer(
        ui,
        &[
            (&["↑", "↓"], "move"),
            (&["Space"], "toggle"),
            (&["Enter"], "done"),
        ],
    );
}

fn handle_keyboard(ui: &mut Ui, is_open: bool, multi_select: bool) -> KeyboardInput {
    if !is_open {
        return KeyboardInput::default();
    }

    let mut keyboard = KeyboardInput::default();
    ui.input_mut(|input| {
        if input.consume_key(Modifiers::NONE, Key::ArrowDown) {
            keyboard.move_focus_by += 1;
            keyboard.scroll_to_focused = true;
        }
        if input.consume_key(Modifiers::NONE, Key::ArrowUp) {
            keyboard.move_focus_by -= 1;
            keyboard.scroll_to_focused = true;
        }
        if input.consume_key(Modifiers::NONE, Key::Enter) {
            keyboard.enter_pressed = true;
            // A multi-select popover is "done": its toggles already applied.
            keyboard.close_requested |= multi_select;
        }
        if multi_select && consume_key_with_text(input, Modifiers::NONE, Key::Space) {
            keyboard.space_pressed = true;
        }
        if input.consume_key(Modifiers::NONE, Key::Escape) {
            keyboard.close_requested = true;
        }
    });
    keyboard
}
