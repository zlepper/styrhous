//! Settings, licence and notifications on the design boards A08–A12 and A14.

use super::super::design_fixtures::{KIND_KIND, KUBE_SYSTEM, design_harness, design_now, pods};
use super::global_blades::fixed_lease;
use super::*;
use crate::api_resource::ApiResource;
use crate::licensing::LicenseStatus;
use crate::ui::clock::WallClock;
use crate::ui::state::{BulkDeleteProgress, BulkDeleteTarget};
use time::Duration;

/// Let a blade finish sliding in, so the blades behind it have taken their
/// resting places.
fn settle_blades(harness: &mut AppHarness) {
    harness.run_steps(40);
}

/// A08: the Settings blade over the Pods table.
#[test]
fn a08_settings_blade() {
    let mut harness = design_harness(BOARD_SIZE);
    harness
        .state_mut()
        .licensing
        .set_status_for_test(LicenseStatus::Licensed(fixed_lease(
            "commercial",
            "active_subscription",
        )));
    harness
        .state_mut()
        .updater
        .set_status_for_test(UpdateStatus::Staged {
            version: "1.4.0".into(),
        });
    harness.run();
    open_settings(&mut harness);
    rest_pointer(&mut harness);
    harness.ui_harness("boards/a08_settings");
}

/// "Now" on A12: half past midnight, so an outcome from an hour ago is from
/// yesterday and the board lists it under "Earlier".
fn notifications_now() -> time::OffsetDateTime {
    design_now().date().midnight().assume_utc() + Duration::minutes(30)
}

/// Set the UI clock `minutes` before "now" on A12, so outcomes recorded
/// next read as that old when the board is drawn.
fn set_clock_minutes_before_now(harness: &mut AppHarness, minutes: i64) {
    harness.state_mut().ui_state.clock =
        WallClock::Fixed(notifications_now() - Duration::minutes(minutes));
}

/// A12's session: five outcomes on `kind-kind` and a discovery warning on
/// `dev-eu`, recorded at the times the board reads (1 h, 21, 14, 6 and 2 min
/// before "now"). Only the last two are unread.
fn deliver_notification_history(harness: &mut AppHarness) {
    // `dev-eu`: one API group did not load.
    set_clock_minutes_before_now(harness, 60);
    harness.deliver_worker_result(KubernetesApiDiscoveryCompleted {
        cluster_key: 1,
        failed_api_groups: vec![FailedApiGroup {
            group_version: "metrics.k8s.io/v1beta1".into(),
            error: "the server is currently unable to handle the request".into(),
        }],
        group_list_failed: false,
    });

    set_clock_minutes_before_now(harness, 21);
    harness.deliver_worker_result(ResourceScaleUpdated {
        cluster_key: KIND_KIND,
        api_resource: ApiResource {
            group: "apps".into(),
            version: "v1".into(),
            kind: "Deployment".into(),
            name: "deployments".into(),
            namespaced: true,
        },
        namespace: Some(KUBE_SYSTEM.into()),
        resource_name: "coredns".into(),
        replicas: 4,
    });

    set_clock_minutes_before_now(harness, 14);
    let operation_id = harness
        .state_mut()
        .ui_state
        .clusters
        .get_mut(&KIND_KIND)
        .expect("kind-kind exists")
        .cron_job_runs
        .start(());
    harness.deliver_worker_result(CronJobRunCompleted {
        cluster_key: KIND_KIND,
        operation_id,
        namespace: KUBE_SYSTEM.into(),
        cron_job_name: "nightly-report".into(),
        job_name: "nightly-report-manual-x7k2p".into(),
    });
    // Everything so far was seen; the next two are not.
    harness.state_mut().ui_state.mark_operation_history_read();

    set_clock_minutes_before_now(harness, 6);
    let targets =
        ["api-7b948f-pod", "kindnet-9qrlh", "kube-proxy-v86gd"].map(|name| BulkDeleteTarget {
            uid: format!("{name}-uid"),
            name: name.into(),
            namespace: Some(KUBE_SYSTEM.into()),
        });
    harness
        .state_mut()
        .ui_state
        .clusters
        .get_mut(&KIND_KIND)
        .expect("kind-kind exists")
        .bulk_delete_progress = Some(BulkDeleteProgress::new(1, pods(), targets.to_vec()));
    for target in &targets[1..] {
        harness.deliver_worker_result(ResourceDeleteCompleted {
            cluster_key: KIND_KIND,
            api_resource: pods(),
            namespace: target.namespace.clone(),
            resource_name: target.name.clone(),
            bulk_delete_id: Some(1),
        });
    }
    harness.deliver_worker_result(ResourceDeleteFailed {
        cluster_key: KIND_KIND,
        api_resource: pods(),
        namespace: Some(KUBE_SYSTEM.into()),
        resource_name: "api-7b948f-pod".into(),
        bulk_delete_id: Some(1),
        error: "pods “api-7b948f-pod” is forbidden".into(),
    });

    set_clock_minutes_before_now(harness, 2);
    let operation_id = harness
        .state_mut()
        .ui_state
        .clusters
        .get_mut(&KIND_KIND)
        .expect("kind-kind exists")
        .cron_job_runs
        .start(());
    harness.deliver_worker_result(CronJobRunFailed {
        cluster_key: KIND_KIND,
        operation_id,
        namespace: KUBE_SYSTEM.into(),
        cron_job_name: "nightly-report".into(),
        error: "Forbidden: cannot create jobs.batch".into(),
    });
    harness.state_mut().ui_state.clock = WallClock::Fixed(notifications_now());
    harness.run();
}

