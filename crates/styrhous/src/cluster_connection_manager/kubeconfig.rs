//! Source-preserving kubeconfig edits. Never write a merged, path-expanded Kubeconfig.

use crate::context_presentation::{CONTEXT_PRESENTATION_EXTENSION, ContextPresentation};
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_yaml::{Mapping, Value};
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

/// Serialize cooperating application edits, including managed-cluster imports.
pub(crate) static KUBECONFIG_WRITE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub(super) fn kubeconfig_paths() -> Result<Vec<PathBuf>> {
    let paths = std::env::var_os("KUBECONFIG")
        .map(|paths| {
            std::env::split_paths(&paths)
                .filter(|p| !p.as_os_str().is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if paths.is_empty() {
        Ok(vec![
            std::env::home_dir()
                .context("Could not find the home directory for kubeconfig")?
                .join(".kube/config"),
        ])
    } else {
        Ok(paths)
    }
}

pub(super) fn resolve_kubeconfig_path(path: &Path) -> Result<PathBuf> {
    let path = if let Ok(suffix) = path.strip_prefix("~") {
        std::env::home_dir()
            .context("Could not find the home directory for kubeconfig")?
            .join(suffix)
    } else {
        path.to_path_buf()
    };
    let path = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()?.join(path)
    };
    match std::fs::symlink_metadata(&path) {
        Ok(_) => path
            .canonicalize()
            .context("Could not resolve kubeconfig path"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(path),
        Err(error) => Err(error).context("Could not inspect kubeconfig path"),
    }
}

pub(super) fn write_kubeconfig_atomically(path: &Path, bytes: &[u8]) -> Result<()> {
    let path = resolve_kubeconfig_path(path)?;
    let metadata = match std::fs::metadata(&path) {
        Ok(metadata) => {
            anyhow::ensure!(
                !metadata.permissions().readonly(),
                "Kubeconfig is read-only"
            );
            Some(metadata)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error).context("Could not inspect kubeconfig permissions"),
    };
    let parent = path
        .parent()
        .context("Kubeconfig path has no parent directory")?;
    std::fs::create_dir_all(parent).context("Could not create kubeconfig directory")?;
    let mut temporary =
        NamedTempFile::new_in(parent).context("Could not create temporary kubeconfig")?;
    if let Some(metadata) = metadata {
        temporary
            .as_file()
            .set_permissions(metadata.permissions())?;
    }
    temporary
        .write_all(bytes)
        .context("Could not write kubeconfig")?;
    temporary
        .as_file()
        .sync_all()
        .context("Could not sync kubeconfig")?;
    temporary
        .persist(&path)
        .map_err(|error| error.error)
        .context("Could not save kubeconfig")?;
    Ok(())
}

pub(crate) fn save_context_presentation(
    context_name: &str,
    presentation: &ContextPresentation,
) -> Result<()> {
    save_context_in_paths(&kubeconfig_paths()?, context_name, presentation)
}

fn save_context_in_paths(
    paths: &[PathBuf],
    context_name: &str,
    presentation: &ContextPresentation,
) -> Result<()> {
    let presentation = presentation.normalized().map_err(anyhow::Error::msg)?;
    for source in paths {
        let path = resolve_kubeconfig_path(source)?;
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error).context("Could not read kubeconfig for editing"),
        };
        let (text, encoding) = decode(&bytes)?;
        // Parser errors can include source lines containing credentials. Keep them private.
        let mut documents = serde_yaml::Deserializer::from_str(&text)
            .map(Value::deserialize)
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|_| {
                anyhow::anyhow!("Kubeconfig contains invalid YAML; fix it before editing contexts")
            })?;
        for document in &mut documents {
            let Some(contexts) = document
                .get_mut("contexts")
                .and_then(Value::as_sequence_mut)
            else {
                continue;
            };
            let Some(named) = contexts
                .iter_mut()
                .find(|named| named.get("name").and_then(Value::as_str) == Some(context_name))
            else {
                continue;
            };
            let context = named
                .get_mut("context")
                .and_then(Value::as_mapping_mut)
                .context("This kubeconfig context has no editable context definition")?;
            patch_presentation(context, &presentation)?;
            let mut yaml = String::new();
            for (index, document) in documents.iter().enumerate() {
                if index > 0 {
                    yaml.push_str("---\n");
                }
                yaml.push_str(
                    &serde_yaml::to_string(document)
                        .map_err(|_| anyhow::anyhow!("Could not serialize kubeconfig"))?,
                );
            }
            // Avoid replacing unrelated edits made while the document was being prepared.
            anyhow::ensure!(
                std::fs::read(&path).context("Could not re-read kubeconfig")? == bytes,
                "Kubeconfig changed while saving. Please try again."
            );
            return write_kubeconfig_atomically(&path, &encode(&yaml, encoding));
        }
    }
    bail!("The context no longer exists in kubeconfig. Reload contexts before editing it.")
}

