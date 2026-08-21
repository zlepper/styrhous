use crate::api_resource::ApiResource;
use components::fuzzy::{matches_fuzzy, normalize_for_search};
use k8s_openapi::serde_json::{self, Value};
use saphyr::{LoadableYamlNode, MarkedYaml, Scalar, YamlData};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionSuggestion {
    pub label: String,
    pub type_label: Option<String>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionContext {
    pub kind: CompletionContextKind,
    pub type_label: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionContextKind {
    MappingKey,
    Value,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompletionResult {
    pub suggestions: Vec<CompletionSuggestion>,
    pub context: Option<CompletionContext>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YamlDiagnostic {
    pub path: String,
    pub message: String,
    pub line: Option<usize>,
    pub range: Option<SourceRange>,
}

/// A half-open character range in the YAML source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRange {
    pub start: usize,
    pub end: usize,
}

impl YamlDiagnostic {
    pub fn at_path(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
            line: None,
            range: None,
        }
    }

    pub fn locate_in(mut self, yaml: &str) -> Self {
        self.range = self
            .range
            .or_else(|| SourceRange::at_yaml_path(yaml, &self.path));
        self.line = self.range.as_ref().map(|range| {
            yaml.chars()
                .take(range.start)
                .filter(|character| *character == '\n')
                .count()
                + 1
        });
        self
    }
}

impl SourceRange {
    pub fn at_yaml_path(source: &str, path: &str) -> Option<Self> {
        yaml_path_range(source, path)
    }

    pub fn at_yaml_location(source: &str, line: usize, column: usize) -> Option<Self> {
        let start = character_index_at_location(source, line, column)?;
        let end = (start + 1).min(source.chars().count());
        Some(Self {
            start,
            end: end.max(start),
        })
    }
}

/// Converts Kubernetes' dotted field-path notation into an RFC 6901 JSON pointer.
///
/// API status causes use paths such as `spec.template.spec.containers[0].image`, while the
/// editor's YAML source mapper operates on JSON pointers.
pub fn kubernetes_field_path_to_json_pointer(field: &str) -> Option<String> {
    let mut segments = Vec::new();
    let mut cursor = 0;
    let bytes = field.as_bytes();

    while cursor < bytes.len() {
        if bytes[cursor] == b'.' {
            return None;
        }
        let start = cursor;
        while cursor < bytes.len() && !matches!(bytes[cursor], b'.' | b'[') {
            cursor += 1;
        }
        if start != cursor {
            segments.push(field[start..cursor].to_owned());
        }
        while cursor < bytes.len() && bytes[cursor] == b'[' {
            cursor += 1;
            let start = cursor;
            while cursor < bytes.len() && bytes[cursor] != b']' {
                cursor += 1;
            }
            if start == cursor || cursor == bytes.len() {
                return None;
            }
            let segment = field[start..cursor].trim_matches('\'').trim_matches('\"');
            if segment.is_empty() {
                return None;
            }
            segments.push(segment.to_owned());
            cursor += 1;
        }
        if cursor == bytes.len() {
            break;
        }
        if bytes[cursor] != b'.' {
            return None;
        }
        cursor += 1;
        if cursor == bytes.len() {
            return None;
        }
    }

    (!segments.is_empty()).then(|| {
        segments
            .into_iter()
            .fold(String::new(), |mut pointer, segment| {
                pointer.push('/');
                pointer.push_str(&segment.replace('~', "~0").replace('/', "~1"));
                pointer
            })
    })
}

/// A resource root schema in JSON Schema form. It is intentionally kept as JSON so the UI and
/// worker can exchange it without exposing Kubernetes' generated schema types to the editor.
#[derive(Debug, Clone)]
pub struct ResourceSchema {
    root: Value,
}

impl ResourceSchema {
    pub fn new(root: Value) -> Self {
        Self { root }
    }

    pub fn from_openapi_document(document: Value, api_resource: &ApiResource) -> Option<Self> {
        let schemas = document.pointer("/components/schemas")?.as_object()?;
        let group = if api_resource.group == "core" {
            ""
        } else {
            &api_resource.group
        };
        let name = schemas.iter().find_map(|(name, schema)| {
            schema
                .get("x-kubernetes-group-version-kind")
                .and_then(Value::as_array)
                .is_some_and(|gvks| {
                    gvks.iter().any(|gvk| {
                        gvk.get("group").and_then(Value::as_str) == Some(group)
                            && gvk.get("version").and_then(Value::as_str)
                                == Some(api_resource.version.as_str())
                            && gvk.get("kind").and_then(Value::as_str)
                                == Some(api_resource.kind.as_str())
                    })
                })
                .then_some(name)
        })?;

        // Keep the components with the root reference. The validator can then resolve every
        // local reference without reaching outside the document.
        Some(Self::new(serde_json::json!({
            "$ref": format!("#/components/schemas/{name}"),
            "components": { "schemas": schemas },
        })))
    }

    pub fn validate_yaml(&self, yaml: &str) -> Result<Vec<YamlDiagnostic>, String> {
        let yaml_value: serde_yaml::Value = serde_yaml::from_str(yaml).map_err(|error| {
            error
                .location()
                .map(|location| format!("{} at {}:{}", error, location.line(), location.column()))
                .unwrap_or_else(|| error.to_string())
        })?;
        let value = serde_json::to_value(yaml_value)
            .map_err(|error| format!("YAML must use string mapping keys: {error}"))?;
        let validator = jsonschema::validator_for(&self.root)
            .map_err(|error| format!("Unable to compile the Kubernetes schema: {error}"))?;
        Ok(validator
            .iter_errors(&value)
            .map(|error| {
                YamlDiagnostic::at_path(error.instance_path.to_string(), error.to_string())
                    .locate_in(yaml)
            })
            .collect())
    }

    pub fn completion_at(&self, yaml: &str, cursor: usize) -> CompletionResult {
        let context = yaml_context(yaml, cursor);
        let mut schema_path = context.path.clone();
        if let Some(key) = &context.value_key {
            schema_path.push(key.clone());
        }
        let Some(mut schema) = self.resolve_path(&schema_path) else {
            return CompletionResult::default();
        };
        while schema.get("type").and_then(Value::as_str) == Some("array") {
            let Some(items) = schema.get("items") else {
                break;
            };
            let Some(items) = resolve_ref(&self.root, items) else {
                break;
            };
            schema = items;
        }

        let completion_context = CompletionContext {
            kind: if context.is_value {
                CompletionContextKind::Value
            } else {
                CompletionContextKind::MappingKey
            },
            type_label: if context.is_value {
                schema.get("type").and_then(Value::as_str)
            } else {
                schema
                    .get("additionalProperties")
                    .and_then(|properties| properties.get("type"))
                    .and_then(Value::as_str)
                    .or_else(|| schema.get("type").and_then(Value::as_str))
            }
            .map(ToOwned::to_owned),
            description: schema
                .get("description")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
        };

        let existing = keys_at_indent(yaml, context.line_start, context.indent);
        let prefix = context.prefix;
        let properties = schema.get("properties").and_then(Value::as_object);

        if context.is_value {
            let suggestions = value_suggestions(schema);
            return CompletionResult {
                suggestions: filter_suggestions(suggestions, &prefix),
                context: Some(completion_context),
            };
        }

        let suggestions = properties
            .into_iter()
            .flatten()
            .filter(|(key, _)| !existing.iter().any(|existing_key| existing_key == *key))
            .map(|(key, property)| CompletionSuggestion {
                label: key.clone(),
                type_label: property
                    .get("type")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
                detail: property
                    .get("description")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
            })
            .collect();
        CompletionResult {
            suggestions: filter_suggestions(suggestions, &prefix),
            context: Some(completion_context),
        }
    }

    fn resolve_path(&self, path: &[String]) -> Option<&Value> {
        let mut schema = &self.root;
        for segment in path {
            schema = resolve_ref(&self.root, schema)?;
            while schema.get("type").and_then(Value::as_str) == Some("array") {
                schema = schema.get("items")?;
                schema = resolve_ref(&self.root, schema)?;
            }
            schema = schema.get("properties")?.get(segment)?;
        }
        resolve_ref(&self.root, schema)
    }
}

fn value_suggestions(schema: &Value) -> Vec<CompletionSuggestion> {
    let explicit_values = schema
        .get("enum")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|value| CompletionSuggestion {
            label: value.to_owned(),
            type_label: Some("enum".into()),
            detail: None,
        })
        .collect::<Vec<_>>();
    if !explicit_values.is_empty() {
        return explicit_values;
    }

    // Kubernetes' OpenAPI leaves some string aliases without an `enum`. Its descriptions still
    // spell out their closed value set, notably LabelSelectorRequirement.operator. Use only the
    // conventional "Valid ... are A, B and C" wording so ordinary prose cannot become a
    // completion source.
    documented_values(schema)
        .into_iter()
        .map(|value| CompletionSuggestion {
            label: value,
            type_label: Some("documented value".into()),
            detail: None,
        })
        .collect()
}

fn documented_values(schema: &Value) -> Vec<String> {
    let Some(description) = schema.get("description").and_then(Value::as_str) else {
        return Vec::new();
    };
    let Some(values) = description
        .split_once("Valid operators are ")
        .map(|(_, values)| values)
        .or_else(|| {
            description
                .split_once("Valid values are ")
                .map(|(_, values)| values)
        })
    else {
        return Vec::new();
    };

    values
        .split_once('.')
        .map_or(values, |(values, _)| values)
        .replace(" and ", ",")
        .split(',')
        .map(|value| value.trim().trim_matches(['`', '\'', '"']))
        .filter(|value| {
            !value.is_empty()
                && value.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
                })
        })
        .map(ToOwned::to_owned)
        .collect()
}

