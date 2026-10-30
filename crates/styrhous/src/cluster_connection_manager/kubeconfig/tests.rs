use super::*;
use kube::config::Kubeconfig;

const CONFIG: &str = r#"apiVersion: v1
kind: Config
current-context: shared
clusters:
- name: cluster
  cluster:
    server: https://example.invalid
    certificate-authority: relative/ca.crt
users:
- name: identity
  user:
    client-key: relative/key.pem
    token: fixture-placeholder
contexts:
- name: shared
  context:
    cluster: cluster
    user: identity
    namespace: workloads
    extensions:
    - name: third-party
      extension:
        untouched: true
    - name: io.github.zlepper.styrhous
      extension:
        future-setting: keep
unknown-top-level: preserved
"#;

fn preferences(name: &str, badge: &str) -> ContextPresentation {
    ContextPresentation {
        display_name: Some(name.into()),
        badge: Some(badge.into()),
    }
}

fn read(path: &Path) -> Value {
    serde_yaml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn loaded_presentation(path: &Path, name: &str) -> ContextPresentation {
    let config = Kubeconfig::read_from(path).unwrap();
    let context = config.contexts.iter().find(|c| c.name == name).unwrap();
    let extension = context
        .context
        .as_ref()
        .unwrap()
        .extensions
        .as_ref()
        .unwrap()
        .iter()
        .find(|e| e.name == CONTEXT_PRESENTATION_EXTENSION)
        .unwrap();
    ContextPresentation::from_extension(&extension.extension)
}

#[test]
fn editing_accepts_a_context_with_null_extensions() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config");
    std::fs::write(
        &path,
        "contexts:\n- name: shared\n  context:\n    cluster: cluster\n    extensions: null\n",
    )
    .unwrap();
    let expected = preferences("Friendly", "FR");
    save_context_in_paths(std::slice::from_ref(&path), "shared", &expected).unwrap();
    assert_eq!(loaded_presentation(&path, "shared"), expected);
}

#[test]
fn editing_uses_first_definition_and_preserves_credentials_extensions_and_relative_paths() {
    let directory = tempfile::tempdir().unwrap();
    let first = directory.path().join("first.yaml");
    let second = directory.path().join("second.yaml");
    std::fs::write(&first, CONFIG).unwrap();
    std::fs::write(
        &second,
        CONFIG.replace("namespace: workloads", "namespace: shadowed"),
    )
    .unwrap();
    let shadowed = std::fs::read(&second).unwrap();
    let original = read(&first);
    let paths = [
        directory.path().join("missing.yaml"),
        first.clone(),
        second.clone(),
    ];
    let expected = preferences("Production", "PR");
    save_context_in_paths(&paths, "shared", &expected).unwrap();
    assert_eq!(loaded_presentation(&first, "shared"), expected);
    assert_eq!(std::fs::read(&second).unwrap(), shadowed);
    let mut saved = read(&first);
    assert_eq!(
        saved["contexts"][0]["context"]["extensions"][1]["extension"]["future-setting"],
        "keep"
    );
    // Only the two owned presentation properties may have changed.
    saved["contexts"][0]["context"]["extensions"][1]["extension"]
        .as_mapping_mut()
        .unwrap()
        .remove(Value::String("display-name".into()));
    saved["contexts"][0]["context"]["extensions"][1]["extension"]
        .as_mapping_mut()
        .unwrap()
        .remove(Value::String("badge".into()));
    assert_eq!(saved, original);

    // Save reads the current source, including independent edits since it was first loaded.
    std::fs::write(
        &first,
        std::fs::read_to_string(&first)
            .unwrap()
            .replace("namespace: workloads", "namespace: newer"),
    )
    .unwrap();
    save_context_in_paths(&paths, "shared", &ContextPresentation::default()).unwrap();
    let cleared = read(&first);
    assert_eq!(cleared["contexts"][0]["context"]["namespace"], "newer");
    let extension = &cleared["contexts"][0]["context"]["extensions"][1]["extension"];
    assert_eq!(extension["future-setting"], "keep");
    assert!(extension.get("badge").is_none());
    assert!(extension.get("display-name").is_none());
}

