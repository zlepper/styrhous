use super::*;
use components::tooltip::TooltipExt;
use components::{LiveMeta, ScopePill, SearchPill, TopBar};

struct NamespaceOption<'a> {
    namespace: &'a MinimalNamespace,
    presentation: crate::ui::namespace_selector::NamespacePresentation,
}

/// What the top bar's live meta says about the list.
#[derive(Clone, Copy)]
pub(super) struct ListSummary {
    pub(super) total: usize,
    pub(super) visible: usize,
    /// The list is watched and has its first response.
    pub(super) live: bool,
}

/// The scope pill for the selected resource, and the placeholder its
/// combobox shows while no namespace is named.
pub(super) fn scope_pill(
    cluster: &super::super::state::ClusterState,
    selected_api_resource: Option<&crate::api_resource::ApiResource>,
    settings: &NamespaceSelectorSettings,
) -> (ScopePill, &'static str) {
    let placeholder = if matches!(cluster.namespaces_load, ClusterLoadState::Loading) {
        "Loading namespaces..."
    } else {
        "Select namespaces"
    };
    if selected_api_resource.is_some_and(|resource| !resource.namespaced) {
        return (ScopePill::cluster_wide(), placeholder);
    }
    if cluster.selected_namespaces == SelectedNamespaces::All {
        let count = matches!(cluster.namespaces_load, ClusterLoadState::Ready)
            .then_some(cluster.namespaces.len());
        return (ScopePill::all_namespaces(count), placeholder);
    }
    let selected_namespaces = cluster.namespace_scope();
    let names = selected_namespaces.iter().map(|selected| {
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
    });
    // The watch lamp is known only for a single namespace.
    let watching = selected_api_resource
        .filter(|_| selected_namespaces.len() == 1)
        .and_then(|api_resource| {
            let namespace = selected_namespaces.iter().next()?;
            Some(cluster.is_namespace_watched(api_resource, namespace))
        });
    (ScopePill::namespaces(names, watching), placeholder)
}

/// Why the namespace list is degraded, as the live meta's warning and its
/// tooltip.
fn namespace_problem(
    cluster: &super::super::state::ClusterState,
) -> Option<(&'static str, String)> {
    match (&cluster.namespaces_reconnecting, &cluster.namespaces_load) {
        (Some(error), _) => Some((
            "namespaces reconnecting",
            format!("Namespace updates are paused: {error}"),
        )),
        (None, ClusterLoadState::Failed(error)) => Some((
            "namespaces unavailable",
            format!("The namespace list could not be loaded: {error}"),
        )),
        _ => None,
    }
}

/// The singular of a resource's plural, for a count of one. The lower-cased
/// kind is Kubernetes' own singular ("configmap", "componentstatus") when
/// the plural starts with it; otherwise ("HelmRelease" for "releases",
/// "NetworkPolicy" for "networkpolicies") English rules undo the plural.
pub(super) fn singular_of<'a>(kind: &str, plural: &'a str) -> std::borrow::Cow<'a, str> {
    let kind = kind.to_lowercase();
    if plural.starts_with(&kind) {
        return plural[..kind.len()].to_owned().into();
    }
    if let Some(stem) = plural.strip_suffix("ies") {
        return format!("{stem}y").into();
    }
    let singular = if ["sses", "xes", "ches", "shes", "zes"]
        .iter()
        .any(|sibilant| plural.ends_with(sibilant))
    {
        &plural[..plural.len() - 2]
    } else if plural.ends_with("us") || plural.ends_with("ss") {
        // "prometheus" or "egress" is already its own singular.
        plural
    } else {
        plural.strip_suffix('s').unwrap_or(plural)
    };
    singular.into()
}

