use super::*;
use crate::context_presentation::ContextPresentation;
use egui_kittest::kittest::NodeT;

const OPEN_CONTEXT_SETTINGS: &str =
    "Open context settings: Customize display names and selector badges.";
const PRODUCTION: &str = "cloudstorage-production-eu";

fn context_state(count: usize) -> UiState {
    let mut state = oracle_resource_table_state();
    state.clusters.retain(|key, _| *key == 2);
    let cluster = state.clusters.get_mut(&2).unwrap();
    cluster.name = PRODUCTION.into();
    cluster.presentation.display_name = Some("Cloudstorage Prod".into());
    let examples = [
        ("cloudstorage-staging-eu", "Cloudstorage Stage"),
        ("cloudstorage-local-dev", "Cloudstorage Local"),
        ("platform-production-eu", "Platform Prod"),
        ("platform-staging-eu", "Platform Stage"),
        ("analytics-production-eu", "Analytics Prod"),
        ("analytics-development-eu", "Analytics Dev"),
        ("sandbox", "Sandbox"),
        ("kind-kind", "kind-kind"),
        ("docker-desktop", "Docker Desktop"),
    ];
    for (index, (name, display_name)) in examples
        .into_iter()
        .take(count.saturating_sub(1))
        .enumerate()
    {
        let key = index as i32 + 3;
        let mut cluster = fixture_cluster(key, name);
        cluster.presentation.display_name = Some(display_name.into());
        state.clusters.insert(key, cluster);
    }
    state.next_cluster_key = 11;
    state
}

fn open_contexts(harness: &mut Harness<MyEguiApp<MockWorker>>) {
    open_settings(harness);
    let count = harness.state().ui_state.clusters.len();
    let position = harness
        .get_by_label(&format!("{OPEN_CONTEXT_SETTINGS} ({count} contexts)"))
        .rect()
        .center();
    primary_click(harness, position);
    harness.run();
}

fn open_context_editor(harness: &mut Harness<MyEguiApp<MockWorker>>, context_name: &str) {
    let position = harness
        .get_by_label(&format!("Edit context {context_name}"))
        .rect()
        .center();
    primary_click(harness, position);
    harness.run();
    harness.get_by_role_and_label(egui::accesskit::Role::TextInput, "Badge");
}

fn replace_context_field(harness: &mut Harness<MyEguiApp<MockWorker>>, field: &str, value: &str) {
    let position = harness
        .get_by_role_and_label(egui::accesskit::Role::TextInput, field)
        .rect()
        .center();
    primary_click(harness, position);
    harness.run();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    harness.event(egui::Event::Text(value.into()));
    harness.run();
}

fn save_context(harness: &mut Harness<MyEguiApp<MockWorker>>) {
    let position = harness.get_by_label("Save changes").rect().center();
    primary_click(harness, position);
    harness.run();
}

#[test]
fn context_settings_home_reserves_room_for_ten_context_count() {
    let mut harness = application_harness_with_state(context_state(10));
    open_settings(&mut harness);
    harness.get_by_label(&format!("{OPEN_CONTEXT_SETTINGS} (10 contexts)"));
    harness.ui_harness("settings/contexts/overview_ten_contexts");
}

#[test]
fn context_settings_list_preserves_spacious_rows() {
    let mut harness = application_harness_with_state(context_state(3));
    open_contexts(&mut harness);
    harness.get_by_label("3 contexts");
    harness.ui_harness("settings/contexts/list");
}

#[test]
fn context_settings_tenth_context_can_be_scrolled_into_view_and_edited() {
    let mut harness = application_harness_with_state(context_state(10));
    open_contexts(&mut harness);
    harness.get_by_label("10 contexts");
    // Scroll through the actual list rather than invoking an off-screen button.
    let first = harness
        .get_by_label(&format!("Edit context {PRODUCTION}"))
        .rect()
        .center();
    harness.event(egui::Event::PointerMoved(first));
    harness.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, -1200.0),
        phase: egui::TouchPhase::Move,
        modifiers: Default::default(),
    });
    harness.run();
    let last = harness.get_by_label("Edit context docker-desktop").rect();
    assert!(
        last.bottom() < 950.0,
        "last row should be within the list viewport: {last:?}"
    );
    open_context_editor(&mut harness, "docker-desktop");
    harness.get_by_label("Edit context");
    assert_eq!(harness.state().ui_state.selected_cluster, Some(2));
    harness.ui_harness("settings/contexts/tenth_context_editor");
}

