use super::*;

/// Get a unique identifier for a resource
pub(crate) fn get_resource_uid<T: Resource>(obj: &T) -> String {
    let metadata = obj.meta();
    metadata.uid.clone().unwrap_or_else(|| {
        format!(
            "{}/{}",
            metadata.namespace.as_deref().unwrap_or(""),
            metadata.name.as_deref().unwrap_or("")
        )
    })
}

pub(crate) fn resource_owners(metadata: &ObjectMeta) -> Vec<ResourceOwner> {
    metadata
        .owner_references
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(|owner| ResourceOwner {
            api_version: owner.api_version.clone(),
            kind: owner.kind.clone(),
            name: owner.name.clone(),
            uid: owner.uid.clone(),
            controller: owner.controller == Some(true),
        })
        .collect()
}

pub(crate) fn controller_owner(metadata: &ObjectMeta) -> Option<ResourceOwner> {
    resource_owners(metadata)
        .into_iter()
        .find(|owner| owner.controller)
}

/// Extract a MinimalResource from a DynamicObject
pub(crate) fn extract_minimal_resource(
    obj: &DynamicObject,
    custom_columns: &[CustomResourceColumn],
) -> MinimalResource {
    let metadata = &obj.metadata;
    let uid = get_resource_uid(obj);

    // Parse creation timestamp
    let creation_timestamp = metadata.creation_timestamp.as_ref().and_then(|ts| {
        OffsetDateTime::parse(
            &ts.0.to_string(),
            &time::format_description::well_known::Rfc3339,
        )
        .ok()
    });

    MinimalResource {
        uid,
        name: metadata.name.clone().unwrap_or_default(),
        namespace: metadata.namespace.clone(),
        creation_timestamp,
        controller_owner: controller_owner(metadata),
        labels: metadata.labels.clone().unwrap_or_default(),
        annotations: metadata.annotations.clone().unwrap_or_default(),
        cells: extract_custom_cells(&obj.data, custom_columns),
        log_containers: Vec::new(),
    }
    .with_lifecycle_metadata(
        metadata.deletion_timestamp.is_some(),
        metadata.finalizers.clone().unwrap_or_default(),
    )
}

pub(crate) fn extract_custom_cells(
    data: &k8s_openapi::serde_json::Value,
    columns: &[CustomResourceColumn],
) -> BTreeMap<String, CellValue> {
    use jsonpath_rust::JsonPath;

    columns
        .iter()
        .filter_map(|column| {
            let path = JsonPath::try_from(column.json_path.as_str()).ok()?;
            let value = path.find(data);
            let values = value.as_array()?.to_vec();
            custom_cell_value(column, &values).map(|cell| (column.id.clone(), cell))
        })
        .collect()
}

pub(crate) fn custom_cell_value(
    column: &CustomResourceColumn,
    values: &[k8s_openapi::serde_json::Value],
) -> Option<CellValue> {
    let value = values.first()?;
    if values.len() == 1 {
        if matches!(column.type_.as_str(), "integer" | "number")
            && let Some(number) = value.as_i64()
        {
            return Some(CellValue::Number(number));
        }
        if matches!(column.type_.as_str(), "date" | "date-time")
            && let Some(value) = value.as_str().and_then(parse_timestamp)
        {
            return Some(CellValue::Timestamp(value));
        }
        return json_value_to_text(value).map(CellValue::Text);
    }

    let values = values.iter().filter_map(json_value_to_text).collect();
    Some(CellValue::List(values))
}

pub(crate) fn parse_timestamp(value: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339).ok()
}

pub(crate) fn json_value_to_text(value: &k8s_openapi::serde_json::Value) -> Option<String> {
    match value {
        k8s_openapi::serde_json::Value::Null => None,
        k8s_openapi::serde_json::Value::String(value) => Some(value.clone()),
        k8s_openapi::serde_json::Value::Bool(value) => Some(value.to_string()),
        k8s_openapi::serde_json::Value::Number(value) => Some(value.to_string()),
        other => Some(other.to_string()),
    }
}
