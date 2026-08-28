use super::*;

impl TailwindCombobox<NoFilter> {
    /// Create a new combobox with an explicit ID.
    pub fn new(id_salt: impl std::hash::Hash + std::fmt::Debug) -> Self {
        TailwindCombobox {
            id_salt: Id::new(id_salt),
            label: None,
            accessibility_label: None,
            placeholder: None,
            search_accessibility_label: None,
            selected_text: None,
            selected_status: None,
            width: None,
            size: ComboboxSize::Default,
            select_all: None,
            filter: NoFilter,
        }
    }

    /// Create a new combobox with the label as the ID source.
    pub fn from_label(label: impl Into<WidgetText>) -> Self {
        let label = label.into();
        TailwindCombobox {
            id_salt: Id::new(label.text()),
            label: Some(label),
            accessibility_label: None,
            placeholder: None,
            search_accessibility_label: None,
            selected_text: None,
            selected_status: None,
            width: None,
            size: ComboboxSize::Default,
            select_all: None,
            filter: NoFilter,
        }
    }

    /// Configure the text used for fuzzy filtering.
    pub fn filter_by<T, F>(self, filter_fn: F) -> TailwindCombobox<WithFilter<F>>
    where
        F: Fn(&T) -> &str,
    {
        TailwindCombobox {
            id_salt: self.id_salt,
            label: self.label,
            accessibility_label: self.accessibility_label,
            placeholder: self.placeholder,
            search_accessibility_label: self.search_accessibility_label,
            selected_text: self.selected_text,
            selected_status: self.selected_status,
            width: self.width,
            size: self.size,
            select_all: self.select_all,
            filter: WithFilter(filter_fn),
        }
    }
}

impl<Filter> TailwindCombobox<Filter> {
    /// Set the accessible name for the combobox and its options popup.
    pub fn accessibility_label(mut self, label: impl Into<String>) -> Self {
        self.accessibility_label = Some(label.into());
        self
    }

    /// Set the placeholder shown when the input is empty.
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
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

    /// Set the text displayed while the combobox is closed.
    pub fn selected_text(mut self, text: impl Into<String>) -> Self {
        self.selected_text = Some(text.into());
        self
    }

    /// Set the optional status marker shown beside the closed selected value.
    pub fn selected_status(mut self, status: Option<bool>) -> Self {
        self.selected_status = status;
        self
    }

    /// Set the width of the combobox.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width.max(0.0));
        self
    }

    /// Use the smaller sizing intended for dense toolbar controls.
    pub fn compact(mut self) -> Self {
        self.size = ComboboxSize::Compact;
        self
    }

    /// Enable a Select all row while the search field is empty.
    ///
    /// `all_selected` controls its checkmark. Its activation is reported by
    /// [`ComboboxResponse::select_all_clicked`].
    pub fn select_all(mut self, all_selected: bool) -> Self {
        self.select_all = Some(all_selected);
        self
    }
}

