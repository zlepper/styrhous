use super::{
    Config, Update, UpdateFormat, atomic_write, clear_pending_update_in, has_staged_update_in,
    macos_application_bundle_path, package_path_in, read_pending_update_in, stage_update_in,
    staged_update, verify_staged_package,
};
use base64::Engine;
use cargo_packager_updater::url::Url;
use std::fs;
use std::path::{Path, PathBuf};

fn update(version: &str) -> Update {
    Update {
        config: Config {
            endpoints: Vec::new(),
            pubkey: String::new(),
            windows: None,
        },
        body: None,
        current_version: "0.0.1-alpha.1".into(),
        version: version.into(),
        date: None,
        target: "linux-x86_64".into(),
        extract_path: PathBuf::from("Styrhous.AppImage"),
        download_url: Url::parse("https://invalid.example/update")
            .expect("fixture URL should be valid"),
        signature: "fixture signature".into(),
        timeout: None,
        headers: Default::default(),
        format: UpdateFormat::AppImage,
    }
}

#[test]
fn atomic_write_replaces_existing_staged_metadata() {
    let directory = std::env::temp_dir().join(format!(
        "styrhous-updater-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("the system clock is after the Unix epoch")
            .as_nanos()
    ));
    fs::create_dir_all(&directory).expect("temporary test directory should be created");
    let destination = directory.join("pending-update.yaml");

    atomic_write(&destination, b"first").expect("initial staged metadata should write");
    atomic_write(&destination, b"second").expect("staged metadata should be replaceable");

    assert_eq!(
        fs::read(&destination).expect("staged metadata should be readable"),
        b"second"
    );
    fs::remove_dir_all(directory).expect("temporary test directory should be removed");
}

#[test]
fn staged_update_persists_payload_and_replaces_a_superseded_version() {
    let directory = tempfile::tempdir().expect("temporary cache directory should exist");
    let first = update("0.0.1-alpha.2");
    stage_update_in(directory.path(), &first, b"first payload")
        .expect("first update should stage successfully");

    let first_pending = read_pending_update_in(directory.path())
        .expect("staged metadata should be readable")
        .expect("first update should have staged metadata");
    assert_eq!(first_pending.version, "0.0.1-alpha.2");
    assert_eq!(
        fs::read(package_path_in(directory.path(), &first_pending).expect("package path"))
            .expect("first payload should be readable"),
        b"first payload"
    );
    assert!(
        has_staged_update_in(directory.path(), "0.0.1-alpha.2")
            .expect("first staged version should be checked")
    );

    let second = update("0.0.1-alpha.3");
    stage_update_in(directory.path(), &second, b"second payload")
        .expect("replacement update should stage successfully");
    assert!(
        !package_path_in(directory.path(), &first_pending)
            .expect("first package path")
            .exists(),
        "the superseded payload should be removed"
    );

    let second_pending = read_pending_update_in(directory.path())
        .expect("replacement staged metadata should be readable")
        .expect("replacement update should have staged metadata");
    let reconstructed = staged_update("fixture public key", &second_pending)
        .expect("trusted staged update should reconstruct");
    assert_eq!(reconstructed.version, "0.0.1-alpha.3");
    assert_eq!(reconstructed.config.pubkey, "fixture public key");
    assert!(
        has_staged_update_in(directory.path(), "0.0.1-alpha.3")
            .expect("replacement staged version should be checked")
    );

    clear_pending_update_in(directory.path()).expect("staged update cleanup should succeed");
    assert!(
        read_pending_update_in(directory.path())
            .expect("staged metadata should be readable after cleanup")
            .is_none()
    );
    assert!(
        !package_path_in(directory.path(), &second_pending)
            .expect("second package path")
            .exists(),
        "cleanup should remove the staged payload"
    );
}

#[test]
fn staged_package_signature_must_match_the_payload() {
    let public_key = concat!(
        "untrusted comment: minisign public key\n",
        "RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3"
    );
    let signature = concat!(
        "untrusted comment: signature from minisign secret key\n",
        "RUQf6LRCGA9i559r3g7V1qNyJDApGip8MfqcadIgT9CuhV3EMhHoN1mGTkUidF/",
        "z7SrlQgXdy8ofjb7bNJJylDOocrCo8KLzZwo=\n",
        "trusted comment: timestamp:1633700835\tfile:test\tprehashed\n",
        "wLMDjy9FLAuxZ3q4NlEvkgtyhrr0gtTu6KC4KBJdITbbOeAi1zBIYo0v4iTgt8jJpIidRJnp94ABQkJAgAooBQ=="
    );
    let encoded_public_key = base64::engine::general_purpose::STANDARD.encode(public_key);
    let encoded_signature = base64::engine::general_purpose::STANDARD.encode(signature);

    verify_staged_package(&encoded_public_key, &encoded_signature, b"test")
        .expect("the fixture signature should authenticate the fixture payload");
    assert!(verify_staged_package(&encoded_public_key, &encoded_signature, b"tampered").is_err());
}

#[test]
fn macos_staged_updates_target_the_application_bundle() {
    assert_eq!(
        macos_application_bundle_path(Path::new(
            "/Applications/Styrhous.app/Contents/MacOS/styrhous"
        ))
        .expect("a macOS bundle executable should have an application path"),
        PathBuf::from("/Applications/Styrhous.app")
    );
}
