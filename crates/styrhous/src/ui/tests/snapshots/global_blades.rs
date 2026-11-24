//! Global-blade ownership and settings scenarios.

use super::*;
use crate::licensing::{FREE_USE_NOTE, FREE_USE_SIGN_IN_NOTE};
use crate::ui::state::OperationOutcome;

#[test]
fn notification_history_blade_shows_session_outcomes_and_marks_them_read() {
    let mut harness = application_harness::<MockWorker>();
    let mut state = oracle_resource_table_state();
    state.record_operation_outcome(
        2,
        OperationOutcome::Success,
        "Deployment scaled to 4 replicas",
        Some("kube-system"),
        "coredns",
        None,
    );
    state.record_operation_outcome(
        2,
        OperationOutcome::Failure,
        "Bulk delete finished with 1 failure",
        None,
        "3 Pods",
        Some("2 deleted, 1 failed".into()),
    );
    state.record_operation_outcome(
        2,
        OperationOutcome::Success,
        "CronJob started",
        Some("kube-system"),
        "nightly-report",
        Some("Created Job nightly-report-manual-x7k2p".into()),
    );
    state.record_operation_outcome(
        2,
        OperationOutcome::Failure,
        "Couldn’t run CronJob",
        Some("kube-system"),
        "nightly-report",
        Some("Forbidden: cannot create jobs".into()),
    );
    harness.state_mut().ui_state = state;
    harness.run();

    assert_eq!(harness.state().ui_state.unread_operation_count(), 4);
    assert_eq!(
        harness.state().ui_state.clusters[&2]
            .transient_operation_toasts
            .len(),
        4
    );
    harness.ui_harness("notifications/notification_history_unread_badge");
    let notifications_position = harness.get_by_label("Notifications").rect().center();
    primary_click(&mut harness, notifications_position);
    harness.run_steps(2);

    harness.get_by_label("Outcomes of actions from this session.");
    harness.get_by_label("Couldn’t run CronJob");
    // Opening the history clears the toasts but leaves the entries unread.
    assert_eq!(harness.state().ui_state.unread_operation_count(), 4);
    assert!(
        harness
            .state()
            .ui_state
            .clusters
            .values()
            .all(|cluster| cluster.transient_operation_toasts.is_empty())
    );
    harness.ui_harness("notifications/notification_history_blade_shows_unread_outcomes");

    click_label(&mut harness, "Mark all read");
    assert_eq!(harness.state().ui_state.unread_operation_count(), 0);
    harness.get_by_label("Couldn’t run CronJob");
    assert!(
        harness
            .get_by_label("Mark all read")
            .accesskit_node()
            .is_disabled(),
        "with nothing left to read, there is nothing to mark"
    );
    harness.ui_harness(
        "notifications/notification_history_blade_shows_session_outcomes_and_marks_them_read",
    );
}

#[test]
fn partial_api_discovery_shows_a_toast_naming_the_failed_groups() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    harness.run();
    harness
        .state_mut()
        .worker
        .results
        .push_back(Box::new(DiscoveryFinished {
            cluster_key: 2,
            // Discovery still reports the Pods the fixture shows.
            api_resources: vec![
                fixture_api_resource("core", "Pod", "pods"),
                fixture_api_resource("apps", "Deployment", "deployments"),
            ],
            failed_api_groups: vec![
                FailedApiGroup {
                    group_version: "metrics.k8s.io/v1beta1".into(),
                    error: "the server is currently unable to handle the request".into(),
                },
                FailedApiGroup {
                    group_version: "custom.metrics.k8s.io/v1beta1".into(),
                    error: "timed out waiting for the API server to respond".into(),
                },
            ],
        }) as WorkerResultBox);
    harness.run();

    harness.get_by_label("2 API groups couldn’t be loaded");
    harness.ui_harness("notifications/partial_api_discovery_toast");
}

/// Applies the queued results, then keeps their toasts from expiring on a slow machine.
fn run_with_pinned_toasts(harness: &mut Harness<'static, MyEguiApp<MockWorker>>) {
    harness.step();
    for cluster in harness.state_mut().ui_state.clusters.values_mut() {
        for toast in &mut cluster.transient_operation_toasts {
            toast.expires_at = std::time::Instant::now() + std::time::Duration::from_secs(3600);
        }
    }
    harness.run();
}

