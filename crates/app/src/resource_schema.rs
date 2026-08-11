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
            kind: context
                .is_value
                .then_some(CompletionContextKind::Value)
                .unwrap_or(CompletionContextKind::MappingKey),
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
    // A partially typed mapping key (`match` before its colon is entered) is still valid YAML:
    // Saphyr represents it as the scalar value of the preceding mapping entry. In an editor,
    // though, it is a key prefix and must inherit that mapping's schema path.
    if is_bare_mapping_key_prefix(yaml, cursor) {
        return fallback_yaml_context(yaml, cursor);
    }
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

fn is_bare_mapping_key_prefix(yaml: &str, cursor: usize) -> bool {
    let line_start = yaml[..cursor].rfind('\n').map_or(0, |index| index + 1);
    let line = yaml[line_start..cursor]
        .trim_start()
        .strip_prefix("- ")
        .unwrap_or(yaml[line_start..cursor].trim_start());
    !line.is_empty() && !line.contains(':')
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

fn yaml_path_range(source: &str, path: &str) -> Option<SourceRange> {
    let segments = path
        .strip_prefix('/')?
        .split('/')
        .map(|segment| segment.replace("~1", "/").replace("~0", "~"))
        .collect::<Vec<_>>();
    let documents = MarkedYaml::load_from_str(source).ok()?;
    documents.into_iter().find_map(|document| {
        yaml_node_at_path(&document, &segments)
            .and_then(|node| source_range_from_node(source, node))
    })
}

fn yaml_node_at_path<'a>(node: &'a MarkedYaml<'a>, path: &[String]) -> Option<&'a MarkedYaml<'a>> {
    let Some((segment, rest)) = path.split_first() else {
        return Some(node);
    };
    match &node.data {
        YamlData::Mapping(mapping) => mapping
            .iter()
            .find(|(key, _)| scalar_text(key).as_deref() == Some(segment))
            .and_then(|(_, value)| yaml_node_at_path(value, rest)),
        YamlData::Sequence(sequence) => sequence
            .get(segment.parse::<usize>().ok()?)
            .and_then(|value| yaml_node_at_path(value, rest)),
        YamlData::Tagged(_, node) => yaml_node_at_path(node, path),
        _ => None,
    }
}

fn source_range_from_node(source: &str, node: &MarkedYaml<'_>) -> Option<SourceRange> {
    let start =
        character_index_at_saphyr_location(source, node.span.start.line(), node.span.start.col())?;
    let end =
        character_index_at_saphyr_location(source, node.span.end.line(), node.span.end.col())?;
    Some(SourceRange {
        start,
        end: end.max(start + 1).min(source.chars().count()),
    })
}

fn character_index_at_saphyr_location(source: &str, line: usize, column: usize) -> Option<usize> {
    let line_start = source
        .split_inclusive('\n')
        .take(line.saturating_sub(1))
        .map(str::chars)
        .map(Iterator::count)
        .sum::<usize>();
    let source_line = source.lines().nth(line.saturating_sub(1))?;
    Some(line_start + column.min(source_line.chars().count()))
}

