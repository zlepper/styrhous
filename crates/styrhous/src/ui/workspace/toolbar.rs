use super::*;

struct NamespaceOption<'a> {
    namespace: &'a MinimalNamespace,
    presentation: crate::ui::namespace_selector::NamespacePresentation,
}

pub(super) fn show_toolbar(
    ui: &mut egui::Ui,
    cluster: &super::super::state::ClusterState,
    selected_api_resource: Option<&crate::api_resource::ApiResource>,
    resource_counts: ResourceCountSummary,
    resource_search: &mut ResourceSearchState,
    namespace_selection: &mut Option<NamespaceSelection>,
    selection_controls: ResourceSelectionControls<'_>,
) {
    let selected_text = match cluster.selected_namespaces.len() {
        0 => "Select namespaces".to_owned(),
        1 => cluster
            .selected_namespaces
            .iter()
            .next()
            .and_then(|selected| {
                cluster
                    .namespaces
                    .values()
                    .find(|namespace| namespace.name == *selected)
            })
            .map(|namespace| {
                crate::ui::namespace_selector::presentation(
                    namespace,
                    selection_controls.namespace_selector_settings,
                )
                .primary
            })
            .unwrap_or_default(),
        count => format!("{count} namespaces"),
    };
    let namespaces = cluster
        .namespaces
        .values()
        .map(|namespace| NamespaceOption {
            namespace,
            presentation: crate::ui::namespace_selector::presentation(
                namespace,
                selection_controls.namespace_selector_settings,
            ),
        })
        .collect::<Vec<_>>();
    let all_namespaces_selected = !namespaces.is_empty()
        && namespaces.iter().all(|namespace| {
            cluster
                .selected_namespaces
                .contains(&namespace.namespace.name)
        });
    let selected_status = if !selected_api_resource.is_some_and(|resource| resource.namespaced) {
        selected_api_resource.map(|api_resource| {
            cluster
                .active_watchers
                .contains(&(api_resource.clone(), None))
        })
    } else if cluster.selected_namespaces.len() == 1 {
        selected_api_resource.map(|api_resource| {
            let namespace = cluster
                .selected_namespaces
                .iter()
                .next()
                .expect("selection length was checked");
            cluster
                .active_watchers
                .contains(&(api_resource.clone(), Some(namespace.clone())))
        })
    } else {
        None
    };

    ui.add_space(TOOLBAR_VERTICAL_PADDING);
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), TOOLBAR_CONTENT_HEIGHT),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.add_space(37.0);
            if selected_api_resource.is_some_and(|resource| !resource.namespaced) {
                ui.label(
                    egui::RichText::new("Scope")
                        .font(typography::body())
                        .color(gray::_700),
                );
                ui.add_space(7.0);
                ui.label(
                    egui::RichText::new("Cluster-wide")
                        .font(typography::body())
                        .color(gray::_700),
                );
            } else {
                ui.label(
                    egui::RichText::new("Namespace")
                        .font(typography::body())
                        .color(gray::_700),
                );
                ui.add_space(7.0);
                let namespace_response = TailwindCombobox::new("namespace-selector")
                    .accessibility_label("Namespace")
                    .placeholder("Search namespaces...")
                    .search_accessibility_label("Search Namespace")
                    .selected_text(selected_text)
                    .selected_status(selected_status)
                    .width(230.0)
                    .compact()
                    .multiline_items()
                    .select_all(all_namespaces_selected)
                    .filter_by(|option: &NamespaceOption| &option.presentation.search_text)
                    .show_items(ui, &namespaces, |cb, ns| {
                        let status = selected_api_resource.map(|api_resource| {
                            cluster
                                .active_watchers
                                .contains(&(api_resource.clone(), Some(ns.namespace.name.clone())))
                        });
                        if let Some(action) = cb
                            .item_with_status_detail(
                                &ns.presentation.primary,
                                &ns.presentation.secondary,
                                cluster.selected_namespaces.contains(&ns.namespace.name),
                                status,
                            )
                            .selection_action()
                        {
                            *namespace_selection = Some(match action {
                                SelectionAction::Replace => {
                                    NamespaceSelection::Replace(ns.namespace.name.clone())
                                }
                                SelectionAction::Toggle => {
                                    NamespaceSelection::Toggle(ns.namespace.name.clone())
                                }
                            });
                        }
                    });
                if namespace_response.select_all_clicked {
                    *namespace_selection = Some(if all_namespaces_selected {
                        NamespaceSelection::ClearAll
                    } else {
                        NamespaceSelection::SelectAll
                    });
                }
                namespace_response.response.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::ComboBox,
                        ui.is_enabled(),
                        "Namespace",
                    )
                });
            }

            if selected_api_resource.is_some() {
                StripBuilder::new(ui)
                    .size(Size::remainder())
                    .size(Size::exact(RESOURCE_SEARCH_WIDTH))
                    .size(Size::exact(TOOLBAR_RIGHT_INSET))
                    .clip(true)
                    .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                    .horizontal(|mut strip| {
                        strip.cell(|ui| {
                            ui.add_space(15.0);
                            ui.separator();
                            ui.add_space(18.0);
                            if selection_controls.selected_count == 0 {
                                ui.label(
                                    egui::RichText::new(resource_count_label(
                                        resource_counts.total,
                                        resource_counts.visible,
                                        !resource_search.query.is_empty(),
                                    ))
                                    .font(typography::section_heading())
                                    .color(gray::_500),
                                );
                            } else {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{} selected",
                                        selection_controls.selected_count
                                    ))
                                    .font(typography::section_heading())
                                    .color(gray::_700),
                                );
                                ui.add_space(spacing::MD);
                                if TailwindButton::secondary("Clear selection")
                                    .size(ButtonSize::Xs)
                                    .show(ui)
                                    .clicked()
                                {
                                    *selection_controls.action =
                                        Some(ResourceSelectionAction::Clear);
                                }
                                let delete =
                                    TailwindButton::danger("Delete selected").size(ButtonSize::Xs);
                                if ui
                                    .add_enabled_ui(selection_controls.actions_enabled, |ui| {
                                        delete.show(ui)
                                    })
                                    .inner
                                    .clicked()
                                {
                                    *selection_controls.action =
                                        Some(ResourceSelectionAction::Delete);
                                }
                            }
                        });
                        strip.cell(|ui| {
                            ui.allocate_ui_with_layout(
                                egui::vec2(RESOURCE_SEARCH_WIDTH, TOOLBAR_CONTENT_HEIGHT),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| show_resource_search(ui, resource_search),
                            );
                        });
                        strip.empty();
                    });
            }
        },
    );
}