#[test]
fn recovered_api_groups_appear_in_the_sidebar_with_a_success_toast() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    harness.run();
    harness
        .state_mut()
        .worker
        .results
        .push_back(Box::new(DiscoveryFinished {
            cluster_key: 2,
            // Discovery still reports the Pods the fixture shows.
            api_resources: vec![
                fixture_api_resource("core", "Pod", "pods"),
                fixture_api_resource("apps", "Deployment", "deployments"),
            ],
            failed_api_groups: vec![FailedApiGroup {
                group_version: "batch/v1".into(),
                error: "the server is currently unable to handle the request".into(),
            }],
        }) as WorkerResultBox);
    run_with_pinned_toasts(&mut harness);
    harness.get_by_label("Workloads").click();
    harness.run();
    harness.get_by_label("1 API group couldn’t be loaded");
    assert!(harness.query_by_label("Jobs").is_none());
    harness.ui_harness("notifications/api_group_recovery_before");

    harness
        .state_mut()
        .worker
        .results
        .push_back(Box::new(KubernetesApiGroupsRecovered {
            cluster_key: 2,
            group_versions: vec!["batch/v1".into()],
            resources: ApiResourceSet {
                api_resources: vec![fixture_api_resource("batch", "Job", "jobs")],
                scalable_api_resources: Default::default(),
            },
            still_failing_group_versions: Vec::new(),
        }) as WorkerResultBox);
    run_with_pinned_toasts(&mut harness);
    // The bottom-anchored toast stack is positioned from the previous frame's size.
    harness.step();

    harness.get_by_label("1 API group loaded");
    harness.get_by_label("Jobs");
    harness.ui_harness("notifications/api_group_recovery_after");
}

#[test]
fn partial_api_discovery_toast_summarises_many_failed_groups() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    harness.run();
    harness
        .state_mut()
        .worker
        .results
        .push_back(Box::new(DiscoveryFinished {
            cluster_key: 2,
            // Discovery still reports the Pods the fixture shows.
            api_resources: vec![
                fixture_api_resource("core", "Pod", "pods"),
                fixture_api_resource("apps", "Deployment", "deployments"),
            ],
            failed_api_groups: [
                "metrics.k8s.io/v1beta1",
                "custom.metrics.k8s.io/v1beta1",
                "external.metrics.k8s.io/v1beta1",
                "webhook.example.com/v1",
                "stuck.example.com/v1",
            ]
            .into_iter()
            .map(|group_version| FailedApiGroup {
                group_version: group_version.into(),
                error: "the server is currently unable to handle the request".into(),
            })
            .collect(),
        }) as WorkerResultBox);
    harness.run();

    harness.get_by_label("5 API groups couldn’t be loaded");
    harness.ui_harness("notifications/partial_api_discovery_toast_many_groups");
}

#[test]
fn notification_history_includes_every_cluster_and_marks_all_entries_read() {
    let mut harness = application_harness::<MockWorker>();
    let mut state = oracle_resource_table_state();
    state.clusters.get_mut(&1).unwrap().connection = ClusterConnectionState::Connected;
    state.record_operation_outcome(
        1,
        OperationOutcome::Success,
        "Deleted Pod",
        Some("default"),
        "api",
        None,
    );
    state.record_operation_outcome(
        2,
        OperationOutcome::Failure,
        "Couldn’t scale Deployment",
        Some("kube-system"),
        "coredns",
        Some("Forbidden".into()),
    );
    harness.state_mut().ui_state = state;
    harness.run();
    assert_eq!(harness.state().ui_state.unread_operation_count(), 2);

    let notifications_position = harness.get_by_label("Notifications").rect().center();
    primary_click(&mut harness, notifications_position);
    harness.run_steps(2);

    let entries = harness.state().ui_state.operation_history_entries();
    assert_eq!(entries.len(), 2);
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.cluster_name.as_str())
            .collect::<Vec<_>>(),
        vec!["kind-kind", "dev"]
    );
    // Looking at the history does not read it; "Mark all read" does.
    assert_eq!(harness.state().ui_state.unread_operation_count(), 2);
    click_label(&mut harness, "Mark all read");
    assert_eq!(harness.state().ui_state.unread_operation_count(), 0);
}