fn filter_suggestions(
    suggestions: Vec<CompletionSuggestion>,
    prefix: &str,
) -> Vec<CompletionSuggestion> {
    let normalized_prefix: String = normalize_for_search(prefix).collect();
    let needle = normalized_prefix.chars().collect::<Vec<_>>();

    let mut matches = suggestions
        .into_iter()
        .enumerate()
        .filter(|(_, suggestion)| matches_fuzzy(&suggestion.label, &needle))
        .collect::<Vec<_>>();
    matches.sort_by_key(|(index, suggestion)| {
        (
            fuzzy_match_rank(&suggestion.label, &normalized_prefix),
            suggestion.label.len(),
            *index,
        )
    });
    matches
        .into_iter()
        .map(|(_, suggestion)| suggestion)
        .collect()
}

fn fuzzy_match_rank(label: &str, normalized_prefix: &str) -> u8 {
    let normalized_label: String = normalize_for_search(label).collect();
    if normalized_label == normalized_prefix {
        0
    } else if normalized_label.starts_with(normalized_prefix) {
        1
    } else if normalized_label.contains(normalized_prefix) {
        2
    } else {
        3
    }
}

fn resolve_ref<'a>(root: &'a Value, value: &'a Value) -> Option<&'a Value> {
    let value = value
        .get("$ref")
        .and_then(Value::as_str)
        .map_or(Some(value), |reference| {
            root.pointer(reference.strip_prefix('#').unwrap_or(reference))
        })?;
    // Kubernetes OpenAPI v3 expresses many typed fields as a one-item `allOf` wrapping a
    // local reference (for example DeploymentSpec.selector -> LabelSelector). Treat that
    // wrapper as transparent for schema traversal.
    if let Some(all_of) = value.get("allOf").and_then(Value::as_array)
        && let [schema] = all_of.as_slice()
    {
        return resolve_ref(root, schema);
    }
    Some(value)
}

mod yaml_context;
use yaml_context::*;

#[cfg(test)]
mod tests;
