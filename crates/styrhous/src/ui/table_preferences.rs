use crate::api_resource::ApiResource;
use components::SortDirection;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const MIN_COLUMN_WIDTH: f32 = 80.0;

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub(super) struct ResourceTableKey {
    pub(super) detail_resource: Option<TableResourceIdentity>,
    pub(super) table_resource: TableResourceIdentity,
}

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub(super) struct TableResourceIdentity {
    group: String,
    name: String,
}

impl TableResourceIdentity {
    fn from_api_resource(resource: &ApiResource) -> Self {
        Self {
            group: if resource.group.is_empty() {
                "core".to_owned()
            } else {
                resource.group.clone()
            },
            name: resource.name.clone(),
        }
    }
}

impl ResourceTableKey {
    pub(super) fn workspace(resource: &ApiResource) -> Self {
        Self {
            detail_resource: None,
            table_resource: TableResourceIdentity::from_api_resource(resource),
        }
    }

    pub(super) fn detail(detail_resource: &ApiResource, table_resource: &ApiResource) -> Self {
        Self {
            detail_resource: Some(TableResourceIdentity::from_api_resource(detail_resource)),
            table_resource: TableResourceIdentity::from_api_resource(table_resource),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct TableColumnDefinition {
    pub(super) id: String,
    pub(super) label: String,
    pub(super) default_width: f32,
    pub(super) sortable: bool,
}

impl TableColumnDefinition {
    pub(super) fn sortable(id: &str, label: &str, default_width: f32) -> Self {
        Self {
            id: id.to_owned(),
            label: label.to_owned(),
            default_width,
            sortable: true,
        }
    }
}

/// The metadata map a user-configured table column reads from.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub(super) enum MetadataColumnSource {
    Label,
    Annotation,
}

/// A user-defined workspace-table column backed by one metadata key.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub(super) struct CustomMetadataColumn {
    pub(super) source: MetadataColumnSource,
    pub(super) key: String,
    pub(super) label: String,
}

impl CustomMetadataColumn {
    pub(super) fn id(&self) -> String {
        let source = match self.source {
            MetadataColumnSource::Label => "label",
            MetadataColumnSource::Annotation => "annotation",
        };
        format!("metadata-{source}-{}", self.key)
    }
}

#[derive(Debug, Clone)]
pub(super) struct ResolvedTableColumn {
    pub(super) definition: TableColumnDefinition,
    pub(super) width: f32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(super) struct PersistedResourceTablePreferences {
    #[serde(default)]
    tables: BTreeMap<ResourceTableKey, PersistedTableLayout>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
struct PersistedTableLayout {
    #[serde(default)]
    columns: Vec<PersistedTableColumn>,
    #[serde(default)]
    custom_columns: Vec<CustomMetadataColumn>,
    #[serde(default)]
    sort: Option<PersistedSort>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct PersistedTableColumn {
    id: String,
    visible: bool,
    width: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct PersistedSort {
    column_id: String,
    direction: PersistedSortDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
enum PersistedSortDirection {
    Ascending,
    Descending,
}

impl PersistedResourceTablePreferences {
    pub(super) fn custom_columns(&mut self, key: &ResourceTableKey) -> Vec<CustomMetadataColumn> {
        self.tables
            .entry(key.clone())
            .or_default()
            .custom_columns
            .clone()
    }

    pub(super) fn add_custom_column(
        &mut self,
        key: &ResourceTableKey,
        column: CustomMetadataColumn,
    ) -> bool {
        let layout = self.tables.entry(key.clone()).or_default();
        if layout
            .custom_columns
            .iter()
            .any(|existing| existing.source == column.source && existing.key == column.key)
        {
            return false;
        }
        layout.custom_columns.push(column);
        true
    }

    pub(super) fn rename_custom_column(
        &mut self,
        key: &ResourceTableKey,
        id: &str,
        label: String,
    ) -> bool {
        let Some(column) = self
            .tables
            .entry(key.clone())
            .or_default()
            .custom_columns
            .iter_mut()
            .find(|column| column.id() == id)
        else {
            return false;
        };
        column.label = label;
        true
    }

    pub(super) fn remove_custom_column(&mut self, key: &ResourceTableKey, id: &str) -> bool {
        let layout = self.tables.entry(key.clone()).or_default();
        let original_len = layout.custom_columns.len();
        layout.custom_columns.retain(|column| column.id() != id);
        if layout.custom_columns.len() == original_len {
            return false;
        }
        layout.columns.retain(|column| column.id != id);
        if layout
            .sort
            .as_ref()
            .is_some_and(|sort| sort.column_id == id)
        {
            layout.sort = None;
        }
        true
    }

    pub(super) fn resolved_columns(
        &mut self,
        key: &ResourceTableKey,
        definitions: &[TableColumnDefinition],
    ) -> Vec<ResolvedTableColumn> {
        let layout = self.layout_mut(key, definitions);
        let definitions_by_id = definitions
            .iter()
            .map(|definition| (definition.id.as_str(), definition))
            .collect::<BTreeMap<_, _>>();
        layout
            .columns
            .iter()
            .filter(|column| column.visible)
            .filter_map(|column| {
                definitions_by_id
                    .get(column.id.as_str())
                    .map(|definition| ResolvedTableColumn {
                        definition: (*definition).clone(),
                        width: column.width.max(MIN_COLUMN_WIDTH),
                    })
            })
            .collect()
    }

    pub(super) fn all_columns(
        &mut self,
        key: &ResourceTableKey,
        definitions: &[TableColumnDefinition],
    ) -> Vec<(TableColumnDefinition, bool)> {
        let layout = self.layout_mut(key, definitions);
        let definitions_by_id = definitions
            .iter()
            .map(|definition| (definition.id.as_str(), definition))
            .collect::<BTreeMap<_, _>>();
        layout
            .columns
            .iter()
            .filter_map(|column| {
                definitions_by_id
                    .get(column.id.as_str())
                    .map(|definition| ((*definition).clone(), column.visible))
            })
            .collect()
    }

    pub(super) fn set_width(
        &mut self,
        key: &ResourceTableKey,
        definitions: &[TableColumnDefinition],
        id: &str,
        width: f32,
    ) {
        if let Some(column) = self
            .layout_mut(key, definitions)
            .columns
            .iter_mut()
            .find(|column| column.id == id)
        {
            column.width = width.max(MIN_COLUMN_WIDTH);
        }
    }

    pub(super) fn set_visible(
        &mut self,
        key: &ResourceTableKey,
        definitions: &[TableColumnDefinition],
        id: &str,
        visible: bool,
    ) {
        let layout = self.layout_mut(key, definitions);
        if !visible
            && layout
                .columns
                .iter()
                .filter(|column| {
                    column.visible
                        && definitions
                            .iter()
                            .any(|definition| definition.id == column.id)
                })
                .count()
                == 1
            && definitions.iter().any(|definition| definition.id == id)
        {
            return;
        }
        if let Some(column) = layout.columns.iter_mut().find(|column| column.id == id) {
            column.visible = visible;
        }
    }

    pub(super) fn set_order(
        &mut self,
        key: &ResourceTableKey,
        definitions: &[TableColumnDefinition],
        ids: &[String],
    ) {
        let known = definitions
            .iter()
            .map(|definition| definition.id.as_str())
            .collect::<BTreeSet<_>>();
        let layout = self.layout_mut(key, definitions);
        let mut ordered = Vec::with_capacity(layout.columns.len());
        for id in ids {
            if known.contains(id.as_str())
                && let Some(index) = layout.columns.iter().position(|column| column.id == *id)
            {
                ordered.push(layout.columns.remove(index));
            }
        }
        ordered.append(&mut layout.columns);
        layout.columns = ordered;
    }

    pub(super) fn sort(
        &mut self,
        key: &ResourceTableKey,
        definitions: &[TableColumnDefinition],
    ) -> Option<(String, SortDirection)> {
        self.layout_mut(key, definitions).sort.as_ref().map(|sort| {
            (
                sort.column_id.clone(),
                match sort.direction {
                    PersistedSortDirection::Ascending => SortDirection::Ascending,
                    PersistedSortDirection::Descending => SortDirection::Descending,
                },
            )
        })
    }

    pub(super) fn set_sort(
        &mut self,
        key: &ResourceTableKey,
        definitions: &[TableColumnDefinition],
        column_id: &str,
        direction: SortDirection,
    ) {
        self.layout_mut(key, definitions).sort = Some(PersistedSort {
            column_id: column_id.to_owned(),
            direction: match direction {
                SortDirection::Ascending => PersistedSortDirection::Ascending,
                SortDirection::Descending => PersistedSortDirection::Descending,
            },
        });
    }

    fn layout_mut(
        &mut self,
        key: &ResourceTableKey,
        definitions: &[TableColumnDefinition],
    ) -> &mut PersistedTableLayout {
        let layout = self.tables.entry(key.clone()).or_default();
        for definition in definitions {
            if !layout
                .columns
                .iter()
                .any(|column| column.id == definition.id)
            {
                let column = PersistedTableColumn {
                    id: definition.id.clone(),
                    visible: true,
                    width: definition.default_width,
                };
                if definition.id.starts_with("metadata-")
                    && let Some(index) = ["node", "age", "actions"]
                        .iter()
                        .find_map(|id| layout.columns.iter().position(|column| column.id == *id))
                {
                    layout.columns.insert(index, column);
                } else {
                    layout.columns.push(column);
                }
            }
        }
        layout
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn resource(group: &str, name: &str) -> ApiResource {
        ApiResource {
            group: group.into(),
            version: "v1".into(),
            kind: name.into(),
            name: name.into(),
            namespaced: true,
        }
    }
    fn columns() -> Vec<TableColumnDefinition> {
        vec![
            TableColumnDefinition {
                id: "name".into(),
                label: "Name".into(),
                default_width: 180.0,
                sortable: true,
            },
            TableColumnDefinition {
                id: "age".into(),
                label: "Age".into(),
                default_width: 77.0,
                sortable: true,
            },
        ]
    }
    #[test]
    fn workspace_key_ignores_cluster_and_version() {
        let mut first = resource("", "pods");
        first.version = "v1".into();
        let mut second = resource("core", "pods");
        second.version = "v2".into();
        assert_eq!(
            ResourceTableKey::workspace(&first),
            ResourceTableKey::workspace(&second)
        );
    }
    #[test]
    fn layout_merges_new_columns_and_keeps_one_visible() {
        let key = ResourceTableKey::workspace(&resource("core", "pods"));
        let mut preferences = PersistedResourceTablePreferences::default();
        preferences.set_visible(&key, &columns(), "name", false);
        preferences.set_visible(&key, &columns(), "age", false);
        assert_eq!(preferences.resolved_columns(&key, &columns()).len(), 1);
        let mut extended = columns();
        extended.push(TableColumnDefinition {
            id: "status".into(),
            label: "Status".into(),
            default_width: 100.0,
            sortable: true,
        });
        assert_eq!(preferences.all_columns(&key, &extended).len(), 3);
    }

    #[test]
    fn stale_columns_do_not_allow_hiding_every_available_column() {
        let key = ResourceTableKey::workspace(&resource("core", "pods"));
        let mut preferences = PersistedResourceTablePreferences::default();
        let mut wide_columns = columns();
        wide_columns.push(TableColumnDefinition {
            id: "namespace".into(),
            label: "Namespace".into(),
            default_width: 120.0,
            sortable: true,
        });
        preferences.all_columns(&key, &wide_columns);

        preferences.set_visible(&key, &columns(), "name", false);
        preferences.set_visible(&key, &columns(), "age", false);

        assert_eq!(preferences.resolved_columns(&key, &columns()).len(), 1);
    }

    #[test]
    fn actions_column_cannot_be_resized_below_the_readable_minimum() {
        let key = ResourceTableKey::workspace(&resource("core", "pods"));
        let mut preferences = PersistedResourceTablePreferences::default();
        let actions = TableColumnDefinition {
            id: "actions".into(),
            label: "Actions".into(),
            default_width: 104.0,
            sortable: false,
        };

        preferences.set_width(&key, std::slice::from_ref(&actions), "actions", 12.0);

        assert_eq!(
            preferences.resolved_columns(&key, &[actions])[0].width,
            MIN_COLUMN_WIDTH
        );
    }

    #[test]
    fn detail_key_includes_the_outer_resource_type() {
        let pods = resource("", "pods");
        let deployment = resource("apps", "deployments");
        let replica_set = resource("apps", "replicasets");

        assert_ne!(
            ResourceTableKey::detail(&deployment, &pods),
            ResourceTableKey::detail(&replica_set, &pods)
        );
    }

    #[test]
    fn custom_columns_preserve_layout_state_and_can_be_renamed_or_removed() {
        let key = ResourceTableKey::workspace(&resource("core", "pods"));
        let custom = CustomMetadataColumn {
            source: MetadataColumnSource::Annotation,
            key: "example.com/team".into(),
            label: "Team".into(),
        };
        let id = custom.id();
        let mut preferences = PersistedResourceTablePreferences::default();

        assert!(preferences.add_custom_column(&key, custom.clone()));
        assert!(!preferences.add_custom_column(&key, custom));
        assert!(preferences.rename_custom_column(&key, &id, "Owning team".into()));
        assert_eq!(preferences.custom_columns(&key)[0].label, "Owning team");

        let mut definitions = columns();
        definitions.push(TableColumnDefinition {
            id: id.clone(),
            label: "Owning team".into(),
            default_width: 160.0,
            sortable: true,
        });
        preferences.set_width(&key, &definitions, &id, 240.0);
        preferences.set_sort(&key, &definitions, &id, SortDirection::Ascending);
        let resolved = preferences.resolved_columns(&key, &definitions);
        assert_eq!(
            resolved
                .iter()
                .find(|column| column.definition.id == id)
                .expect("custom column is present")
                .width,
            240.0
        );
        let custom_index = resolved
            .iter()
            .position(|column| column.definition.id == id)
            .expect("custom column is present");
        let age_index = resolved
            .iter()
            .position(|column| column.definition.id == "age")
            .expect("age column is present");
        assert!(custom_index < age_index);

        assert!(preferences.remove_custom_column(&key, &id));
        assert!(preferences.custom_columns(&key).is_empty());
        assert!(preferences.sort(&key, &columns()).is_none());
        assert!(!preferences.remove_custom_column(&key, &id));
    }
}
