use super::{UpdateStatus, updater_activation_for, updates_disabled_by_value};

#[cfg(feature = "self-updater")]
use super::{updater_activation, updates_disabled_by_environment};

#[test]
fn recognizes_package_manager_update_opt_out_values() {
    for value in ["1", "true", "TRUE", " yes "] {
        assert!(
            updates_disabled_by_value(value),
            "{value:?} should disable updates"
        );
    }
    for value in ["", "0", "false", "no", "enabled"] {
        assert!(
            !updates_disabled_by_value(value),
            "{value:?} should not disable updates"
        );
    }
}

#[test]
fn staged_update_summary_mentions_the_next_launch() {
    assert!(
        UpdateStatus::Staged {
            version: "1.2.3".into()
        }
        .summary()
        .contains("next launch")
    );
}

#[test]
fn only_active_update_states_show_a_badge() {
    assert!(UpdateStatus::Checking.shows_badge());
    assert!(
        UpdateStatus::Staged {
            version: "1.2.3".into()
        }
        .shows_badge()
    );
    assert!(!UpdateStatus::LocalBuild.shows_badge());
    assert!(!UpdateStatus::ExternallyManaged.shows_badge());
}

#[test]
fn updater_activation_follows_the_build_policy() {
    let missing_key = || UpdateStatus::Failed {
        message: "this release was built without an updater public key".into(),
    };
    let cases = [
        (
            "local build",
            (false, false, false, None),
            (UpdateStatus::LocalBuild, None),
        ),
        (
            "direct release",
            (false, false, true, Some("public-key")),
            (UpdateStatus::Checking, Some("public-key")),
        ),
        (
            "runtime opt-out",
            (true, false, true, Some("public-key")),
            (UpdateStatus::ExternallyManaged, None),
        ),
        (
            "package-managed build",
            (false, true, true, Some("public-key")),
            (UpdateStatus::ExternallyManaged, None),
        ),
        (
            "missing release key",
            (false, false, true, None),
            (missing_key(), None),
        ),
        (
            "blank release key",
            (false, false, true, Some("  ")),
            (missing_key(), None),
        ),
    ];

    for (description, (disabled, package_managed, release, key), expected) in cases {
        assert_eq!(
            updater_activation_for(disabled, package_managed, release, key),
            expected,
            "{description}"
        );
    }
}

#[cfg(feature = "self-updater")]
#[test]
fn updater_activation_matches_the_compiled_build_configuration() {
    let (status, public_key) = updater_activation();
    if updates_disabled_by_environment() || cfg!(styrhous_package_managed_build) {
        assert_eq!(status, UpdateStatus::ExternallyManaged);
        assert!(public_key.is_none());
    } else if !cfg!(styrhous_release_build) {
        assert_eq!(status, UpdateStatus::LocalBuild);
        assert!(public_key.is_none());
    } else if let Some(expected_key) =
        option_env!("STYRHOUS_UPDATER_PUBLIC_KEY").filter(|key| !key.trim().is_empty())
    {
        assert_eq!(status, UpdateStatus::Checking);
        assert_eq!(public_key, Some(expected_key));
    } else {
        assert_eq!(
            status,
            UpdateStatus::Failed {
                message: "this release was built without an updater public key".into()
            }
        );
        assert!(public_key.is_none());
    }
}
