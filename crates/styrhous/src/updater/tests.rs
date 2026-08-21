use super::{UpdateStatus, updates_disabled_by_value};

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

#[cfg(feature = "self-updater")]
#[test]
fn updater_activation_matches_the_compiled_build_kind() {
    let (status, public_key) = super::updater_activation();
    if cfg!(styrhous_package_managed_build) {
        assert_eq!(status, UpdateStatus::ExternallyManaged);
        assert!(public_key.is_none());
    } else if cfg!(styrhous_release_build) {
        assert_eq!(status, UpdateStatus::Checking);
        assert!(public_key.is_some());
    } else {
        assert_eq!(status, UpdateStatus::LocalBuild);
        assert!(public_key.is_none());
    }
}