#[test]
fn licensing_warning_opens_account_settings_without_restricting_the_application() {
    let mut harness = licensing_harness(LicenseStatus::Revoked);

    harness.get_by_label("This device's license was revoked.");
    harness.get_by_label("More actions for kube-scheduler-kind-control-plane");
    harness.ui_harness(
        "licensing/licensing_warning_opens_account_settings_without_restricting_the_application/warning",
    );

    open_license_settings(&mut harness, LicenseEntry::Warning);

    harness.get_by_label(FREE_USE_SIGN_IN_NOTE);
    harness.get_by_label("Sign in");
    harness.get_by_label("Sign out");
    assert!(harness.query_by_label("Refresh license").is_none());
    assert!(harness.query_by_label("Self-hosted server").is_none());
    harness.ui_harness(
        "licensing/licensing_warning_opens_account_settings_without_restricting_the_application/settings",
    );

    harness.get_by_label("License device display name").click();
    harness.run();
    harness
        .get_by_label("License device display name")
        .type_text(&"x".repeat(121));
    harness.run();
    harness.get_by_label("Save name").click();
    harness.run();
    harness.get_by_label("Device name must contain 1 to 120 characters.");
    assert!(harness.query_by_label("Self-hosted server").is_none());
    harness.get_by_label("Advanced").click();
    harness.run();
    harness.get_by_label("Self-hosted server");

    // Signing out dismisses the warning and returns to quiet free use.
    harness.get_by_label("Sign out").click();
    harness.run();
    harness.get_by_label("Not signed in.");
    assert!(
        harness
            .query_by_role_and_label(egui::accesskit::Role::Link, LICENSE_WARNING)
            .is_none()
    );
}

#[test]
fn signed_out_free_use_shows_no_warning() {
    let mut harness = licensing_harness(LicenseStatus::SignedOut);

    open_license_settings(
        &mut harness,
        LicenseEntry::Settings {
            card_detail: "Not signed in",
        },
    );

    harness.get_by_label("Not signed in.");
    harness.get_by_label(FREE_USE_SIGN_IN_NOTE);
    harness.get_by_label("Sign in");
    harness.get_by_label("Manage account");
    assert!(harness.query_by_label("Refresh license").is_none());
    assert!(harness.query_by_label("Sign out").is_none());
    harness.ui_harness("licensing/signed_out_free_use_account_settings");
}

#[test]
fn signed_in_without_entitlement_keeps_account_actions_available() {
    // Users without a subscription most likely qualify for free use, so only problems with a
    // commercial subscription raise the warning.
    for (reason, entry) in [
        (
            "trial_expired",
            LicenseEntry::Settings {
                card_detail: "No subscription",
            },
        ),
        (
            "no_valid_entitlement",
            LicenseEntry::Settings {
                card_detail: "No subscription",
            },
        ),
        ("offline_lease_expired", LicenseEntry::Warning),
        ("subscription_expired", LicenseEntry::Warning),
    ] {
        let mut harness = licensing_harness(LicenseStatus::Evaluation {
            reason_code: reason.into(),
        });
        open_license_settings(&mut harness, entry);
        harness.get_by_label(FREE_USE_NOTE);
        assert!(harness.query_by_label("Sign in").is_none());
        harness.get_by_label("Refresh license");
        harness.get_by_label("Sign out");
        if reason == "trial_expired" {
            harness.get_by_label("Your organization's trial has ended.");
            harness.ui_harness("licensing/trial_ended_account_actions");
        }
        harness.get_by_label("Sign out").click();
        harness.run_steps(2);
        harness.get_by_label("Sign in");
        assert!(harness.query_by_label("Sign out").is_none());
    }
}

/// The license warning banner's action.
const LICENSE_WARNING: &str = "Manage license";

