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
pub struct SchemaDiagnostic {
    pub path: String,
    pub message: String,
    pub line: Option<usize>,
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

    pub fn validate_yaml(&self, yaml: &str) -> Result<Vec<SchemaDiagnostic>, String> {
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
            .map(|error| SchemaDiagnostic {
                path: error.instance_path.to_string(),
                message: error.to_string(),
                line: None,
            })
            .collect())
    }

    pub fn suggestions_at(&self, yaml: &str, cursor: usize) -> Vec<CompletionSuggestion> {
        let context = yaml_context(yaml, cursor);
        let mut schema_path = context.path.clone();
        if let Some(key) = &context.value_key {
            schema_path.push(key.clone());
        }
        let Some(mut schema) = self.resolve_path(&schema_path) else {
            return Vec::new();
        };
        while schema.get("type").and_then(Value::as_str) == Some("array") {
            let Some(items) = schema.get("items") else {
                break;
            };
            schema = items;
        }

        let existing = keys_at_indent(yaml, context.line_start, context.indent);
        let prefix = context.prefix;
        let properties = schema.get("properties").and_then(Value::as_object);
        let enum_values = schema.get("enum").and_then(Value::as_array);

        if context.is_value {
            let suggestions = enum_values
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(|value| CompletionSuggestion {
                    label: value.to_owned(),
                    type_label: Some("enum".into()),
                    detail: Some("allowed value".into()),
                })
                .collect();
            return filter_suggestions(suggestions, &prefix);
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
        filter_suggestions(suggestions, &prefix)
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
    value
        .get("$ref")
        .and_then(Value::as_str)
        .map_or(Some(value), |reference| root.pointer(reference))
}

struct YamlContext {
    path: Vec<String>,
    prefix: String,
    value_key: Option<String>,
    line_start: usize,
    indent: usize,
    is_value: bool,
}

fn yaml_context(yaml: &str, cursor: usize) -> YamlContext {
    let cursor = cursor.min(yaml.len());
    let location = source_location(yaml, cursor);
    if let Ok(documents) = MarkedYaml::load_from_str(yaml) {
        for document in &documents {
            if let Some(mut context) = context_in_node(document, location) {
                context.prefix = token_before_cursor(yaml, cursor);
                return context;
            }
        }
    }
    fallback_yaml_context(yaml, cursor)
}

fn context_in_node(node: &MarkedYaml<'_>, cursor: (usize, usize)) -> Option<YamlContext> {
    if !span_contains(node, cursor) {
        return None;
    }
    match &node.data {
        YamlData::Mapping(mapping) => {
            for (key, value) in mapping {
                let key_text = scalar_text(key)?;
                if span_contains(key, cursor) {
                    return Some(YamlContext::keys());
                }
                if span_contains(value, cursor) {
                    if matches!(
                        value.data,
                        YamlData::Value(_) | YamlData::Representation(..)
                    ) {
                        return Some(YamlContext::value(key_text));
                    }
                    if let Some(mut nested) = context_in_node(value, cursor) {
                        nested.path.insert(0, key_text);
                        return Some(nested);
                    }
                    let mut context = YamlContext::keys();
                    context.path.push(key_text);
                    return Some(context);
                }
            }
            Some(YamlContext::keys())
        }
        YamlData::Sequence(sequence) => sequence
            .iter()
            .find_map(|item| context_in_node(item, cursor))
            .or_else(|| Some(YamlContext::keys())),
        YamlData::Tagged(_, node) => context_in_node(node, cursor),
        _ => Some(YamlContext::keys()),
    }
}

fn scalar_text(node: &MarkedYaml<'_>) -> Option<String> {
    match &node.data {
        YamlData::Representation(value, ..) => Some(value.to_string()),
        YamlData::Value(Scalar::String(value)) => Some(value.to_string()),
        _ => None,
    }
}

fn span_contains(node: &MarkedYaml<'_>, cursor: (usize, usize)) -> bool {
    let start = (node.span.start.line(), node.span.start.col());
    // Saphyr spans end at the first character after a node. Keep the insertion caret at
    // that boundary associated with the node so completion works after the final character.
    let end = (node.span.end.line(), node.span.end.col().saturating_add(1));
    start <= cursor && cursor <= end
}

fn source_location(source: &str, byte_index: usize) -> (usize, usize) {
    let prefix = &source[..byte_index];
    let line = prefix.lines().count().max(1);
    let column = prefix
        .rsplit('\n')
        .next()
        .unwrap_or_default()
        .chars()
        .count()
        + 1;
    (line, column)
}

fn token_before_cursor(source: &str, cursor: usize) -> String {
    source[..cursor]
        .chars()
        .rev()
        .take_while(|character| character.is_alphanumeric() || matches!(character, '-' | '_'))
        .collect::<String>()
        .chars()
        .rev()
        .collect()
}

fn fallback_yaml_context(yaml: &str, cursor: usize) -> YamlContext {
    let cursor = cursor.min(yaml.len());
    let line_start = yaml[..cursor].rfind('\n').map_or(0, |index| index + 1);
    let line = &yaml[line_start..cursor];
    let indent = line.len() - line.trim_start().len();
    let before_cursor = &line[indent..];
    let (is_value, prefix, value_key) = before_cursor
        .split_once(':')
        .map(|(key, value)| (true, value.trim().to_owned(), Some(key.trim().to_owned())))
        .unwrap_or((
            false,
            before_cursor.trim_start_matches("- ").to_owned(),
            None,
        ));

    let mut path = Vec::<(usize, String)>::new();
    for source_line in yaml[..line_start].lines() {
        let indentation = source_line.len() - source_line.trim_start().len();
        let trimmed = source_line.trim_start().trim_start_matches("- ");
        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };
        if key.is_empty() || !value.trim().is_empty() {
            continue;
        }
        while path
            .last()
            .is_some_and(|(previous_indent, _)| *previous_indent >= indentation)
        {
            path.pop();
        }
        path.push((indentation, key.trim().to_owned()));
    }
    while path
        .last()
        .is_some_and(|(previous_indent, _)| *previous_indent >= indent)
    {
        path.pop();
    }

    YamlContext {
        path: path.into_iter().map(|(_, key)| key).collect(),
        prefix,
        value_key,
        line_start,
        indent,
        is_value,
    }
}

