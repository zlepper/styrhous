use super::table_preferences::MetadataColumnSource;
use crate::minimal_namespace::MinimalNamespace;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Global display preferences for namespace identities.
#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(super) struct NamespaceSelectorSettings {
    #[serde(default)]
    pub(super) fields: Vec<NamespaceMetadataField>,
    #[serde(default)]
    pub(super) templates: Vec<NamespaceIdentityTemplate>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub(super) struct NamespaceMetadataField {
    pub(super) alias: String,
    pub(super) source: MetadataColumnSource,
    pub(super) key: String,
}

impl Default for NamespaceMetadataField {
    fn default() -> Self {
        Self {
            alias: String::new(),
            source: MetadataColumnSource::Label,
            key: String::new(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub(super) struct NamespaceIdentityTemplate {
    pub(super) template: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) struct NamespacePresentation {
    pub(super) primary: String,
    pub(super) secondary: String,
    pub(super) search_text: String,
}

impl NamespaceSelectorSettings {
    pub(super) fn validate(&self) -> Result<(), String> {
        self.validate_fields()?;
        for template in &self.templates {
            let aliases = template_aliases(&template.template)?;
            if aliases.is_empty() {
                return Err(
                    "Each identity template must include at least one {{alias}} placeholder."
                        .into(),
                );
            }
            if let Some(alias) = aliases.iter().find(|alias| {
                !self
                    .fields
                    .iter()
                    .any(|field| field.alias.trim() == alias.as_str())
            }) {
                return Err(format!(
                    "The template refers to unknown alias '{{{{{alias}}}}}'."
                ));
            }
        }
        Ok(())
    }

    pub(super) fn validate_fields(&self) -> Result<(), String> {
        let mut aliases = BTreeSet::new();
        let mut metadata_keys = BTreeSet::new();
        for field in &self.fields {
            if !is_valid_alias(field.alias.trim()) {
                return Err("Each template alias must start with a lowercase letter and contain only lowercase letters, numbers, hyphens, or underscores.".into());
            }
            if field.key.trim().is_empty() {
                return Err("Each metadata field needs a metadata key.".into());
            }
            if !aliases.insert(field.alias.trim()) {
                return Err("Template aliases must be unique.".into());
            }
            if !metadata_keys.insert((field.source, field.key.trim())) {
                return Err("Each label or annotation key can only be configured once.".into());
            }
        }
        Ok(())
    }
}

pub(super) fn presentation(
    namespace: &MinimalNamespace,
    settings: &NamespaceSelectorSettings,
) -> NamespacePresentation {
    let primary = settings
        .templates
        .iter()
        .find_map(|template| resolve_template(namespace, settings, &template.template))
        .unwrap_or_else(|| namespace.name.clone());
    let mut search_values = vec![namespace.name.clone(), primary.clone()];
    search_values.extend(
        settings
            .fields
            .iter()
            .filter_map(|field| metadata_value(namespace, field).map(ToOwned::to_owned)),
    );
    NamespacePresentation {
        primary,
        secondary: namespace.name.clone(),
        search_text: search_values.join(" "),
    }
}

fn resolve_template(
    namespace: &MinimalNamespace,
    settings: &NamespaceSelectorSettings,
    template: &str,
) -> Option<String> {
    let mut rendered = String::new();
    let mut remaining = template;
    let mut placeholder_count = 0;
    while let Some(open) = remaining.find("{{") {
        rendered.push_str(&remaining[..open]);
        let after_open = &remaining[open + 2..];
        let close = after_open.find("}}")?;
        let alias = &after_open[..close];
        if !is_valid_alias(alias) {
            return None;
        }
        let field = settings
            .fields
            .iter()
            .find(|field| field.alias.trim() == alias)?;
        rendered.push_str(metadata_value(namespace, field)?);
        placeholder_count += 1;
        remaining = &after_open[close + 2..];
    }
    if placeholder_count == 0 || remaining.contains("}}") {
        return None;
    }
    rendered.push_str(remaining);
    Some(rendered)
}

fn metadata_value<'a>(
    namespace: &'a MinimalNamespace,
    field: &NamespaceMetadataField,
) -> Option<&'a str> {
    let value = match field.source {
        MetadataColumnSource::Label => namespace.labels.get(field.key.trim()),
        MetadataColumnSource::Annotation => namespace.annotations.get(field.key.trim()),
    }?;
    (!value.trim().is_empty()).then_some(value.as_str())
}

fn is_valid_alias(alias: &str) -> bool {
    let mut chars = alias.chars();
    matches!(chars.next(), Some(character) if character.is_ascii_lowercase())
        && chars.all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '-' | '_')
        })
}