/// How a test reaches the license blade. A status that warns must be reachable from the warning
/// banner; any other status must show no banner and is opened from the settings home, whose
/// license card shows `card_detail`.
enum LicenseEntry {
    Warning,
    Settings { card_detail: &'static str },
}

fn open_license_settings(harness: &mut Harness<MyEguiApp<MockWorker>>, entry: LicenseEntry) {
    match entry {
        LicenseEntry::Warning => harness
            .get_by_role_and_label(egui::accesskit::Role::Link, LICENSE_WARNING)
            .click(),
        LicenseEntry::Settings { card_detail } => {
            assert!(
                harness
                    .query_by_role_and_label(egui::accesskit::Role::Link, LICENSE_WARNING)
                    .is_none(),
                "unexpected license warning"
            );
            open_settings(harness);
            let card = format!(
                "Open license and account settings: Custom compatible server ({card_detail})"
            );
            let license_position = harness.get_by_label(&card).rect().center();
            primary_click(harness, license_position);
        }
    }
    harness.run();
    harness.get_by_label("License device display name");
}

/// A lease far in the future, so snapshot text never depends on the current time.
pub(super) fn fixed_lease(state: &str, reason_code: &str) -> VerifiedLease {
    VerifiedLease {
        user_id: uuid::Uuid::from_u128(1),
        seat_id: uuid::Uuid::from_u128(2),
        billing_account_id: uuid::Uuid::from_u128(3),
        installation_id: uuid::Uuid::from_u128(4),
        activation_id: uuid::Uuid::from_u128(5),
        state: state.into(),
        reason_code: reason_code.into(),
        expires_at: 4_102_444_800,
        refresh_after: 4_102_358_400,
    }
}

fn licensing_harness(status: LicenseStatus) -> Harness<'static, MyEguiApp<MockWorker>> {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    harness
        .state_mut()
        .licensing
        .switch_server(LicenseServer::Custom("http://127.0.0.1:5050".into()));
    harness.state_mut().licensing.set_status_for_test(status);
    harness.run();
    harness
}

#[test]
fn licensed_commercial_account_actions_show_license_details() {
    let mut harness = licensing_harness(LicenseStatus::Licensed(fixed_lease(
        "commercial",
        "active_subscription",
    )));
    open_license_settings(
        &mut harness,
        LicenseEntry::Settings {
            card_detail: "Active",
        },
    );

    harness.get_by_label("Commercial");
    assert!(harness.query_by_label(FREE_USE_NOTE).is_none());
    harness.get_by_label("1 Jan 2100");
    assert!(harness.query_by_label("Sign in").is_none());
    harness.get_by_label("Refresh license");
    harness.get_by_label("Manage account");
    harness.get_by_label("Sign out");
    harness.get_by_label("Advanced").click();
    harness.run();
    harness.get_by_label("License details: commercial (active_subscription)");
    harness.ui_harness("licensing/licensed_commercial_account_actions");
}

#[test]
fn grace_lease_warns_but_keeps_the_license_details() {
    let mut harness = licensing_harness(LicenseStatus::Licensed(fixed_lease(
        "grace",
        "subscription_past_due",
    )));
    harness.get_by_label("License grace period active until 1 Jan 2100.");
    open_license_settings(&mut harness, LicenseEntry::Warning);

    harness.get_by_label("1 Jan 2100");
    assert!(
        harness
            .query_by_label_contains("Styrhous is free")
            .is_none()
    );
    assert!(harness.query_by_label("Sign in").is_none());
    harness.get_by_label("Refresh license");
    harness.get_by_label("Sign out");
    harness.ui_harness("licensing/grace_lease_account_actions");
}

#[test]
fn offline_lease_account_actions_warn_about_session_only_credentials() {
    let mut harness =
        licensing_harness(LicenseStatus::Offline(fixed_lease("trial", "trial_active")));
    harness
        .state_mut()
        .licensing
        .set_keychain_warning_for_test(true);
    harness.run();
    // A still-valid offline lease is not a problem, so only the blade shows the summary.
    open_license_settings(
        &mut harness,
        LicenseEntry::Settings {
            card_detail: "Offline",
        },
    );

    harness.get_by_label(
        "Licensing service is unreachable; offline trial lease is valid until 1 Jan 2100.",
    );
    harness.get_by_label("Secure credential storage is unavailable");
    harness.get_by_label("This sign-in lasts only for this session.");
    assert!(harness.query_by_label("Sign in").is_none());
    harness.get_by_label("Refresh license");
    harness.get_by_label("Sign out");
    harness.ui_harness("licensing/offline_lease_account_actions");
}

