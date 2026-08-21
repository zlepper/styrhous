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

    let array_value =
        "spec:\n  templates:\n    - spec:\n        containers:\n          - imagePullPolicy: Al";
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
