use super::*;
use egui_kittest::Harness;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

fn resource(name: &str) -> MinimalResource {
    MinimalResource {
        uid: name.into(),
        name: name.into(),
        namespace: Some("default".into()),
        creation_timestamp: None,
        controller_owner: None,
        labels: Default::default(),
        annotations: Default::default(),
        cells: Default::default(),
        log_containers: Vec::new(),
    }
}

#[test]
fn fuzzy_search_matches_normalized_resource_names() {
    let resources = vec![resource("Café-API"), resource("worker")];
    let filtered = filter_resources(
        &resources,
        &ResourceSearchState {
            query: "cfa".into(),
            regex_mode: false,
        },
    );

    assert_eq!(
        filtered
            .resources
            .iter()
            .map(|resource| resource.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Café-API"]
    );
    assert!(filtered.regex_error.is_none());
}

#[test]
fn fuzzy_search_orders_resources_by_match_quality() {
    let resources = vec![
        resource("my-api"),
        resource("a-p-i"),
        resource("api-server"),
        resource("api"),
    ];
    let filtered = filter_resources(
        &resources,
        &ResourceSearchState {
            query: "api".into(),
            regex_mode: false,
        },
    );

    assert_eq!(
        filtered
            .resources
            .iter()
            .map(|resource| resource.name.as_str())
            .collect::<Vec<_>>(),
        ["api", "api-server", "my-api", "a-p-i"]
    );
}

#[test]
fn fuzzy_search_preserves_source_order_for_equal_or_empty_normalized_matches() {
    let resources = vec![resource("z-api"), resource("a-api")];
    let fuzzy = filter_resources(
        &resources,
        &ResourceSearchState {
            query: "api".into(),
            regex_mode: false,
        },
    );
    let normalized_empty = filter_resources(
        &resources,
        &ResourceSearchState {
            query: "\u{301}".into(),
            regex_mode: false,
        },
    );
    let regex = filter_resources(
        &resources,
        &ResourceSearchState {
            query: "api".into(),
            regex_mode: true,
        },
    );

    for filtered in [&fuzzy, &normalized_empty, &regex] {
        assert_eq!(
            filtered
                .resources
                .iter()
                .map(|resource| resource.name.as_str())
                .collect::<Vec<_>>(),
            ["z-api", "a-api"]
        );
    }
    assert!(normalized_empty.fuzzy_scores.is_none());
    assert!(regex.fuzzy_scores.is_none());
}

#[test]
fn resource_column_sort_uses_fuzzy_relevance_before_name_tie_breaking() {
    let stronger_match = resource("z-api");
    let weaker_match = resource("a-p-i");
    let query = components::fuzzy::normalize_for_search("api").collect::<Vec<_>>();
    let scores = std::collections::HashMap::from([
        (
            stronger_match.uid.clone(),
            components::fuzzy::fuzzy_match_score(&stronger_match.name, &query)
                .expect("resource should match"),
        ),
        (
            weaker_match.uid.clone(),
            components::fuzzy::fuzzy_match_score(&weaker_match.name, &query)
                .expect("resource should match"),
        ),
    ]);

    assert_eq!(
        compare_resource_column_with_relevance(
            &stronger_match,
            &weaker_match,
            "owner",
            components::SortDirection::Ascending,
            &[],
            Some(&scores),
        ),
        std::cmp::Ordering::Less,
    );
    assert_eq!(
        compare_resource_column_with_relevance(
            &stronger_match,
            &weaker_match,
            "owner",
            components::SortDirection::Ascending,
            &[],
            None,
        ),
        std::cmp::Ordering::Greater,
    );
}

#[test]
fn regex_search_matches_normalized_resource_names_case_insensitively() {
    let resources = vec![resource("Café-API"), resource("worker")];
    let filtered = filter_resources(
        &resources,
        &ResourceSearchState {
            query: "CAFE-.*".into(),
            regex_mode: true,
        },
    );

    assert_eq!(filtered.resources.len(), 1);
    assert_eq!(filtered.resources[0].name, "Café-API");
    assert!(filtered.regex_error.is_none());
}

#[test]
fn invalid_regex_has_no_results_and_an_error() {
    let filtered = filter_resources(
        &[resource("pod")],
        &ResourceSearchState {
            query: "[".into(),
            regex_mode: true,
        },
    );

    assert!(filtered.resources.is_empty());
    assert!(
        filtered
            .regex_error
            .as_deref()
            .is_some_and(|error| error.starts_with("regex parse error:"))
    );
}

#[test]
fn metadata_suggestions_are_sorted_and_cover_labels_and_annotations() {
    let mut first = resource("first");
    first.labels = BTreeMap::from([("app".into(), "api".into())]);
    first.annotations = BTreeMap::from([("example.com/team".into(), "platform".into())]);
    let mut second = resource("second");
    second.labels = BTreeMap::from([
        ("app".into(), "worker".into()),
        ("tier".into(), "backend".into()),
    ]);
    second.annotations = BTreeMap::from([("example.com/owner".into(), "ops".into())]);

    let suggestions = metadata_key_suggestions(&[first, second]);

    assert_eq!(suggestions.labels, ["app", "tier"]);
    assert_eq!(
        suggestions.annotations,
        ["example.com/owner", "example.com/team"]
    );
}

#[test]
fn custom_metadata_columns_render_and_sort_by_metadata_values() {
    let mut api = resource("api");
    api.labels.insert("app".into(), "api".into());
    let worker = resource("worker");
    let column = super::super::table_preferences::CustomMetadataColumn {
        source: MetadataColumnSource::Label,
        key: "app".into(),
        label: "Application".into(),
    };

    assert_eq!(
        resource_metadata_value(&api, column.source, &column.key),
        Some("api")
    );
    assert_eq!(
        resource_metadata_value(&worker, column.source, &column.key),
        None
    );
    assert_eq!(
        compare_resource_column(
            &api,
            &worker,
            &column.id(),
            components::SortDirection::Ascending,
            std::slice::from_ref(&column),
        ),
        std::cmp::Ordering::Less
    );

    let annotation_column = super::super::table_preferences::CustomMetadataColumn {
        source: MetadataColumnSource::Annotation,
        key: "example.com/team".into(),
        label: "Team".into(),
    };
    api.annotations
        .insert("example.com/team".into(), "platform".into());
    assert_eq!(
        resource_metadata_value(&api, annotation_column.source, &annotation_column.key),
        Some("platform")
    );
}

#[test]
fn resource_count_includes_the_total_while_searching() {
    assert_eq!(resource_count_label(8, 1, true), "1 of 8 items");
    assert_eq!(resource_count_label(8, 8, false), "8 items");
}

#[test]
fn command_f_focuses_resource_search_input() {
    let search = Rc::new(RefCell::new(ResourceSearchState::default()));
    let search_for_ui = search.clone();
    let mut harness = Harness::builder().build_ui(move |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            show_resource_search(ui, &mut search_for_ui.borrow_mut());
        });
    });
    components::test_support::setup_egui(&mut harness);
    harness.run();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::F);
    harness.run();
    harness.event(egui::Event::Text("worker".into()));
    harness.run();

    assert_eq!(search.borrow().query, "worker");
}