#[test]
fn failed_check_offers_every_account_action() {
    let mut harness = licensing_harness(LicenseStatus::Failed {
        message: "the licensing service is unreachable".into(),
    });
    open_license_settings(&mut harness, LicenseEntry::Warning);

    for action in ["Sign in", "Refresh license", "Manage account", "Sign out"] {
        harness.get_by_label(action);
    }
    harness.ui_harness("licensing/failed_check_account_actions");
}

#[test]
fn awaiting_approval_shows_the_code_and_can_be_cancelled() {
    let mut harness = licensing_harness(LicenseStatus::AwaitingApproval(DeviceAuthorization {
        user_code: "BCDF-GHJK".into(),
        verification_uri: "http://127.0.0.1:5050/devices/authorize".into(),
        verification_uri_complete: "http://127.0.0.1:5050/devices/authorize?user_code=BCDF-GHJK"
            .into(),
        expires_in: 600,
    }));
    open_license_settings(
        &mut harness,
        LicenseEntry::Settings {
            card_detail: "Pending",
        },
    );

    harness.get_by_label("BCDF-GHJK");
    harness.get_by_label("This request expires in 10 minutes.");
    harness.get_by_label("Open approval page");
    harness.get_by_label("Copy code");
    harness.get_by_label("Approval server: http://127.0.0.1:5050/devices/authorize");
    assert!(harness.query_by_label("Sign in").is_none());
    harness.ui_harness("licensing/awaiting_approval_account_actions");

    harness.get_by_label("Cancel sign-in").click();
    harness.run_steps(2);
    harness.get_by_label("Sign in");
    assert!(harness.query_by_label("Cancel sign-in").is_none());
}

#[test]
fn checking_offers_only_account_management_and_sign_out() {
    let mut harness = licensing_harness(LicenseStatus::Checking);
    open_license_settings(
        &mut harness,
        LicenseEntry::Settings {
            card_detail: "Checking…",
        },
    );

    harness.get_by_label("Manage account");
    harness.get_by_label("Sign out");
    assert!(harness.query_by_label("Sign in").is_none());
    assert!(harness.query_by_label("Refresh license").is_none());
}

#[test]
fn unconfigured_licensing_requires_a_server_before_sign_in() {
    let mut harness = application_harness::<MockWorker>();
    harness
        .state_mut()
        .licensing
        .switch_server(LicenseServer::Custom(String::new()));
    harness.run();

    open_license_settings(
        &mut harness,
        LicenseEntry::Settings {
            card_detail: "Not signed in",
        },
    );

    harness.get_by_label("No licensing server is configured.");
    harness.get_by_label(FREE_USE_SIGN_IN_NOTE);
    assert!(harness.query_by_label("Sign in").is_none());
    assert!(harness.query_by_label("Refresh license").is_none());
    assert!(harness.query_by_label("Manage account").is_none());
}