fn character_index_at_location(source: &str, line: usize, column: usize) -> Option<usize> {
    let line_start = source
        .split_inclusive('\n')
        .take(line.saturating_sub(1))
        .map(str::chars)
        .map(Iterator::count)
        .sum::<usize>();
    let source_line = source.lines().nth(line.saturating_sub(1))?;
    Some(line_start + column.saturating_sub(1).min(source_line.chars().count()))
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
    let source_indent = line.len() - line.trim_start().len();
    let (indent, before_cursor) = line[source_indent..]
        .strip_prefix("- ")
        .map_or((source_indent, &line[source_indent..]), |mapping| {
            (source_indent + 2, mapping)
        });
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
        let source_indentation = source_line.len() - source_line.trim_start().len();
        let (indentation, trimmed) = source_line[source_indentation..]
            .strip_prefix("- ")
            // A sequence item's mapping key begins after its dash. Keeping that extra two
            // columns preserves the array property's place in the enclosing schema path.
            .map_or(
                (source_indentation, &source_line[source_indentation..]),
                |trimmed| (source_indentation + 2, trimmed),
            );
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
    let current_is_sequence_item = yaml[before_line..]
        .lines()
        .next()
        .is_some_and(|line| line.trim_start().starts_with("- "));
    if current_is_sequence_item {
        return Vec::new();
    }
    let mut keys = Vec::new();
    let mut found_sequence_item = false;
    for line in yaml[..before_line].lines().rev() {
        let line_indent = line.len() - line.trim_start().len();
        let line_after_indent = &line[line_indent..];
        let sequence_mapping = line_after_indent.strip_prefix("- ");
        let effective_indent = sequence_mapping.map_or(line_indent, |_| line_indent + 2);

        if sequence_mapping.is_some() && effective_indent == indent {
            if found_sequence_item {
                break;
            }
            found_sequence_item = true;
        } else if line_indent < indent {
            break;
        }

        if effective_indent == indent
            && let Some((key, _)) = sequence_mapping
                .unwrap_or(line_after_indent)
                .split_once(':')
        {
            keys.push(key.trim().to_owned());
        }
        if found_sequence_item {
            break;
        }
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::{
        CompletionContextKind, ResourceSchema, kubernetes_field_path_to_json_pointer, yaml_context,
    };
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
    fn converts_kubernetes_field_paths_to_json_pointers() {
        assert_eq!(
            kubernetes_field_path_to_json_pointer("spec.template.spec.containers[0].image"),
            Some("/spec/template/spec/containers/0/image".into())
        );
        assert_eq!(
            kubernetes_field_path_to_json_pointer("metadata.labels[app.kubernetes.io/name]"),
            Some("/metadata/labels/app.kubernetes.io~1name".into())
        );
        assert_eq!(
            kubernetes_field_path_to_json_pointer("spec..replicas"),
            None
        );
    }

    #[test]
    fn reports_declared_schema_errors() {
        let schema = ResourceSchema::new(json!({"type": "object", "required": ["kind"]}));
        let errors = schema.validate_yaml("apiVersion: v1").expect("YAML parses");
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "");
    }

    #[test]
    fn local_validation_preserves_the_exact_invalid_scalar_range() {
        let yaml = "mode: unsupported";
        let schema = ResourceSchema::new(json!({
            "type": "object",
            "properties": {"mode": {"type": "string", "enum": ["ReadOnly"]}}
        }));

        let errors = schema.validate_yaml(yaml).expect("YAML parses");
        let range = errors[0].range.as_ref().expect("range is available");
        let highlighted = yaml
            .chars()
            .skip(range.start)
            .take(range.end - range.start)
            .collect::<String>();

        assert_eq!(highlighted, "unsupported");
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
        assert_eq!(
            schema.completion_at("met", 3).suggestions[0].label,
            "metadata"
        );
        assert_eq!(
            schema.completion_at("mode: Read", 10).suggestions[0].label,
            "ReadOnly"
        );
        let completion = schema.completion_at("mode: Read", 10);
        assert_eq!(
            completion.context.as_ref().map(|context| context.kind),
            Some(CompletionContextKind::Value)
        );
        assert_eq!(
            completion
                .context
                .as_ref()
                .and_then(|context| context.type_label.as_deref()),
            Some("string")
        );
    }

    #[test]
    fn suggests_documented_kubernetes_value_sets_when_openapi_omits_an_enum() {
        let schema = ResourceSchema::new(json!({
            "type": "object",
            "properties": {
                "operator": {
                    "type": "string",
                    "description": "operator represents a key's relationship with a set of values. Valid operators are In, NotIn, Exists and DoesNotExist."
                }
            }
        }));

        let suggestions = schema
            .completion_at("operator: I", "operator: I".len())
            .suggestions
            .into_iter()
            .map(|suggestion| (suggestion.label, suggestion.detail))
            .collect::<Vec<_>>();
        assert_eq!(
            suggestions,
            vec![
                ("In".into(), None),
                ("NotIn".into(), None),
                ("Exists".into(), None),
                ("DoesNotExist".into(), None),
            ]
        );
    }

    #[test]
    fn completion_context_tracks_deeply_nested_sequence_values() {
        let yaml = r#"apiVersion: apps/v1
kind: Deployment
metadata:
  name: deployment-completion
spec:
  affinity:
    podAntiAffinity:
      preferredDuringSchedulingIgnoredDuringExecution:
      - podAffinityTerm:
          labelSelector:
            matchExpressions:
            - key: k8s-app
              operator: I"#;

        let context = yaml_context(yaml, yaml.len());
        assert_eq!(
            context.path,
            vec![
                "spec",
                "affinity",
                "podAntiAffinity",
                "preferredDuringSchedulingIgnoredDuringExecution",
                "podAffinityTerm",
                "labelSelector",
                "matchExpressions",
            ]
        );
        assert_eq!(context.value_key.as_deref(), Some("operator"));
        assert!(context.is_value);
    }

    #[test]
    fn completion_context_tracks_mapping_keys_inside_deeply_nested_sequences() {
        let yaml = r#"spec:
  template:
    spec:
      affinity:
        podAntiAffinity:
          preferredDuringSchedulingIgnoredDuringExecution:
          - podAffinityTerm:
              labelSelector: {}"#;
        let cursor = yaml.find("podAffinityTerm").expect("key exists") + "podAffinityTerm".len();

        let context = yaml_context(yaml, cursor);
        assert_eq!(
            context.path,
            vec![
                "spec",
                "template",
                "spec",
                "affinity",
                "podAntiAffinity",
                "preferredDuringSchedulingIgnoredDuringExecution",
            ]
        );
        assert!(!context.is_value);
    }

    #[test]
    fn partial_mapping_keys_keep_their_enclosing_sequence_path() {
        let yaml = r#"spec:
  template:
    spec:
      affinity:
        podAntiAffinity:
          preferredDuringSchedulingIgnoredDuringExecution:
          - podAffinityTerm:
              labelSelector:
                matchExpressions:
                - key: k8s-app
                  oper"#;

        let context = yaml_context(yaml, yaml.len());
        assert_eq!(
            context.path,
            vec![
                "spec",
                "template",
                "spec",
                "affinity",
                "podAntiAffinity",
                "preferredDuringSchedulingIgnoredDuringExecution",
                "podAffinityTerm",
                "labelSelector",
                "matchExpressions",
            ]
        );
        assert!(!context.is_value);
    }

    #[test]
    fn suggests_keys_from_array_items_wrapped_in_openapi_all_of_references() {
        let schema = ResourceSchema::new(json!({
            "$ref": "#/components/schemas/Root",
            "components": {"schemas": {
                "Root": {"type": "object", "properties": {
                    "terms": {"type": "array", "items": {"allOf": [
                        {"$ref": "#/components/schemas/Term"}
                    ]}}
                }},
                "Term": {"type": "object", "properties": {
                    "operator": {"type": "string"}
                }}
            }}
        }));
        let yaml = "terms:\n- oper";

        assert_eq!(
            schema.completion_at(yaml, yaml.len()).suggestions[0].label,
            "operator"
        );
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
            .completion_at("mta", 3)
            .suggestions
            .into_iter()
            .map(|suggestion| suggestion.label)
            .collect::<Vec<_>>();
        assert_eq!(labels, vec!["metadata", "xmetadata"]);

        let labels = schema
            .completion_at("meta", 4)
            .suggestions
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
                nested_schema.completion_at(document, cursor).suggestions[0].label,
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
            root_schema
                .completion_at(document, document.len())
                .suggestions[0]
                .label,
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
            schema
                .completion_at(nested_value, nested_value.len())
                .suggestions[0]
                .label,
            "Always"
        );

        let array_value = "spec:\n  templates:\n    - spec:\n        containers:\n          - imagePullPolicy: Al";
        assert_eq!(
            schema
                .completion_at(array_value, array_value.len())
                .suggestions[0]
                .label,
            "Always"
        );

        let selector_schema = ResourceSchema::new(json!({
            "type": "object",
            "properties": {
                "spec": {"type": "object", "properties": {
                    "selector": {"type": "object", "properties": {
                        "matchLabels": {"type": "object"}
                    }}
                }}
            }
        }));
        let partial_selector_key = "spec:\n  selector:\n    match";
        let labels = selector_schema
            .completion_at(partial_selector_key, partial_selector_key.len())
            .suggestions
            .into_iter()
            .map(|suggestion| suggestion.label)
            .collect::<Vec<_>>();
        assert!(
            labels.iter().any(|label| label == "matchLabels"),
            "a bare partial mapping key must use its enclosing schema path"
        );

        let array_key = "spec:\n  templates:\n    - spec:\n        containers:\n          - na";
        assert_eq!(
            schema.completion_at(array_key, array_key.len()).suggestions[0].label,
            "name"
        );

        let comment_between_nodes =
            "spec:\n  template:\n    # select the runtime mode\n    spec:\n      mode: Al";
        assert_eq!(
            schema
                .completion_at(comment_between_nodes, comment_between_nodes.len())
                .suggestions[0]
                .label,
            "Always"
        );

        let partial_sibling_key = "spec:\n  templates:\n    - spec:\n        containers:\n          - name: api\n            im";
        let suggestions = schema
            .completion_at(partial_sibling_key, partial_sibling_key.len())
            .suggestions;
        assert_eq!(suggestions[0].label, "imagePullPolicy");
        assert!(
            suggestions
                .iter()
                .all(|suggestion| suggestion.label != "name")
        );
    }
}