fn open_notifications(harness: &mut AppHarness) {
    click_label(harness, "Notifications");
}

/// A12: the Notifications blade over the Pods table.
#[test]
fn a12_notifications() {
    let mut harness = design_harness(BOARD_SIZE);
    deliver_notification_history(&mut harness);
    assert_eq!(harness.state().ui_state.unread_operation_count(), 2);
    open_notifications(&mut harness);
    rest_pointer(&mut harness);
    harness.ui_harness("boards/a12_notifications");
}

/// A14: Application settings, opened from the Settings blade, with the first
/// debug image's profile menu open.
#[test]
fn a14_application_settings() {
    let mut harness = design_harness(BOARD_SIZE);
    harness.state_mut().terminal_launch_settings.custom_template =
        Some("alacritty -e {command}".into());
    harness.run();
    open_settings(&mut harness);
    click_label(&mut harness, OPEN_APPLICATION_SETTINGS);
    settle_blades(&mut harness);
    let profile = harness
        .get_by_role_and_label(
            egui::accesskit::Role::ComboBox,
            "Debug image 1 debug profile",
        )
        .rect()
        .center();
    primary_click(&mut harness, profile);
    harness.run();
    let baseline = harness.get_by_label("Baseline").rect().center();
    harness.event(egui::Event::PointerMoved(baseline));
    harness.run();
    harness.ui_harness("boards/a14_application_settings");
}