#[test]
fn namespace_selector_settings_show_templates_and_the_open_field_editor() {
    let mut harness = application_harness::<MockWorker>();
    let mut state = oracle_resource_table_state();
    state
        .clusters
        .get_mut(&2)
        .expect("kind fixture exists")
        .namespaces
        .insert(
            "acme-corp-a1b2c3d4".into(),
            MinimalNamespace {
                name: "acme-corp-a1b2c3d4".into(),
                labels: std::collections::BTreeMap::from([(
                    "company.example/customer".into(),
                    "Acme Corp".into(),
                )]),
                annotations: std::collections::BTreeMap::from([(
                    "company.example/environment".into(),
                    "Production".into(),
                )]),
            },
        );
    harness.state_mut().ui_state = state;
    harness.state_mut().namespace_selector_settings =
        crate::ui::namespace_selector::NamespaceSelectorSettings {
            fields: vec![
                crate::ui::namespace_selector::NamespaceMetadataField {
                    alias: "customer".into(),
                    source: crate::ui::table_preferences::MetadataColumnSource::Label,
                    key: "company.example/customer".into(),
                },
                crate::ui::namespace_selector::NamespaceMetadataField {
                    alias: "environment".into(),
                    source: crate::ui::table_preferences::MetadataColumnSource::Annotation,
                    key: "company.example/environment".into(),
                },
            ],
            templates: vec![
                crate::ui::namespace_selector::NamespaceIdentityTemplate {
                    template: "Customer: {{customer}} · Environment: {{environment}}".into(),
                },
                crate::ui::namespace_selector::NamespaceIdentityTemplate {
                    template: "Customer: {{customer}}".into(),
                },
            ],
        };
    harness.run();

    let settings_position = harness.get_by_label("Settings").rect().center();
    primary_click(&mut harness, settings_position);
    harness.run();
    let namespace_settings_position = harness
        .get_by_label(OPEN_NAMESPACE_SELECTOR_SETTINGS)
        .rect()
        .center();
    primary_click(&mut harness, namespace_settings_position);
    harness.run();
    harness.get_by_label("Namespace selector");
    harness.get_by_label("Customer: Acme Corp · Environment: Production");
    harness.ui_harness(
        "settings/namespace_selector_settings_show_templates_and_the_open_field_editor/settings",
    );

    harness.get_by_label("Edit customer field").click();
    harness.run();
    harness.get_by_label("Edit metadata field");
    harness.get_by_role_and_label(egui::accesskit::Role::TextInput, "Template key");
    harness.ui_harness(
        "settings/namespace_selector_settings_show_templates_and_the_open_field_editor/open_field_editor",
    );
}

#[test]
fn settings_shows_that_application_updates_are_being_checked() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    harness
        .state_mut()
        .updater
        .set_status_for_test(UpdateStatus::Checking);
    harness.run();

    let settings_position = harness.get_by_label("Settings").rect().center();
    primary_click(&mut harness, settings_position);
    harness.run();
    let application_settings_position = harness
        .get_by_label(OPEN_APPLICATION_SETTINGS)
        .rect()
        .center();
    primary_click(&mut harness, application_settings_position);
    harness.run();

    harness.get_by_label("Updates");
    harness.get_by_label("Checking for updates…");
    harness.ui_harness(HarnessSnapshotOptions::new(
        "updater/settings_shows_that_application_updates_are_being_checked/checking",
    ));
}

#[test]
fn settings_shows_a_staged_application_update() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    harness
        .state_mut()
        .updater
        .set_status_for_test(UpdateStatus::Staged {
            version: "0.2.0".into(),
        });
    harness.run();

    let settings_position = harness.get_by_label("Settings").rect().center();
    primary_click(&mut harness, settings_position);
    harness.run();
    let application_settings_position = harness
        .get_by_label(&format!("{OPEN_APPLICATION_SETTINGS} (Update ready)"))
        .rect()
        .center();
    primary_click(&mut harness, application_settings_position);
    harness.run();

    harness.get_by_label("Updates");
    harness.get_by_label("Version 0.2.0 is ready and will be installed on the next launch.");
    harness.ui_harness(HarnessSnapshotOptions::new(
        "updater/settings_shows_a_staged_application_update/staged_update",
    ));
}

