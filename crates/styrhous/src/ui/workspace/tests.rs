use super::*;
use egui_kittest::Harness;
use std::cell::RefCell;
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
}

#[test]
fn invalid_regex_has_no_results_and_an_error() {
    let search = ResourceSearchState {
        query: "[".into(),
        regex_mode: true,
    };
    let filtered = filter_resources(&[resource("pod")], &search);

    assert!(filtered.resources.is_empty());
    assert!(
        regex_error(&search)
            .as_deref()
            .is_some_and(|error| error.starts_with("Invalid regular expression:"))
    );
}

#[test]
fn resource_count_includes_the_total_while_searching() {
    assert_eq!(
        resource_count_label(8, 1, true, ("pod", "pods")),
        "1 of 8 pods"
    );
    assert_eq!(resource_count_label(8, 8, false, ("pod", "pods")), "8 pods");
    assert_eq!(resource_count_label(1, 1, false, ("pod", "pods")), "1 pod");
    assert_eq!(
        resource_count_label(1, 0, true, ("pod", "pods")),
        "0 of 1 pod"
    );
    assert_eq!(
        resource_count_label(12_400, 1_234, true, ("pod", "pods")),
        "1,234 of 12.4k pods"
    );
}

#[test]
fn command_f_focuses_resource_search_input() {
    let search = Rc::new(RefCell::new(ResourceSearchState::default()));
    let search_for_ui = search.clone();
    let mut harness = Harness::builder().build_ui(move |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            show_resource_search(ui, &mut search_for_ui.borrow_mut(), "pods", 300.0);
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

#[test]
fn a_single_resource_is_counted_with_the_singular_of_its_plural() {
    for (kind, plural, singular) in [
        ("Pod", "pods", "pod"),
        ("ConfigMap", "configmaps", "configmap"),
        ("ComponentStatus", "componentstatuses", "componentstatus"),
        ("Prometheus", "prometheuses", "prometheus"),
        ("Endpoints", "endpoints", "endpoints"),
        // The kind does not start the plural: English rules undo it.
        ("HelmRelease", "releases", "release"),
        ("NetworkPolicy", "networkpolicies", "networkpolicy"),
        ("Unrelated", "ingresses", "ingress"),
        ("Unrelated", "storageclasses", "storageclass"),
        ("Unrelated", "egress", "egress"),
    ] {
        assert_eq!(singular_of(kind, plural), singular, "{plural}");
    }
}

#[test]
fn the_release_list_is_live_once_every_chosen_namespace_has_synchronised() {
    use crate::ui::state::{ClusterState, HelmReleaseWatchState};
    let synced = |is_synced| HelmReleaseWatchState {
        is_synced,
        ..HelmReleaseWatchState::default()
    };
    let mut cluster = ClusterState::for_test(1, "dev");
    cluster.selected_namespaces = SelectedNamespaces::default();
    assert!(!helm_list_is_live(&cluster), "no namespace is chosen");

    cluster.selected_namespaces = SelectedNamespaces::from(["default", "web"]);
    cluster
        .helm_release_cache
        .insert("default".into(), synced(true));
    assert!(!helm_list_is_live(&cluster), "web has no release watch yet");

    cluster
        .helm_release_cache
        .insert("web".into(), synced(false));
    assert!(!helm_list_is_live(&cluster), "web has not synchronised");

    cluster
        .helm_release_cache
        .insert("web".into(), synced(true));
    assert!(helm_list_is_live(&cluster));
}