pub(super) fn show_resource_search(ui: &mut egui::Ui, resource_search: &mut ResourceSearchState) {
    let invalid = regex_error(resource_search).is_some();
    let focus_search = ui
        .ctx()
        .input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::F));
    TailwindSearchInput::new(&mut resource_search.query, &mut resource_search.regex_mode)
        .hint_text("Search resources...")
        .id_salt("resource-search-input")
        .accessibility_label("Search resources")
        .invalid(invalid)
        .focus(focus_search)
        .show(ui);
}

pub(super) fn filter_resources(
    all_resources: &[MinimalResource],
    resource_search: &ResourceSearchState,
) -> FilteredResources {
    if resource_search.query.is_empty() {
        return FilteredResources {
            resources: all_resources.to_vec(),
        };
    }

    if resource_search.regex_mode {
        let regex = match regex::RegexBuilder::new(&resource_search.query)
            .case_insensitive(true)
            .build()
        {
            Ok(regex) => regex,
            Err(_) => {
                return FilteredResources {
                    resources: Vec::new(),
                };
            }
        };
        return FilteredResources {
            resources: all_resources
                .iter()
                .filter(|resource| {
                    let normalized_name: String = normalize_for_search(&resource.name).collect();
                    regex.is_match(&normalized_name)
                })
                .cloned()
                .collect(),
        };
    }

    let query: Vec<char> = normalize_for_search(&resource_search.query).collect();
    if query.is_empty() {
        return FilteredResources {
            resources: all_resources.to_vec(),
        };
    }
    let mut scored_resources = all_resources
        .iter()
        .filter_map(|resource| {
            fuzzy_match_score(&resource.name, &query).map(|score| (score, resource))
        })
        .collect::<Vec<_>>();
    scored_resources.sort_by(|(left_score, _), (right_score, _)| right_score.cmp(left_score));
    FilteredResources {
        resources: scored_resources
            .into_iter()
            .map(|(_, resource)| resource.clone())
            .collect(),
    }
}

pub(super) fn regex_error(resource_search: &ResourceSearchState) -> Option<String> {
    if resource_search.regex_mode && !resource_search.query.is_empty() {
        regex::RegexBuilder::new(&resource_search.query)
            .case_insensitive(true)
            .build()
            .err()
            .map(|error| format!("Invalid regular expression: {error}"))
    } else {
        None
    }
}

pub(super) fn resource_count_label(
    total_count: usize,
    visible_count: usize,
    search_is_active: bool,
) -> String {
    if search_is_active {
        format!("{visible_count} of {total_count} items")
    } else {
        format!("{total_count} items")
    }
}