#[test]
fn settings_replaces_an_inspector_owned_child_blade_without_resurrecting_history() {
    let mut harness = application_harness::<MockWorker>();
    let mut state = oracle_resource_table_state();
    let mut commands = Vec::new();
    open_inspector_with_column_settings(&mut state, &mut commands);
    assert_eq!(
        state.global_blades.navigator().unwrap().back_stack().len(),
        1
    );
    state.open_terminal_settings(&TerminalLaunchSettings::default(), &mut commands);
    assert!(state.clusters[&2].resource_detail_panel.is_none());
    assert!(state.terminal_settings_blade().is_some());
    assert!(commands.iter().any(|command| {
        command_is::<StopResourceDetailWatch>(command)
            .is_some_and(|command| command.cluster_key == 2 && command.history_entry_id == 1)
    }));
    harness.state_mut().ui_state = state;
    harness.state_mut().worker.commands.append(&mut commands);
    harness
        .ctx
        .global_style_mut(|style| style.animation_time = 0.0);
    harness.run_steps(2);

    harness.get_by_label("Close blade").click();
    harness.run_steps(2);

    assert!(harness.state().ui_state.global_blades.navigator().is_none());
    assert!(
        harness.state().ui_state.clusters[&2]
            .resource_detail_panel
            .is_none()
    );
}

#[test]
fn opening_an_inspector_in_another_cluster_replaces_all_panel_state() {
    let mut state = oracle_resource_table_state();
    let deployment = fixture_api_resource("apps", "Deployment", "deployments");
    let mut commands = Vec::new();
    state.open_resource_detail(
        1,
        deployment.clone(),
        "dev-api".into(),
        Some("default".into()),
        "dev-deployment-uid".into(),
        &mut commands,
    );
    state.open_resource_detail(
        2,
        deployment,
        "kind-api".into(),
        Some("kube-system".into()),
        "kind-deployment-uid".into(),
        &mut commands,
    );

    assert!(state.clusters[&1].resource_detail_panel.is_none());
    assert!(state.clusters[&2].resource_detail_panel.is_some());
    assert_eq!(
        state
            .global_blades
            .navigator()
            .unwrap()
            .current()
            .resource_detail()
            .unwrap()
            .cluster_key,
        2
    );
    assert!(commands.iter().any(|command| {
        command_is::<StopResourceDetailWatch>(command)
            .is_some_and(|command| command.cluster_key == 1 && command.history_entry_id == 1)
    }));
}

#[test]
fn closing_an_inspector_owned_column_settings_blade_cleans_up_the_inspector() {
    let mut harness = application_harness::<MockWorker>();
    let mut state = oracle_resource_table_state();
    let mut commands = Vec::new();
    open_inspector_with_column_settings(&mut state, &mut commands);
    harness.state_mut().ui_state = state;
    harness.state_mut().worker.commands.append(&mut commands);
    harness
        .ctx
        .global_style_mut(|style| style.animation_time = 0.0);
    harness.run_steps(2);

    harness.get_by_label("Configure columns");
    harness.get_by_label("Close blade").click();
    harness.run_steps(2);

    assert!(harness.state().ui_state.global_blades.navigator().is_none());
    assert!(
        harness.state().ui_state.clusters[&2]
            .resource_detail_panel
            .is_none()
    );
    assert!(harness.state().worker.commands.iter().any(|command| {
        command_is::<StopResourceDetailWatch>(command)
            .is_some_and(|command| command.cluster_key == 2 && command.history_entry_id == 1)
    }));
}