/// A09: Edit context, opened from Contexts, after a save that failed.
#[test]
fn a09_edit_context() {
    let mut harness = design_harness(BOARD_SIZE);
    // The board edits a context of another name.
    harness
        .state_mut()
        .ui_state
        .clusters
        .get_mut(&4)
        .expect("prod-eu exists")
        .name = "cloudstorage-production-eu".into();
    harness.run();
    open_settings(&mut harness);
    click_label(
        &mut harness,
        "Open context settings: Display names and rail badges (4 contexts)",
    );
    click_label(&mut harness, "Edit context cloudstorage-production-eu");
    // The name is typed in the field, which keeps the focus.
    let name = harness
        .get_by_role_and_label(egui::accesskit::Role::TextInput, "Display name")
        .rect()
        .center();
    primary_click(&mut harness, name);
    harness.event(egui::Event::Text("Production Europe".into()));
    harness.run();
    let badge = harness
        .get_by_role_and_label(egui::accesskit::Role::TextInput, "Badge")
        .rect()
        .center();
    primary_click(&mut harness, badge);
    harness.event(egui::Event::Text("PE".into()));
    harness.run();
    // The kubeconfig is read-only: the save is refused.
    click_label(&mut harness, "Save display name");
    let request_id =
        harness.state().ui_state.context_presentation_saves["cloudstorage-production-eu"]
            .request_id;
    harness.deliver_worker_result(ContextPresentationSaveFailed {
        context_name: "cloudstorage-production-eu".into(),
        request_id,
        error: "Styrhous couldn’t write to ~/.kube/config (read-only file system). Display names are stored separately; the badge needs write access.".into(),
    });
    let name = harness
        .get_by_role_and_label(egui::accesskit::Role::TextInput, "Display name")
        .rect()
        .center();
    primary_click(&mut harness, name);
    settle_blades(&mut harness);
    harness.get_by_label("Changes can’t be saved");
    harness.get_by_label("Save display name");
    harness.ui_harness("boards/a09_edit_context");
}

/// A10: Cluster discovery. The Azure CLI and Tailscale are missing; the AWS
/// CLI found one account with a `production` profile, and a scan found two
/// clusters, one already in the kubeconfig.
#[test]
fn a10_cluster_discovery() {
    let mut harness = design_harness(BOARD_SIZE);
    open_settings(&mut harness);
    click_label(
        &mut harness,
        "Open cluster discovery: Find clusters in Azure, AWS and Tailscale",
    );
    harness.deliver_worker_result(AwsAccountsUpdated {
        request_id: 1,
        cli_installed: true,
        accounts: vec![AwsAccount {
            id: "222222222222".into(),
            partition: "aws".into(),
            profiles: vec![AwsProfile::Named("production".into())],
        }],
        warning: None,
        error: None,
    });
    harness.deliver_worker_result(ManagedClusterDiscoveryUpdated {
        tools: ClusterDiscoveryTools::default(),
        aks_clusters: Vec::new(),
        tailscale_clusters: Vec::new(),
        azure_error: None,
        azure_warning: None,
        tailscale_error: None,
    });
    click_label(&mut harness, "Scan clusters");
    let request_id = harness
        .state()
        .ui_state
        .managed_cluster_discovery
        .aws
        .request_id;
    let profile = AwsProfile::Named("production".into());
    let cluster = |name: &str, configured| AvailableEksCluster {
        name: name.into(),
        region: "eu-west-1".into(),
        account_id: "222222222222".into(),
        partition: "aws".into(),
        profile: profile.clone(),
        tags: BTreeMap::from([("team".into(), "payments".into())]),
        configured,
    };
    harness.deliver_worker_result(EksClustersUpdated {
        request_id,
        account_id: "222222222222".into(),
        profile: profile.clone(),
        clusters: vec![
            cluster("payments", false),
            cluster("payments-staging", true),
        ],
        warning: None,
    });
    settle_blades(&mut harness);
    rest_pointer(&mut harness);
    harness.ui_harness("boards/a10_cluster_discovery");
}

/// A11: License & account in a grace period, under the warning banner.
#[test]
fn a11_license_and_account_in_grace_period() {
    let mut harness = design_harness(BOARD_SIZE);
    let licensing = &mut harness.state_mut().licensing;
    licensing.set_status_for_test(LicenseStatus::Licensed(fixed_lease(
        "grace",
        "payment_failed",
    )));
    licensing.set_last_check_for_test((design_now() - Duration::days(2)).unix_timestamp() as u64);
    harness.run();
    open_settings(&mut harness);
    click_label(
        &mut harness,
        "Open license and account settings: Hosted Styrhous service (Needs attention)",
    );
    settle_blades(&mut harness);
    rest_pointer(&mut harness);
    harness.get_by_label("Grace period until 1 January 2100");
    harness.ui_harness("boards/a11_license_account");
}

