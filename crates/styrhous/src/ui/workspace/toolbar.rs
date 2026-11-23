use super::*;

struct NamespaceOption<'a> {
    namespace: &'a MinimalNamespace,
    presentation: crate::ui::namespace_selector::NamespacePresentation,
}

/// What the namespace selector says is selected.
pub(super) fn namespace_selector_text(
    cluster: &super::super::state::ClusterState,
    settings: &NamespaceSelectorSettings,
) -> String {
    let selected_namespaces = cluster.namespace_scope();
    match selected_namespaces.len() {
        0 if cluster.selected_namespaces == SelectedNamespaces::All => "All namespaces".to_owned(),
        0 if matches!(cluster.namespaces_load, ClusterLoadState::Loading) => {
            "Loading namespaces...".to_owned()
        }
        0 => "Select namespaces".to_owned(),
        1 => selected_namespaces
            .iter()
            .next()
            .map(|selected| {
                // A restored namespace is named before the namespace list arrives.
                cluster
                    .namespaces
                    .values()
                    .find(|namespace| namespace.name == *selected)
                    .map_or_else(
                        || selected.clone(),
                        |namespace| {
                            crate::ui::namespace_selector::presentation(namespace, settings).primary
                        },
                    )
            })
            .unwrap_or_default(),
        count => format!("{count} namespaces"),
    }
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
    let theme = ui.tokens();
    let selected_namespaces = cluster.namespace_scope();
    let selected_text =
        namespace_selector_text(cluster, selection_controls.namespace_selector_settings);
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
    let all_namespaces_selected = selected_namespaces.covers_all_discovered();
    let selected_status = if !selected_api_resource.is_some_and(|resource| resource.namespaced) {
        selected_api_resource.map(|api_resource| {
            cluster
                .active_watchers
                .contains(&(api_resource.clone(), None))
        })
    } else if selected_namespaces.len() == 1 {
        selected_api_resource.map(|api_resource| {
            let namespace = selected_namespaces
                .iter()
                .next()
                .expect("selection length was checked");
            cluster.is_namespace_watched(api_resource, namespace)
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
                        .font(type_scale::BODY.font_id(ui.ctx()))
                        .color(theme.text.secondary),
                );
                ui.add_space(7.0);
                ui.label(
                    egui::RichText::new("Cluster-wide")
                        .font(type_scale::BODY.font_id(ui.ctx()))
                        .color(theme.text.secondary),
                );
            } else {
                ui.label(
                    egui::RichText::new("Namespace")
                        .font(type_scale::BODY.font_id(ui.ctx()))
                        .color(theme.text.secondary),
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
                            cluster.is_namespace_watched(api_resource, &ns.namespace.name)
                        });
                        if let Some(action) = cb
                            .item_with_status_detail(
                                &ns.presentation.primary,
                                &ns.presentation.secondary,
                                selected_namespaces.contains(&ns.namespace.name),
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
                                    .font(type_scale::TITLE.font_id(ui.ctx()))
                                    .color(theme.text.muted),
                                );
                            } else {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{} selected",
                                        selection_controls.selected_count
                                    ))
                                    .font(type_scale::TITLE.font_id(ui.ctx()))
                                    .color(theme.text.secondary),
                                );
                                ui.add_space(space::S12);
                                if TailwindButton::secondary("Clear selection")
                                    .size(ButtonSize::Xs)
                                    .show(ui)
                                    .clicked()
                                {
                                    *selection_controls.action =
                                        Some(ResourceSelectionAction::Clear);
                                }
                                if selection_controls.interleaved_logs_available
                                    && ui
                                        .add_enabled_ui(selection_controls.actions_enabled, |ui| {
                                            TailwindButton::secondary("View interleaved logs")
                                                .size(ButtonSize::Xs)
                                                .show(ui)
                                        })
                                        .inner
                                        .clicked()
                                {
                                    *selection_controls.action =
                                        Some(ResourceSelectionAction::ViewLogs);
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
                            // Trailing, so toggling it never shifts the controls
                            // before it.
                            let namespace_problem = match (
                                &cluster.namespaces_reconnecting,
                                &cluster.namespaces_load,
                            ) {
                                (Some(error), _) => Some((
                                    "Reconnecting...",
                                    format!("Namespace updates are paused: {error}"),
                                )),
                                (None, ClusterLoadState::Failed(error)) => Some((
                                    "Namespaces unavailable",
                                    format!("The namespace list could not be loaded: {error}"),
                                )),
                                _ => None,
                            };
                            if let Some((label, hover)) = namespace_problem {
                                ui.add_space(space::S12);
                                ui.label(
                                    egui::RichText::new(label)
                                        .font(type_scale::META.font_id(ui.ctx()))
                                        .color(theme.warning.text),
                                )
                                .on_hover_text(hover);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::state::ClusterState;

    fn text(selected: SelectedNamespaces, namespaces_load: ClusterLoadState) -> String {
        let mut cluster = ClusterState::for_test(1, "dev");
        cluster.selected_namespaces = selected;
        cluster.namespaces_load = namespaces_load;
        namespace_selector_text(&cluster, &NamespaceSelectorSettings::default())
    }

    #[test]
    fn the_selector_names_the_selection_before_the_namespace_list_arrives() {
        assert_eq!(
            text(SelectedNamespaces::default(), ClusterLoadState::Loading),
            "Loading namespaces..."
        );
        assert_eq!(
            text(SelectedNamespaces::All, ClusterLoadState::Loading),
            "All namespaces"
        );
        assert_eq!(
            text(
                SelectedNamespaces::from(["default"]),
                ClusterLoadState::Loading
            ),
            "default",
            "a restored namespace is named before it is listed"
        );
        assert_eq!(
            text(SelectedNamespaces::default(), ClusterLoadState::Ready),
            "Select namespaces"
        );
    }
}