#[test]
fn context_settings_editor_shows_live_custom_badge_preview() {
    let mut harness = application_harness_with_state(context_state(3));
    open_contexts(&mut harness);
    open_context_editor(&mut harness, PRODUCTION);
    replace_context_field(&mut harness, "Badge", "PR");
    harness.get_by_label("Automatic badge: CP");
    harness.ui_harness("settings/contexts/editor");
}

#[test]
fn context_editor_scrolls_only_its_form_when_needed() {
    let mut harness = application_harness_with_state(context_state(3));
    open_contexts(&mut harness);
    open_context_editor(&mut harness, PRODUCTION);
    let default_spacing = harness.ctx.global_style().spacing.item_spacing.y;
    for (height, item_spacing, form_should_scroll) in [
        (1024.0, default_spacing, false),
        (800.0, default_spacing, false),
        (1024.0, default_spacing * 3.0, false),
        (600.0, default_spacing, true),
    ] {
        harness
            .ctx
            .global_style_mut(|style| style.spacing.item_spacing.y = item_spacing);
        harness.set_size(egui::vec2(1200.0, height));
        harness.run();
        let editor = harness
            .queryable_node()
            .children()
            .find(|node| node.query_by_label("Edit context").is_some())
            .expect("editor blade should be present");
        let bars = editor
            .query_all_by_role(egui::accesskit::Role::ScrollBar)
            .map(|bar| bar.rect())
            .filter(|rect| rect.height() > rect.width())
            .collect::<Vec<_>>();
        assert!(
            bars.iter().all(|rect| rect.min.x < 1180.0),
            "Editor must not overflow the blade at height {height}: {bars:?}"
        );
        assert_eq!(
            bars.len(),
            usize::from(form_should_scroll),
            "height {height}: {bars:?}, save={:?}, badge={:?}",
            harness.get_by_label("Save changes").rect(),
            harness
                .get_by_role_and_label(egui::accesskit::Role::TextInput, "Badge")
                .rect()
        );
        let header = harness.get_by_label("Edit context").rect();
        let save = harness.get_by_label("Save changes").rect();
        let badge = harness
            .get_by_role_and_label(egui::accesskit::Role::TextInput, "Badge")
            .rect();
        harness.event(egui::Event::PointerMoved(egui::pos2(1000.0, 250.0)));
        harness.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -1200.0),
            phase: egui::TouchPhase::Move,
            modifiers: Default::default(),
        });
        harness.run();
        assert_eq!(harness.get_by_label("Edit context").rect(), header);
        assert_eq!(harness.get_by_label("Save changes").rect(), save);
        assert!(
            save.bottom() <= height - 32.0,
            "Footer must fit inside blade padding: {save:?}"
        );
        let scrolled_badge = harness
            .get_by_role_and_label(egui::accesskit::Role::TextInput, "Badge")
            .rect();
        if form_should_scroll {
            assert!(scrolled_badge.top() < badge.top());
        } else {
            assert_eq!(scrolled_badge, badge);
        }
    }
    harness.ui_harness("settings/contexts/editor_short_window");
}

#[test]
fn context_editor_preserves_keyboard_order_and_saves_the_latest_input() {
    let mut harness = application_harness_with_state(context_state(3));
    open_contexts(&mut harness);
    open_context_editor(&mut harness, PRODUCTION);
    replace_context_field(&mut harness, "Display name", "Production");
    harness.key_press(egui::Key::Tab);
    harness.run();
    assert!(
        harness
            .get_by_role_and_label(egui::accesskit::Role::TextInput, "Badge")
            .is_focused()
    );
    for label in ["Save changes", "Cancel"] {
        harness.key_press(egui::Key::Tab);
        harness.run();
        assert!(
            harness.get_by_label(label).is_focused(),
            "{label} should follow the form"
        );
    }
    let badge = harness
        .get_by_role_and_label(egui::accesskit::Role::TextInput, "Badge")
        .rect()
        .center();
    primary_click(&mut harness, badge);
    harness.run();
    // Typing and clicking Save can arrive in the same native frame.
    harness.event(egui::Event::Text("PR".into()));
    let save = harness.get_by_label("Save changes").rect().center();
    primary_click(&mut harness, save);
    harness.run();
    let command = harness
        .state()
        .worker
        .commands
        .iter()
        .find_map(|command| {
            command
                .as_ref()
                .as_any()
                .downcast_ref::<SaveContextPresentation>()
        })
        .expect("save command should be sent");
    assert_eq!(command.presentation.badge.as_deref(), Some("PR"));
    assert_eq!(
        command.presentation.display_name.as_deref(),
        Some("Production")
    );
}

