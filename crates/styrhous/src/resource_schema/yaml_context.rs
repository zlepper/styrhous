use super::*;

pub(super) struct YamlContext {
    pub(super) path: Vec<String>,
    pub(super) prefix: String,
    pub(super) value_key: Option<String>,
    pub(super) line_start: usize,
    pub(super) indent: usize,
    pub(super) is_value: bool,
}

pub(super) fn yaml_context(yaml: &str, cursor: usize) -> YamlContext {
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

pub(super) fn is_bare_mapping_key_prefix(yaml: &str, cursor: usize) -> bool {
    let line_start = yaml[..cursor].rfind('\n').map_or(0, |index| index + 1);
    let line = yaml[line_start..cursor]
        .trim_start()
        .strip_prefix("- ")
        .unwrap_or(yaml[line_start..cursor].trim_start());
    !line.is_empty() && !line.contains(':')
}

pub(super) fn context_in_node(
    node: &MarkedYaml<'_>,
    cursor: (usize, usize),
) -> Option<YamlContext> {
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

pub(super) fn scalar_text(node: &MarkedYaml<'_>) -> Option<String> {
    match &node.data {
        YamlData::Representation(value, ..) => Some(value.to_string()),
        YamlData::Value(Scalar::String(value)) => Some(value.to_string()),
        _ => None,
    }
}

pub(super) fn span_contains(node: &MarkedYaml<'_>, cursor: (usize, usize)) -> bool {
    let start = (node.span.start.line(), node.span.start.col());
    // Saphyr spans end at the first character after a node. Keep the insertion caret at
    // that boundary associated with the node so completion works after the final character.
    let end = (node.span.end.line(), node.span.end.col().saturating_add(1));
    start <= cursor && cursor <= end
}

pub(super) fn source_location(source: &str, byte_index: usize) -> (usize, usize) {
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

pub(super) fn yaml_path_range(source: &str, path: &str) -> Option<SourceRange> {
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

pub(super) fn yaml_node_at_path<'a>(
    node: &'a MarkedYaml<'a>,
    path: &[String],
) -> Option<&'a MarkedYaml<'a>> {
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

pub(super) fn source_range_from_node(source: &str, node: &MarkedYaml<'_>) -> Option<SourceRange> {
    let start =
        character_index_at_saphyr_location(source, node.span.start.line(), node.span.start.col())?;
    let end =
        character_index_at_saphyr_location(source, node.span.end.line(), node.span.end.col())?;
    Some(SourceRange {
        start,
        end: end.max(start + 1).min(source.chars().count()),
    })
}

pub(super) fn character_index_at_saphyr_location(
    source: &str,
    line: usize,
    column: usize,
) -> Option<usize> {
    let line_start = source
        .split_inclusive('\n')
        .take(line.saturating_sub(1))
        .map(str::chars)
        .map(Iterator::count)
        .sum::<usize>();
    let source_line = source.lines().nth(line.saturating_sub(1))?;
    Some(line_start + column.min(source_line.chars().count()))
}

pub(super) fn character_index_at_location(
    source: &str,
    line: usize,
    column: usize,
) -> Option<usize> {
    let line_start = source
        .split_inclusive('\n')
        .take(line.saturating_sub(1))
        .map(str::chars)
        .map(Iterator::count)
        .sum::<usize>();
    let source_line = source.lines().nth(line.saturating_sub(1))?;
    Some(line_start + column.saturating_sub(1).min(source_line.chars().count()))
}

pub(super) fn token_before_cursor(source: &str, cursor: usize) -> String {
    source[..cursor]
        .chars()
        .rev()
        .take_while(|character| character.is_alphanumeric() || matches!(character, '-' | '_'))
        .collect::<String>()
        .chars()
        .rev()
        .collect()
}

pub(super) fn fallback_yaml_context(yaml: &str, cursor: usize) -> YamlContext {
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
    pub(super) fn keys() -> Self {
        Self {
            path: Vec::new(),
            prefix: String::new(),
            value_key: None,
            line_start: 0,
            indent: 0,
            is_value: false,
        }
    }

    pub(super) fn value(key: String) -> Self {
        Self {
            value_key: Some(key),
            is_value: true,
            ..Self::keys()
        }
    }
}

pub(super) fn keys_at_indent(yaml: &str, before_line: usize, indent: usize) -> Vec<String> {
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