/// "Problems" lists the warnings and failures only; "All" brings the rest
/// back, and "Mark all read" clears the unread lamps and then disables itself.
#[test]
fn notifications_filter_problems_and_mark_everything_read() {
    let mut harness = design_harness(BOARD_SIZE);
    deliver_notification_history(&mut harness);
    open_notifications(&mut harness);
    click_label(&mut harness, "Problems");
    for title in [
        "Couldn’t run CronJob",
        "Bulk delete finished with 1 failure",
    ] {
        harness.get_by_label(title);
    }
    for title in ["CronJob started", "Deployment scaled to 4 replicas"] {
        assert!(harness.query_by_label(title).is_none(), "{title}");
    }
    // The warning from an hour ago is a problem too, listed under "Earlier".
    harness.get_by_label("1 API group couldn’t be loaded");
    harness.get_by_label("EARLIER");
    rest_pointer(&mut harness);
    harness.ui_harness("boards/a12_notifications_problems");

    click_label(&mut harness, "All");
    harness.get_by_label("CronJob started");
    click_label(&mut harness, "Mark all read");
    assert_eq!(harness.state().ui_state.unread_operation_count(), 0);
    assert!(
        harness
            .get_by_label("Mark all read")
            .accesskit_node()
            .is_disabled()
    );
}

/// With nothing recorded the blade says so, and "Problems" has nothing to list.
#[test]
fn notifications_say_when_there_is_nothing_to_show() {
    let mut harness = design_harness(BOARD_SIZE);
    open_notifications(&mut harness);
    harness.get_by_label("Resource operation outcomes will appear here.");
    assert!(
        harness
            .get_by_label("Mark all read")
            .accesskit_node()
            .is_disabled()
    );
}

/// The update switch is part of the draft: Cancel drops it, Reset restores
/// the default, Save keeps it.
#[test]
fn application_settings_keep_the_update_choice_until_saved() {
    let mut harness = design_harness(BOARD_SIZE);
    open_settings(&mut harness);
    click_label(&mut harness, OPEN_APPLICATION_SETTINGS);
    settle_blades(&mut harness);

    click_label(&mut harness, "Check for updates automatically");
    assert!(harness.state().terminal_launch_settings.check_for_updates);
    // Nothing is stored until Save changes, and Cancel leaves the stored choice.
    click_label(&mut harness, "Cancel");
    settle_blades(&mut harness);
    assert!(harness.state().terminal_launch_settings.check_for_updates);

    open_settings(&mut harness);
    click_label(&mut harness, OPEN_APPLICATION_SETTINGS);
    settle_blades(&mut harness);
    click_label(&mut harness, "Check for updates automatically");
    click_label(&mut harness, "Reset");
    assert!(
        harness
            .state()
            .ui_state
            .terminal_settings_blade()
            .unwrap()
            .draft
            .check_for_updates,
        "Reset restores the default"
    );
    click_label(&mut harness, "Check for updates automatically");
    click_label(&mut harness, "Save changes");
    assert!(!harness.state().terminal_launch_settings.check_for_updates);
}

/// Picking a colour offers to save just the colour and sends it with the
/// save; picking it again returns to the automatic colour.
#[test]
fn context_colour_is_picked_saved_and_cleared_by_picking_it_again() {
    let mut harness = design_harness(BOARD_SIZE);
    open_settings(&mut harness);
    click_label(
        &mut harness,
        "Open context settings: Display names and rail badges (4 contexts)",
    );
    click_label(&mut harness, "Edit context prod-eu");
    click_label(&mut harness, "Colour 5");
    click_label(&mut harness, "Colour 5");
    harness.get_by_label("Save changes");
    click_label(&mut harness, "Colour 5");
    harness.get_by_label("Save colour");
    click_label(&mut harness, "Save colour");
    let command = harness
        .state()
        .worker
        .commands
        .iter()
        .find_map(command_is::<SaveContextPresentation>)
        .expect("saving sends the context presentation");
    assert_eq!(command.context_name, "prod-eu");
    assert_eq!(command.presentation.colour, Some(4));
}