pub(super) fn template_aliases(template: &str) -> Result<Vec<String>, String> {
    let mut aliases = Vec::new();
    let mut remaining = template;
    while let Some(open) = remaining.find("{{") {
        let after_open = &remaining[open + 2..];
        let Some(close) = after_open.find("}}") else {
            return Err("Templates must close every {{alias}} placeholder.".into());
        };
        let alias = &after_open[..close];
        if !is_valid_alias(alias) {
            return Err("Template placeholders must use a valid lowercase alias.".into());
        }
        if !aliases.iter().any(|existing| existing == alias) {
            aliases.push(alias.to_owned());
        }
        remaining = &after_open[close + 2..];
    }
    if remaining.contains("}}") {
        return Err("Templates cannot contain an unmatched }}.".into());
    }
    Ok(aliases)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn namespace() -> MinimalNamespace {
        MinimalNamespace {
            name: "company-a1b2".into(),
            labels: BTreeMap::from([("company.example/customer".into(), "Acme".into())]),
            annotations: BTreeMap::from([(
                "company.example/environment".into(),
                "Production".into(),
            )]),
        }
    }

    fn settings() -> NamespaceSelectorSettings {
        NamespaceSelectorSettings {
            fields: vec![
                NamespaceMetadataField {
                    alias: "customer".into(),
                    source: MetadataColumnSource::Label,
                    key: "company.example/customer".into(),
                },
                NamespaceMetadataField {
                    alias: "environment".into(),
                    source: MetadataColumnSource::Annotation,
                    key: "company.example/environment".into(),
                },
            ],
            templates: vec![
                NamespaceIdentityTemplate {
                    template: "Customer: {{customer}} · Environment: {{environment}}".into(),
                },
                NamespaceIdentityTemplate {
                    template: "Customer: {{customer}}".into(),
                },
            ],
        }
    }

    #[test]
    fn selects_the_first_template_with_all_metadata_values() {
        let result = presentation(&namespace(), &settings());
        assert_eq!(result.primary, "Customer: Acme · Environment: Production");
        assert!(result.search_text.contains("Production"));
    }

    #[test]
    fn falls_back_to_the_next_template_then_the_raw_namespace_name() {
        let mut namespace = namespace();
        namespace.annotations.clear();
        assert_eq!(
            presentation(&namespace, &settings()).primary,
            "Customer: Acme"
        );
        namespace.labels.clear();
        assert_eq!(
            presentation(&namespace, &settings()).primary,
            "company-a1b2"
        );
    }

    #[test]
    fn rejects_invalid_or_unknown_template_aliases() {
        let mut settings = settings();
        settings.templates[0].template = "{{unknown}}".into();
        assert!(settings.validate().is_err());
        settings.templates[0].template = "{{Customer}}".into();
        assert!(settings.validate().is_err());
        settings.templates[0].template = "{{ customer }}".into();
        assert!(settings.validate().is_err());
    }

    #[test]
    fn template_values_are_inserted_once_without_reprocessing_placeholders() {
        let mut namespace = namespace();
        namespace
            .labels
            .insert("company.example/customer".into(), "{{environment}}".into());
        assert_eq!(
            presentation(&namespace, &settings()).primary,
            "Customer: {{environment}} · Environment: Production"
        );
    }

    #[test]
    fn default_settings_ignore_the_legacy_tesseract_display_name_annotation() {
        let mut namespace = namespace();
        namespace.annotations.insert(
            "tesseract.dev/display-name".into(),
            "Old display name".into(),
        );
        assert_eq!(
            presentation(&namespace, &NamespaceSelectorSettings::default()).primary,
            "company-a1b2"
        );
    }
}