/// The list's top bar (C16): the search pill (disabled until a resource is
/// selected), the namespace scope pill and the live meta.
pub(super) fn show_toolbar(
    ui: &mut egui::Ui,
    cluster: &super::super::state::ClusterState,
    selected_api_resource: Option<&crate::api_resource::ApiResource>,
    summary: ListSummary,
    resource_search: &mut ResourceSearchState,
    namespace_selection: &mut Option<NamespaceSelection>,
    namespace_selector_settings: &NamespaceSelectorSettings,
) {
    let bar = TopBar::new(ui.ctx().content_rect().width());
    let namespaced = selected_api_resource.is_none_or(|resource| resource.namespaced);
    let selected_namespaces = cluster.namespace_scope();
    let namespaces = cluster
        .namespaces
        .values()
        .map(|namespace| NamespaceOption {
            namespace,
            presentation: crate::ui::namespace_selector::presentation(
                namespace,
                namespace_selector_settings,
            ),
        })
        .collect::<Vec<_>>();
    let all_namespaces_selected = selected_namespaces.covers_all_discovered();
    let all_namespaces_state = if all_namespaces_selected {
        CheckState::On
    } else if selected_namespaces.is_empty() {
        CheckState::Off
    } else {
        CheckState::Mixed
    };
    let (scope, placeholder) =
        scope_pill(cluster, selected_api_resource, namespace_selector_settings);
    let plural = selected_api_resource.map_or("resources", |api_resource| &api_resource.name);
    let kind = selected_api_resource.map_or("Resource", |api_resource| &api_resource.kind);
    let singular = singular_of(kind, plural);
    let count = resource_count_label(
        summary.total,
        summary.visible,
        !resource_search.query.is_empty(),
        (&singular, plural),
    );
    let problem = namespace_problem(cluster);

    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(TOP_BAR_INSET as i8, 0))
        .show(ui, |ui| {
            let top_bar = bar.show(
                ui,
                |ui| {
                    ui.add_enabled_ui(selected_api_resource.is_some(), |ui| {
                        show_resource_search(ui, resource_search, plural, bar.search_width());
                    });
                    let mut combobox = scope
                        .id_salt("namespace-selector")
                        .width(bar.scope_width())
                        .combobox()
                        .placeholder(placeholder)
                        .search_placeholder("Search namespaces…")
                        .search_accessibility_label("Search Namespace")
                        .multiline_items();
                    if namespaced {
                        combobox = combobox.select_all("All namespaces", all_namespaces_state);
                    }
                    let namespace_response = combobox
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
                },
                selected_api_resource.map(|_| {
                    LiveMeta::new(&count)
                        .live(summary.live)
                        .warning(problem.as_ref().map(|(warning, _)| *warning))
                        // The list cannot step through its problem rows yet.
                        .problem_keys(false)
                }),
            );
            if let (Some(meta), Some((_, hover))) = (top_bar.meta, &problem) {
                let _ = meta.tooltip(hover);
            }
        });
}

/// The search pill over the list of `noun` ("pods"): `/` or Ctrl+F
/// focuses it, and its regex toggle switches from fuzzy matching.
pub(super) fn show_resource_search(
    ui: &mut egui::Ui,
    resource_search: &mut ResourceSearchState,
    noun: &str,
    width: f32,
) {
    let focus_search = ui.is_enabled()
        && ui
            .ctx()
            .input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::F));
    let status = match regex_error(resource_search) {
        Some(error) => SearchStatus::Invalid(error),
        None => SearchStatus::Idle,
    };
    SearchPill::new(&mut resource_search.query)
        .regex(&mut resource_search.regex_mode)
        .placeholder(format!("Find {noun}"))
        .id_salt("resource-search-input")
        .accessibility_label("Search resources")
        .status(status)
        .width(width)
        .request_focus(focus_search)
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
    (singular, plural): (&str, &str),
) -> String {
    let total = components::counted(total_count, singular, plural);
    if search_is_active {
        format!(
            "{} of {total}",
            components::search::format_count(visible_count)
        )
    } else {
        total
    }
}

/// What the bulk selection bar offers.
pub(super) struct BulkSelection {
    pub(super) selected: usize,
    /// The selected rows the search hides.
    pub(super) hidden: usize,
    /// The rows the search shows.
    pub(super) visible: usize,
    pub(super) actions_enabled: bool,
    /// Pods can show their selected logs interleaved in one window.
    pub(super) interleaved_logs: bool,
}