fn patch_presentation(context: &mut Mapping, presentation: &ContextPresentation) -> Result<()> {
    let key = Value::String("extensions".into());
    let extensions = context
        .entry(key.clone())
        .or_insert_with(|| Value::Sequence(Vec::new()));
    if extensions.is_null() {
        *extensions = Value::Sequence(Vec::new());
    }
    let extensions = extensions
        .as_sequence_mut()
        .context("This context's extensions must be a list")?;
    let index = extensions.iter().position(|entry| {
        entry.get("name").and_then(Value::as_str) == Some(CONTEXT_PRESENTATION_EXTENSION)
    });
    let index = match index {
        Some(index) => index,
        None if presentation == &ContextPresentation::default() => {
            if extensions.is_empty() {
                context.remove(&key);
            }
            return Ok(());
        }
        None => {
            extensions.push(serde_yaml::to_value(serde_json::json!({
                "name": CONTEXT_PRESENTATION_EXTENSION, "extension": {}
            }))?);
            extensions.len() - 1
        }
    };
    let values = extensions[index]
        .get_mut("extension")
        .and_then(Value::as_mapping_mut)
        .context("The Styrhous context extension must contain an object")?;
    for (key, value) in [
        ("display-name", &presentation.display_name),
        ("badge", &presentation.badge),
    ] {
        let key = Value::String(key.into());
        match value {
            Some(value) => {
                values.insert(key, Value::String(value.clone()));
            }
            None => {
                values.remove(&key);
            }
        }
    }
    if values.is_empty() {
        extensions.remove(index);
    }
    if extensions.is_empty() {
        context.remove(&key);
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Encoding {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
}

fn decode(bytes: &[u8]) -> Result<(String, Encoding)> {
    let (payload, encoding) = if let Some(bytes) = bytes.strip_prefix(&[0xff, 0xfe]) {
        (bytes, Encoding::Utf16Le)
    } else if let Some(bytes) = bytes.strip_prefix(&[0xfe, 0xff]) {
        (bytes, Encoding::Utf16Be)
    } else if let Some(bytes) = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]) {
        (bytes, Encoding::Utf8Bom)
    } else {
        (bytes, Encoding::Utf8)
    };
    let text = match encoding {
        Encoding::Utf8 | Encoding::Utf8Bom => String::from_utf8(payload.to_vec()).ok(),
        Encoding::Utf16Le | Encoding::Utf16Be => {
            anyhow::ensure!(
                payload.len().is_multiple_of(2),
                "Kubeconfig has invalid text encoding"
            );
            let words = payload
                .chunks_exact(2)
                .map(|p| match encoding {
                    Encoding::Utf16Le => u16::from_le_bytes([p[0], p[1]]),
                    _ => u16::from_be_bytes([p[0], p[1]]),
                })
                .collect::<Vec<_>>();
            String::from_utf16(&words).ok()
        }
    }
    .context("Kubeconfig has invalid text encoding")?;
    Ok((text, encoding))
}

fn encode(text: &str, encoding: Encoding) -> Vec<u8> {
    match encoding {
        Encoding::Utf8 => text.as_bytes().to_vec(),
        Encoding::Utf8Bom => [b"\xef\xbb\xbf".as_slice(), text.as_bytes()].concat(),
        Encoding::Utf16Le => [
            vec![0xff, 0xfe],
            text.encode_utf16().flat_map(u16::to_le_bytes).collect(),
        ]
        .concat(),
        Encoding::Utf16Be => [
            vec![0xfe, 0xff],
            text.encode_utf16().flat_map(u16::to_be_bytes).collect(),
        ]
        .concat(),
    }
}

#[cfg(test)]
mod tests;