#[test]
fn context_settings_editor_validation_and_save_failure_keep_the_draft() {
    let mut harness = application_harness_with_state(context_state(3));
    open_contexts(&mut harness);
    open_context_editor(&mut harness, PRODUCTION);
    replace_context_field(&mut harness, "Badge", "ABC");
    save_context(&mut harness);
    harness.get_by_label("Context settings need attention");
    assert!(
        !harness
            .state()
            .worker
            .commands
            .iter()
            .any(|command| command_is::<SaveContextPresentation>(command).is_some())
    );
    replace_context_field(&mut harness, "Display name", "Production Europe");
    replace_context_field(&mut harness, "Badge", "PR");
    save_context(&mut harness);
    let command = harness
        .state()
        .worker
        .commands
        .iter()
        .rev()
        .find_map(command_is::<SaveContextPresentation>)
        .unwrap();
    let request_id = command.request_id;
    assert_eq!(command.context_name, PRODUCTION);
    assert_eq!(command.presentation.badge.as_deref(), Some("PR"));
    harness.get_by_label("Saving…");
    // An older editor's completion cannot clear this editor's pending request.
    harness.deliver_worker_result(ContextPresentationSaved {
        request_id: uuid::Uuid::now_v7(),
        context_name: PRODUCTION.into(),
        presentation: ContextPresentation::default(),
    });
    harness.get_by_label("Saving…");
    harness.deliver_worker_result(ContextPresentationSaveFailed {
        request_id,
        context_name: PRODUCTION.into(),
        error: "Kubeconfig is read-only".into(),
    });
    assert_eq!(
        harness
            .get_by_role_and_label(egui::accesskit::Role::TextInput, "Display name")
            .value()
            .as_deref(),
        Some("Production Europe")
    );
    assert_eq!(
        harness.state().ui_state.clusters[&2]
            .presentation
            .display_name
            .as_deref(),
        Some("Cloudstorage Prod")
    );
    harness.ui_harness("settings/contexts/save_failed");
}

#[test]
fn context_settings_save_and_clear_preserve_the_active_workspace() {
    let mut harness = application_harness_with_state(context_state(3));
    let original_watch_count = harness.state().ui_state.clusters[&2].resource_cache.len();
    open_contexts(&mut harness);
    open_context_editor(&mut harness, PRODUCTION);
    replace_context_field(&mut harness, "Display name", "Production Europe");
    replace_context_field(&mut harness, "Badge", "PR");
    save_context(&mut harness);
    let command = harness
        .state()
        .worker
        .commands
        .iter()
        .rev()
        .find_map(command_is::<SaveContextPresentation>)
        .unwrap();
    let request_id = command.request_id;
    let presentation = command.presentation.clone();
    harness.deliver_worker_result(ContextPresentationSaveFailed {
        request_id,
        context_name: PRODUCTION.into(),
        error: "Temporary write failure".into(),
    });
    save_context(&mut harness);
    let command = harness
        .state()
        .worker
        .commands
        .iter()
        .rev()
        .find_map(command_is::<SaveContextPresentation>)
        .unwrap();
    assert_ne!(command.request_id, request_id);
    let previous_request_id = request_id;
    let request_id = command.request_id;
    harness.deliver_worker_result(ContextPresentationSaveFailed {
        request_id: previous_request_id,
        context_name: PRODUCTION.into(),
        error: "Stale failure".into(),
    });
    harness.get_by_label("Saving…");
    harness.deliver_worker_result(ContextPresentationSaved {
        request_id,
        context_name: PRODUCTION.into(),
        presentation,
    });
    harness.run();
    harness.get_by_label("Your contexts");
    assert_eq!(harness.state().ui_state.selected_cluster, Some(2));
    assert_eq!(harness.state().ui_state.clusters[&2].name, PRODUCTION);
    assert_eq!(
        harness.state().ui_state.clusters[&2].resource_cache.len(),
        original_watch_count
    );
    assert_eq!(
        harness.state().ui_state.clusters[&2]
            .presentation
            .badge(PRODUCTION),
        "PR"
    );
    harness.get_by_role_and_label(
        egui::accesskit::Role::Button,
        &format!("Production Europe ({PRODUCTION})"),
    );
    assert!(
        harness.get_all_by_label("Production Europe").count() >= 2,
        "updated name should appear in resource heading and context list"
    );
    open_context_editor(&mut harness, PRODUCTION);
    replace_context_field(&mut harness, "Display name", " ");
    replace_context_field(&mut harness, "Badge", " ");
    save_context(&mut harness);
    let command = harness
        .state()
        .worker
        .commands
        .iter()
        .rev()
        .find_map(command_is::<SaveContextPresentation>)
        .unwrap();
    assert_eq!(command.presentation, ContextPresentation::default());
    let request_id = command.request_id;
    harness.deliver_worker_result(ContextPresentationSaved {
        request_id,
        context_name: PRODUCTION.into(),
        presentation: ContextPresentation::default(),
    });
    harness.run();
    assert_eq!(
        harness.state().ui_state.clusters[&2]
            .presentation
            .badge(PRODUCTION),
        "CP"
    );
    assert!(
        !harness
            .state()
            .worker
            .commands
            .iter()
            .any(|command| command_is::<ConnectToCluster>(command).is_some())
    );
}