#[test]
fn editing_second_file_and_yaml_document_preserves_encoding_and_document_precedence() {
    let directory = tempfile::tempdir().unwrap();
    let first = directory.path().join("first.yaml");
    let second = directory.path().join("second.yaml");
    std::fs::write(&first, "apiVersion: v1\nkind: Config\ncontexts: []\n").unwrap();
    let yaml = "apiVersion: v1\nkind: Config\ncontexts: []\n---\ncontexts:\n- name: target\n  context:\n    cluster: cluster\n---\ncontexts:\n- name: target\n  context:\n    cluster: shadowed\n";
    for encoding in [
        Encoding::Utf8,
        Encoding::Utf8Bom,
        Encoding::Utf16Le,
        Encoding::Utf16Be,
    ] {
        std::fs::write(&second, encode(yaml, encoding)).unwrap();
        let expected = preferences("Friendly", "FR");
        save_context_in_paths(&[first.clone(), second.clone()], "target", &expected).unwrap();
        assert_eq!(loaded_presentation(&second, "target"), expected);
        let bytes = std::fs::read(&second).unwrap();
        let (saved, _) = decode(&bytes).unwrap();
        assert_eq!(bytes, encode(&saved, encoding));
        let documents: Vec<Value> = serde_yaml::Deserializer::from_str(&saved)
            .map(|doc| Value::deserialize(doc).unwrap())
            .collect();
        assert_eq!(documents.len(), 3);
        assert!(documents[0]["contexts"].as_sequence().unwrap().is_empty());
        assert_eq!(
            documents[2]["contexts"][0]["context"]["cluster"],
            "shadowed"
        );
        assert!(
            documents[2]["contexts"][0]["context"]
                .get("extensions")
                .is_none()
        );
        save_context_in_paths(
            std::slice::from_ref(&second),
            "target",
            &ContextPresentation::default(),
        )
        .unwrap();
        let (cleared, _) = decode(&std::fs::read(&second).unwrap()).unwrap();
        assert!(!cleared.contains(CONTEXT_PRESENTATION_EXTENSION));
    }
}

#[test]
fn invalid_missing_or_malformed_context_edits_leave_the_source_untouched() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config");
    for source in [
        CONFIG.to_owned(),
        "contexts: [invalid yaml".into(),
        CONFIG.replace(
            "extensions:\n    -",
            "extensions: invalid\n    ignored:\n    -",
        ),
    ] {
        std::fs::write(&path, &source).unwrap();
        assert!(
            save_context_in_paths(
                std::slice::from_ref(&path),
                "missing",
                &preferences("Friendly", "FR")
            )
            .is_err()
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), source);
        assert!(
            save_context_in_paths(
                std::slice::from_ref(&path),
                "shared",
                &preferences("Friendly", "too long")
            )
            .is_err()
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), source);
    }
    std::fs::write(
        &path,
        "contexts:\n- name: shared\n  context:\n    cluster: cluster\n    extensions: invalid\n",
    )
    .unwrap();
    let original = std::fs::read(&path).unwrap();
    assert!(
        save_context_in_paths(
            std::slice::from_ref(&path),
            "shared",
            &preferences("Friendly", "FR")
        )
        .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), original);
}

#[cfg(unix)]
#[test]
fn saves_preserve_symlinks_and_permissions_and_report_read_only_files() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("actual-config");
    let link = directory.path().join("config");
    std::fs::write(&target, CONFIG).unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
    symlink("actual-config", &link).unwrap();
    save_context_in_paths(
        std::slice::from_ref(&link),
        "shared",
        &preferences("Friendly", "FR"),
    )
    .unwrap();
    assert!(std::fs::symlink_metadata(&link).unwrap().is_symlink());
    assert_eq!(
        loaded_presentation(&target, "shared"),
        preferences("Friendly", "FR")
    );
    assert_eq!(
        std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let original = std::fs::read(&target).unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o400)).unwrap();
    assert!(save_context_in_paths(&[link], "shared", &preferences("Other", "OT")).is_err());
    assert_eq!(std::fs::read(&target).unwrap(), original);
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 2);
}