impl YamlContext {
    fn keys() -> Self {
        Self {
            path: Vec::new(),
            prefix: String::new(),
            value_key: None,
            line_start: 0,
            indent: 0,
            is_value: false,
        }
    }

    fn value(key: String) -> Self {
        Self {
            value_key: Some(key),
            is_value: true,
            ..Self::keys()
        }
    }
}

fn keys_at_indent(yaml: &str, before_line: usize, indent: usize) -> Vec<String> {
    yaml[..before_line]
        .lines()
        .filter_map(|line| {
            let indentation = line.len() - line.trim_start().len();
            (indentation == indent)
                .then(|| line.trim_start().trim_start_matches("- ").split_once(':'))
                .flatten()
                .map(|(key, _)| key.trim().to_owned())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::ResourceSchema;
    use crate::api_resource::ApiResource;
    use k8s_openapi::serde_json::json;

    #[test]
    fn extracts_the_matching_openapi_resource_schema() {
        let schema = ResourceSchema::from_openapi_document(
            json!({"components": {"schemas": {"Deployment": {
                "x-kubernetes-group-version-kind": [{"group": "apps", "version": "v1", "kind": "Deployment"}],
                "type": "object"
            }}}}),
            &ApiResource {
                group: "apps".into(),
                version: "v1".into(),
                kind: "Deployment".into(),
                name: "deployments".into(),
                namespaced: true,
            },
        );
        assert!(schema.is_some());
    }

    #[test]
    fn reports_declared_schema_errors() {
        let schema = ResourceSchema::new(json!({"type": "object", "required": ["kind"]}));
        let errors = schema.validate_yaml("apiVersion: v1").expect("YAML parses");
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "");
    }

    #[test]
    fn validates_a_schema_root_referenced_from_openapi_components() {
        let resource = ApiResource {
            group: "apps".into(),
            version: "v1".into(),
            kind: "Deployment".into(),
            name: "deployments".into(),
            namespaced: true,
        };
        let schema = ResourceSchema::from_openapi_document(
            json!({"components": {"schemas": {"Deployment": {
                "x-kubernetes-group-version-kind": [{"group": "apps", "version": "v1", "kind": "Deployment"}],
                "type": "object", "required": ["spec"]
            }}}}),
            &resource,
        ).expect("matches the resource");
        assert_eq!(
            schema
                .validate_yaml("metadata: {} ")
                .expect("YAML parses")
                .len(),
            1
        );
    }

    #[test]
    fn suggests_mapping_keys_and_enum_values_from_the_current_schema_node() {
        let schema = ResourceSchema::new(json!({
            "type": "object",
            "properties": {
                "metadata": {"type": "object"},
                "mode": {"type": "string", "enum": ["ReadOnly", "ReadWrite"]}
            }
        }));
        assert_eq!(schema.suggestions_at("met", 3)[0].label, "metadata");
        assert_eq!(schema.suggestions_at("mode: Read", 10)[0].label, "ReadOnly");
    }

    #[test]
    fn filters_and_ranks_suggestions_using_the_shared_fuzzy_matcher() {
        let schema = ResourceSchema::new(json!({
            "type": "object",
            "properties": {
                "xmetadata": {"type": "object"},
                "metadata": {"type": "object"},
                "managedFields": {"type": "array"}
            }
        }));

        let labels = schema
            .suggestions_at("mta", 3)
            .into_iter()
            .map(|suggestion| suggestion.label)
            .collect::<Vec<_>>();
        assert_eq!(labels, vec!["metadata", "xmetadata"]);

        let labels = schema
            .suggestions_at("meta", 4)
            .into_iter()
            .map(|suggestion| suggestion.label)
            .collect::<Vec<_>>();
        assert_eq!(labels, vec!["metadata", "xmetadata"]);
    }

    #[test]
    fn completion_context_uses_yaml_spans_for_flow_block_alias_and_multidocument_syntax() {
        let nested_schema = ResourceSchema::new(json!({
            "type": "object",
            "properties": {
                "spec": {"type": "object", "properties": {
                    "mode": {"type": "string", "enum": ["ReadOnly", "ReadWrite"]}
                }}
            }
        }));
        for document in [
            "spec: { mode: Read }",
            "note: \"mode: Ignore\"\nspec:\n  mode: Read",
            "spec:\n  description: |\n    mode: Ignore\n  mode: Read",
            "defaults: &defaults\n  mode: Ignore\nspec:\n  <<: *defaults\n  mode: Read",
        ] {
            let cursor = document.rfind("Read").expect("test cursor") + "Read".len();
            assert_eq!(
                nested_schema.suggestions_at(document, cursor)[0].label,
                "ReadOnly",
                "completion should target the final mode scalar in {document:?}",
            );
        }

        let root_schema = ResourceSchema::new(json!({
            "type": "object",
            "properties": {"mode": {"type": "string", "enum": ["ReadOnly"]}}
        }));
        let document = "---\nmode: Ignore\n---\nmode: Read";
        assert_eq!(
            root_schema.suggestions_at(document, document.len())[0].label,
            "ReadOnly"
        );
    }

    #[test]
    fn completion_resolves_deeply_nested_mappings_and_array_items() {
        let schema = ResourceSchema::new(json!({
            "type": "object",
            "properties": {
                "spec": {"type": "object", "properties": {
                    "template": {"type": "object", "properties": {
                        "spec": {"type": "object", "properties": {
                            "mode": {"type": "string", "enum": ["Always", "Never"]}
                        }}
                    }},
                    "templates": {"type": "array", "items": {"type": "object", "properties": {
                        "spec": {"type": "object", "properties": {
                            "containers": {"type": "array", "items": {"type": "object", "properties": {
                                "name": {"type": "string"},
                                "imagePullPolicy": {"type": "string", "enum": ["Always", "IfNotPresent", "Never"]}
                            }}}
                        }}
                    }}}
                }}
            }
        }));

        let nested_value = "spec:\n  template:\n    spec:\n      mode: Al";
        assert_eq!(
            schema.suggestions_at(nested_value, nested_value.len())[0].label,
            "Always"
        );

        let array_value = "spec:\n  templates:\n    - spec:\n        containers:\n          - imagePullPolicy: Al";
        assert_eq!(
            schema.suggestions_at(array_value, array_value.len())[0].label,
            "Always"
        );

        let array_key = "spec:\n  templates:\n    - spec:\n        containers:\n          - na";
        assert_eq!(
            schema.suggestions_at(array_key, array_key.len())[0].label,
            "name"
        );

        let comment_between_nodes =
            "spec:\n  template:\n    # select the runtime mode\n    spec:\n      mode: Al";
        assert_eq!(
            schema.suggestions_at(comment_between_nodes, comment_between_nodes.len())[0].label,
            "Always"
        );

        let partial_sibling_key = "spec:\n  templates:\n    - spec:\n        containers:\n          - name: api\n            im";
        let suggestions = schema.suggestions_at(partial_sibling_key, partial_sibling_key.len());
        assert_eq!(suggestions[0].label, "imagePullPolicy");
        assert!(
            suggestions
                .iter()
                .all(|suggestion| suggestion.label != "name")
        );
    }
}