#[test]
fn context_settings_cancel_discards_edits() {
    let mut harness = application_harness_with_state(context_state(3));
    open_contexts(&mut harness);
    open_context_editor(&mut harness, PRODUCTION);
    replace_context_field(&mut harness, "Badge", "XX");
    let position = harness.get_by_label("Cancel").rect().center();
    primary_click(&mut harness, position);
    harness.run();
    harness.get_by_label("Your contexts");
    open_context_editor(&mut harness, PRODUCTION);
    assert_eq!(
        harness
            .get_by_role_and_label(egui::accesskit::Role::TextInput, "Badge")
            .value()
            .as_deref(),
        Some("")
    );
    assert!(
        !harness
            .state()
            .worker
            .commands
            .iter()
            .any(|command| command_is::<SaveContextPresentation>(command).is_some())
    );
}

#[test]
fn context_settings_list_scrolls_at_a_shorter_window_height() {
    let mut harness = application_harness_with_state(context_state(10));
    harness.set_size(egui::vec2(1200.0, 800.0));
    harness.run();
    open_contexts(&mut harness);
    assert!(
        !harness
            .query_all_by_role(egui::accesskit::Role::ScrollBar)
            .any(|bar| bar.rect().min.x >= 1180.0),
        "The blade should fit the window while its context list scrolls"
    );
    let position = harness
        .get_by_label(&format!("Edit context {PRODUCTION}"))
        .rect()
        .center();
    harness.event(egui::Event::PointerMoved(position));
    harness.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, -1600.0),
        phase: egui::TouchPhase::Move,
        modifiers: Default::default(),
    });
    harness.run();
    harness.get_by_label("10 contexts");
    harness.ui_harness("settings/contexts/scrolled_list_short_window");
    open_context_editor(&mut harness, "docker-desktop");
}

#[test]
fn context_settings_reopening_a_pending_save_uses_the_submitted_values() {
    let mut harness = application_harness_with_state(context_state(3));
    open_contexts(&mut harness);
    open_context_editor(&mut harness, PRODUCTION);
    replace_context_field(&mut harness, "Display name", "Production Europe");
    replace_context_field(&mut harness, "Badge", "PR");
    // Two clicks queued at the same position still submit only one save.
    let position = harness.get_by_label("Save changes").rect().center();
    primary_click(&mut harness, position);
    primary_click(&mut harness, position);
    harness.run();
    assert_eq!(
        harness
            .state()
            .worker
            .commands
            .iter()
            .filter(|c| command_is::<SaveContextPresentation>(c).is_some())
            .count(),
        1
    );
    let command = harness
        .state()
        .worker
        .commands
        .iter()
        .find_map(command_is::<SaveContextPresentation>)
        .unwrap();
    let request_id = command.request_id;
    let presentation = command.presentation.clone();
    // Header navigation remains available while a write is pending.
    let back = harness.get_by_label("Back").rect().center();
    primary_click(&mut harness, back);
    harness.run();
    open_context_editor(&mut harness, PRODUCTION);
    assert_eq!(
        harness
            .get_by_role_and_label(egui::accesskit::Role::TextInput, "Display name")
            .value()
            .as_deref(),
        Some("Production Europe")
    );
    assert_eq!(
        harness
            .get_by_role_and_label(egui::accesskit::Role::TextInput, "Badge")
            .value()
            .as_deref(),
        Some("PR")
    );
    harness.deliver_worker_result(ContextPresentationSaved {
        request_id,
        context_name: PRODUCTION.into(),
        presentation,
    });
    harness.run();
    harness.get_by_label("Your contexts");
    open_context_editor(&mut harness, PRODUCTION);
    assert_eq!(
        harness
            .get_by_role_and_label(egui::accesskit::Role::TextInput, "Display name")
            .value()
            .as_deref(),
        Some("Production Europe")
    );
    assert_eq!(
        harness
            .get_by_role_and_label(egui::accesskit::Role::TextInput, "Badge")
            .value()
            .as_deref(),
        Some("PR")
    );
}