#[test]
fn escape_closes_an_inspector_owned_column_settings_blade_and_cleans_up() {
    let mut harness = application_harness::<MockWorker>();
    let mut state = oracle_resource_table_state();
    let mut commands = Vec::new();
    open_inspector_with_column_settings(&mut state, &mut commands);
    harness.state_mut().ui_state = state;
    harness.state_mut().worker.commands.append(&mut commands);
    harness
        .ctx
        .global_style_mut(|style| style.animation_time = 0.0);
    harness.run_steps(2);

    harness.input_mut().events.push(egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run_steps(2);

    assert!(harness.state().ui_state.global_blades.navigator().is_none());
    assert!(
        harness.state().ui_state.clusters[&2]
            .resource_detail_panel
            .is_none()
    );
    assert!(harness.state().worker.commands.iter().any(|command| {
        command_is::<StopResourceDetailWatch>(command)
            .is_some_and(|command| command.cluster_key == 2 && command.history_entry_id == 1)
    }));
}

#[test]
fn deleting_an_inspector_removes_its_foreground_column_settings_child() {
    let mut state = oracle_resource_table_state();
    let mut commands = Vec::new();
    open_inspector_with_column_settings(&mut state, &mut commands);

    ResourceDetailDeleted {
        cluster_key: 2,
        history_entry_id: 1,
    }
    .apply(&mut state, &mut commands);

    assert!(state.global_blades.navigator().is_none());
    assert!(state.clusters[&2].resource_detail_panel.is_none());
    assert!(commands.iter().any(|command| {
        command_is::<StopResourceDetailWatch>(command)
            .is_some_and(|command| command.cluster_key == 2 && command.history_entry_id == 1)
    }));
}

#[test]
fn refreshing_clusters_discards_open_inspectors_and_stops_their_watches() {
    let mut state = oracle_resource_table_state();
    let mut commands = Vec::new();
    state.open_resource_detail(
        2,
        fixture_api_resource("apps", "Deployment", "deployments"),
        "api".into(),
        Some("kube-system".into()),
        "deployment-uid".into(),
        &mut commands,
    );

    KubernetesClustersUpdated(vec![Cluster {
        name: "refreshed".into(),
        is_current: true,
        presentation: Default::default(),
        server: None,
    }])
    .apply(&mut state, &mut commands);

    assert!(state.global_blades.navigator().is_none());
    assert!(commands.iter().any(|command| {
        command_is::<StopResourceDetailWatch>(command)
            .is_some_and(|command| command.cluster_key == 2 && command.history_entry_id == 1)
    }));
}

#[test]
fn clicking_the_cluster_card_under_an_inspector_dismisses_it_without_reconnecting() {
    let mut harness = application_harness::<MockWorker>();
    let mut state = oracle_resource_table_state();
    let mut commands = Vec::new();
    state.open_resource_detail(
        2,
        fixture_api_resource("apps", "Deployment", "deployments"),
        "api".into(),
        Some("kube-system".into()),
        "deployment-uid".into(),
        &mut commands,
    );
    harness.state_mut().ui_state = state;
    harness.state_mut().worker.commands.append(&mut commands);
    harness
        .ctx
        .global_style_mut(|style| style.animation_time = 1.0);
    harness.run_steps(2);
    let commands_before = harness.state().worker.commands.len();

    let card = harness.get_by_label("Switch cluster").rect().center();
    primary_click(&mut harness, card);
    harness
        .ctx
        .global_style_mut(|style| style.animation_time = 0.0);
    harness.run_steps(4);

    let state = &harness.state().ui_state;
    assert!(state.global_blades.navigator().is_none());
    assert!(state.clusters[&2].resource_detail_panel.is_none());
    let new_commands = &harness.state().worker.commands[commands_before..];
    assert_eq!(
        new_commands
            .iter()
            .filter(|command| command_is::<StopResourceDetailWatch>(command)
                .is_some_and(|command| command.cluster_key == 2 && command.history_entry_id == 1))
            .count(),
        1,
        "the inspector's watches must be stopped exactly once"
    );
    assert!(
        new_commands
            .iter()
            .all(|command| command_is::<ConnectToCluster>(command).is_none()),
        "re-selecting a connected cluster must not reconnect it"
    );
}

/// Dismissing a toast removes only that toast; the outcome stays in the
/// notification history.
#[test]
fn dismissing_a_toast_removes_only_that_toast() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    harness.run();
    for title in ["Deployment scaled", "Deployment restarted"] {
        harness.state_mut().ui_state.push_operation_outcome(
            2,
            crate::ui::state::OperationOutcome::Success,
            title,
            Some("kube-system"),
            "web",
            None,
        );
    }
    run_with_pinned_toasts(&mut harness);
    let history = harness.state().ui_state.clusters[&2]
        .operation_history
        .len();
    let dismisses = harness.get_all_by_label("Dismiss").collect::<Vec<_>>();
    assert_eq!(dismisses.len(), 2);
    dismisses[0].click();
    harness.run();

    let toasts = &harness.state().ui_state.clusters[&2].transient_operation_toasts;
    assert_eq!(toasts.len(), 1);
    assert_eq!(toasts[0].title, "Deployment restarted");
    assert_eq!(
        harness.state().ui_state.clusters[&2]
            .operation_history
            .len(),
        history
    );
}