impl<F> TailwindCombobox<WithFilter<F>> {
    /// Show the combobox and call `render` for every filtered option.
    pub fn show_items<'a, T, I, R>(self, ui: &mut Ui, items: I, mut render: R) -> ComboboxResponse
    where
        F: Fn(&T) -> &str,
        I: IntoIterator<Item = &'a T>,
        T: 'a,
        R: FnMut(&mut ComboboxUi<'_>, &T),
    {
        let TailwindCombobox {
            id_salt,
            label,
            accessibility_label,
            placeholder,
            search_accessibility_label,
            selected_text,
            selected_status,
            width,
            size,
            select_all,
            filter: WithFilter(filter_fn),
        } = self;

        let state_id = ui.make_persistent_id(id_salt);
        let mut state = ui.ctx().memory_mut(|mem| {
            mem.data
                .get_temp::<ComboboxState>(state_id)
                .unwrap_or_default()
        });

        if let Some(label) = &label {
            ui.label(label.clone());
        }

        let width = width.unwrap_or(DEFAULT_WIDTH);
        let is_open = state.was_open;
        let keyboard = Self::handle_keyboard(ui, &mut state, is_open);
        let input_response = Self::render_input(
            ui,
            &mut state,
            ComboboxInput {
                width,
                is_open,
                placeholder: placeholder.as_deref(),
                accessibility_label: search_accessibility_label.unwrap_or_else(|| {
                    label.as_ref().map_or_else(
                        || "Search combobox options".to_owned(),
                        |label| format!("Search {}", label.text()),
                    )
                }),
                selected_text: selected_text.as_deref(),
                selected_status,
                size,
            },
        );

        let is_enabled = ui.is_enabled();
        let label_text = label.as_ref().map(|label| label.text().to_owned());
        let accessibility_label = accessibility_label
            .or_else(|| label_text.clone())
            .unwrap_or_else(|| "Combobox".to_owned());
        input_response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, is_enabled, &accessibility_label)
        });

        let popup_id = Popup::default_response_id(&input_response);
        state.filter_chars = normalize_for_search(&state.filter_text).collect();

        let mut filtered_items: Vec<_> = items
            .into_iter()
            .filter_map(|item| {
                fuzzy_match_score(filter_fn(item), &state.filter_chars).map(|score| (score, item))
            })
            .collect();
        if !state.filter_chars.is_empty() {
            filtered_items.sort_by(|(left_score, _), (right_score, _)| right_score.cmp(left_score));
        }
        let select_all_visible = select_all.is_some() && state.filter_text.is_empty();
        let item_count = filtered_items.len() + usize::from(select_all_visible);
        let item_spacing = ui.spacing().item_spacing.y;
        let content_height =
            item_count as f32 * ITEM_HEIGHT + item_count.saturating_sub(1) as f32 * item_spacing;
        let dropdown_height = content_height.clamp(ITEM_HEIGHT, DROPDOWN_MAX_HEIGHT);
        let scroll_to_focused = keyboard.scroll_to_focused && content_height > DROPDOWN_MAX_HEIGHT;
        if item_count > 0 {
            state.focused_index = state.focused_index.min(item_count - 1);
        }

        let mut select_all_clicked = false;
        let popup_label = format!("{accessibility_label} options");
        if let Some(popup) = Popup::menu(&input_response)
            .width(width)
            .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                ui.set_min_height(dropdown_height);
                crate::scroll::vertical()
                    .max_height(dropdown_height)
                    .min_scrolled_height(dropdown_height)
                    .show(ui, |ui| {
                        let mut cb = ComboboxUi {
                            ui,
                            focused_index: state.focused_index,
                            current_index: 0,
                            popup_id,
                            enter_pressed: keyboard.enter_pressed,
                            scroll_to_focused,
                        };

                        if let Some(all_selected) = select_all.filter(|_| select_all_visible)
                            && cb.select_all_item(all_selected).clicked()
                        {
                            select_all_clicked = true;
                        }

                        for (_, item) in &filtered_items {
                            render(&mut cb, item);
                        }
                    });
            })
        {
            ui.ctx()
                .accesskit_node_builder(popup.response.id, |builder| {
                    builder.set_label(popup_label);
                });
        }

        if keyboard.close_requested {
            Popup::close_id(ui.ctx(), popup_id);
        }
        let is_open_after = Popup::is_id_open(ui.ctx(), popup_id);
        if state.was_open && !is_open_after {
            state.filter_text.clear();
            state.filter_chars.clear();
            state.focused_index = 0;
        }
        state.was_open = is_open_after;
        ui.ctx()
            .memory_mut(|mem| mem.data.insert_temp(state_id, state));

        ComboboxResponse {
            response: input_response,
            select_all_clicked,
        }
    }

    fn render_input(ui: &mut Ui, state: &mut ComboboxState, input: ComboboxInput<'_>) -> Response {
        let ComboboxInput {
            width,
            is_open,
            placeholder,
            accessibility_label,
            selected_text,
            selected_status,
            size,
        } = input;
        let corner_radius = radius::control();
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(width, size.input_height()), Sense::click());
        let response = response.with_pointing_hand();
        let has_focus = response.has_focus()
            || ui.memory(|memory| memory.has_focus(response.id.with("input")))
            || is_open;

        if ui.is_rect_visible(rect) {
            ui.painter().rect_filled(rect, corner_radius, WHITE);
            ui.painter().rect_stroke(
                rect,
                corner_radius,
                Stroke::new(
                    if has_focus { 2.0 } else { 1.0 },
                    if has_focus { indigo::_500 } else { gray::_300 },
                ),
                StrokeKind::Inside,
            );

            let icon_rect = Rect::from_center_size(
                rect.right_center() - Vec2::new(size.icon_size() / 2.0 + 8.0, 0.0),
                Vec2::splat(size.icon_size()),
            );
            let mut icon_ui = ui.new_child(egui::UiBuilder::new().max_rect(icon_rect).layout(
                egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
            ));
            icons::chevron_down(&mut icon_ui, size.icon_size(), gray::_400);
        }

        let input_rect = Rect::from_min_max(
            rect.min + Vec2::new(INPUT_PADDING_X, 0.0),
            rect.max - Vec2::new(size.icon_area_width(), 0.0),
        );

        if !is_open {
            if let Some(text) = selected_text.filter(|text| !text.is_empty()) {
                if let Some(is_active) = selected_status {
                    ui.painter().circle_filled(
                        input_rect.left_center() + Vec2::new(5.0, 0.0),
                        5.0,
                        if is_active { SUCCESS } else { gray::_200 },
                    );
                }
                let text_pos = input_rect.left_center()
                    + Vec2::new(if selected_status.is_some() { 20.0 } else { 0.0 }, 0.0);
                let galley =
                    layout_truncated_text(ui, text, size.font(), gray::_900, input_rect.width());
                let is_truncated = galley.elided;
                ui.painter().galley(
                    text_pos - Vec2::new(0.0, galley.size().y / 2.0),
                    galley,
                    gray::_900,
                );
                return if is_truncated {
                    response.on_hover_text(text)
                } else {
                    response
                };
            }
            if let Some(placeholder) = placeholder {
                ui.painter().text(
                    input_rect.left_center(),
                    Align2::LEFT_CENTER,
                    placeholder,
                    size.font(),
                    gray::_400,
                );
            }
            return response;
        }

        let mut input_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(input_rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        let mut text_edit = TextEdit::singleline(&mut state.filter_text)
            .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(4, 2)))
            .desired_width(input_rect.width())
            .vertical_align(egui::Align::Center)
            .id(response.id.with("input"));
        if matches!(size, ComboboxSize::Compact) {
            text_edit = text_edit.font(size.font());
        }
        if let Some(placeholder) = placeholder {
            text_edit = text_edit.hint_text(placeholder);
        }
        let text_response = input_ui.add(text_edit);
        let is_enabled = input_ui.is_enabled();
        text_response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::TextEdit,
                is_enabled,
                accessibility_label.clone(),
            )
        });
        if !text_response.has_focus() {
            text_response.request_focus();
        }
        response | text_response
    }

    fn handle_keyboard(ui: &mut Ui, state: &mut ComboboxState, is_open: bool) -> KeyboardInput {
        if !is_open {
            return KeyboardInput::default();
        }

        let mut keyboard = KeyboardInput::default();
        ui.input_mut(|input| {
            if input.consume_key(Modifiers::NONE, Key::ArrowDown) {
                state.focused_index = state.focused_index.saturating_add(1);
                keyboard.scroll_to_focused = true;
            }
            if input.consume_key(Modifiers::NONE, Key::ArrowUp) {
                state.focused_index = state.focused_index.saturating_sub(1);
                keyboard.scroll_to_focused = true;
            }
            if input.consume_key(Modifiers::NONE, Key::Enter) {
                keyboard.enter_pressed = true;
            }
            if input.consume_key(Modifiers::NONE, Key::Escape) {
                keyboard.close_requested = true;
            }
        });
        keyboard
    }
}