/// The bulk selection bar between the toolbar and the table header (S08):
/// the count, the bulk actions and Clear selection.
pub(super) fn show_bulk_selection_bar(
    ui: &mut egui::Ui,
    selection: BulkSelection,
    action: &mut Option<ResourceSelectionAction>,
) {
    ui.add_space(BULK_BAR_GAP);
    // Like the header's checkbox, the bar's is ticked only when every row
    // shown is selected; rows the search hides count only when selected.
    let hidden_hint = format!("{} selected hidden by search", selection.hidden);
    let mut bar = BulkSelectionBar::new(selection.selected, selection.visible + selection.hidden);
    if selection.hidden > 0 {
        bar = bar.hint(&hidden_hint);
    }
    let response = bar.show(ui, |ui| {
        if selection.interleaved_logs
            && Button::ghost("View interleaved logs")
                .leading_icon(Icon::DocumentText)
                .size(ButtonSize::Sm)
                .enabled(selection.actions_enabled)
                .show(ui)
                .clicked()
        {
            *action = Some(ResourceSelectionAction::ViewLogs);
        }
        if Button::danger_quiet("Delete")
            .leading_icon(Icon::Trash)
            .accessibility_label("Delete selected")
            .size(ButtonSize::Sm)
            .enabled(selection.actions_enabled)
            .show(ui)
            .clicked()
        {
            *action = Some(ResourceSelectionAction::Delete);
        }
    });
    if response.clear {
        *action = Some(ResourceSelectionAction::Clear);
    }
    // The layout's own item spacing is part of the gap.
    ui.add_space(BULK_BAR_GAP - ui.spacing().item_spacing.y);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::state::ClusterState;

    /// What the scope pill shows: its text, or its placeholder.
    fn text(
        selected: SelectedNamespaces,
        namespaces_load: ClusterLoadState,
        api_resource: Option<&crate::api_resource::ApiResource>,
    ) -> String {
        let mut cluster = ClusterState::for_test(1, "dev");
        cluster.selected_namespaces = selected;
        cluster.namespaces_load = namespaces_load;
        let (pill, placeholder) = scope_pill(
            &cluster,
            api_resource,
            &NamespaceSelectorSettings::default(),
        );
        Some(pill.text())
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| placeholder.to_owned())
    }

    #[test]
    fn the_scope_pill_names_the_selection_before_the_namespace_list_arrives() {
        assert_eq!(
            text(
                SelectedNamespaces::default(),
                ClusterLoadState::Loading,
                None
            ),
            "Loading namespaces..."
        );
        assert_eq!(
            text(SelectedNamespaces::All, ClusterLoadState::Loading, None),
            "All namespaces"
        );
        assert_eq!(
            text(
                SelectedNamespaces::from(["default"]),
                ClusterLoadState::Loading,
                None
            ),
            "default",
            "a restored namespace is named before it is listed"
        );
        assert_eq!(
            text(SelectedNamespaces::default(), ClusterLoadState::Ready, None),
            "Select namespaces"
        );
    }

    #[test]
    fn the_scope_pill_counts_every_namespace_and_names_cluster_scoped_kinds() {
        assert_eq!(
            text(SelectedNamespaces::All, ClusterLoadState::Ready, None),
            "All namespaces · 0"
        );
        let nodes = crate::api_resource::ApiResource {
            group: "core".into(),
            version: "v1".into(),
            kind: "Node".into(),
            name: "nodes".into(),
            namespaced: false,
        };
        assert_eq!(
            text(
                SelectedNamespaces::from(["default"]),
                ClusterLoadState::Ready,
                Some(&nodes)
            ),
            "Cluster-wide"
        );
    }

    #[test]
    fn a_reconnecting_namespace_watch_is_reported_before_a_failed_list() {
        let mut cluster = ClusterState::for_test(1, "dev");
        assert_eq!(namespace_problem(&cluster), None);

        cluster.namespaces_load = ClusterLoadState::Failed("forbidden".into());
        assert_eq!(
            namespace_problem(&cluster),
            Some((
                "namespaces unavailable",
                "The namespace list could not be loaded: forbidden".to_owned()
            ))
        );

        cluster.namespaces_reconnecting = Some("connection lost".into());
        assert_eq!(
            namespace_problem(&cluster),
            Some((
                "namespaces reconnecting",
                "Namespace updates are paused: connection lost".to_owned()
            ))
        );
    }
}
