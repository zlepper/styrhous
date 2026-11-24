use crate::api_resource::ApiResource;
use components::SortDirection;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

// Persisted widths never go below the narrowest a table column can be resized to.
use components::design::size::TABLE_MIN_COLUMN;

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
    /// Holds numbers: right-aligned, header included (S08).
    pub(super) numeric: bool,
    /// Hidden until the user shows it.
    pub(super) default_hidden: bool,
}

impl TableColumnDefinition {
    pub(super) fn sortable(id: &str, label: &str, default_width: f32) -> Self {
        Self {
            id: id.to_owned(),
            label: label.to_owned(),
            default_width,
            sortable: true,
            numeric: false,
            default_hidden: false,
        }
    }

    /// This column, hidden until the user shows it.
    pub(super) fn hidden_by_default(self) -> Self {
        Self {
            default_hidden: true,
            ..self
        }
    }

    /// This column, holding numbers.
    pub(super) fn numeric(self) -> Self {
        Self {
            numeric: true,
            ..self
        }
    }
}

/// The metadata map a user-configured table column reads from.
#[derive(Debug, Clone, Copy, Ord, PartialOrd, Eq, PartialEq, Serialize, Deserialize)]
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
    /// Whether the rows show under a header per namespace; on until turned off.
    #[serde(default)]
    group_by_namespace: Option<bool>,
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
    /// Whether the table of `key` groups its rows by namespace (on by default).
    pub(super) fn groups_by_namespace(&self, key: &ResourceTableKey) -> bool {
        self.tables
            .get(key)
            .and_then(|layout| layout.group_by_namespace)
            .unwrap_or(true)
    }

    pub(super) fn set_groups_by_namespace(&mut self, key: &ResourceTableKey, group: bool) {
        self.tables
            .entry(key.clone())
            .or_default()
            .group_by_namespace = Some(group);
    }

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
                        width: column.width.max(TABLE_MIN_COLUMN),
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
            column.width = width.max(TABLE_MIN_COLUMN);
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
        // Rows sorted by a column nobody can see could not be put back.
        if !visible
            && layout
                .sort
                .as_ref()
                .is_some_and(|sort| sort.column_id == id)
        {
            layout.sort = None;
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
    ) -> Option<components::SortState> {
        self.layout_mut(key, definitions).sort.as_ref().map(|sort| {
            components::SortState::new(
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

    /// Sort as a click on `column_id`'s header does: ascending, then
    /// descending, then back to the default order (S08).
    pub(super) fn sort_after_header_click(
        &mut self,
        key: &ResourceTableKey,
        definitions: &[TableColumnDefinition],
        column_id: &str,
    ) {
        let current = self.sort(key, definitions);
        match components::SortState::after_click(current.as_ref(), column_id) {
            Some(next) => self.set_sort(key, definitions, &next.column_id, next.direction),
            None => self.layout_mut(key, definitions).sort = None,
        }
    }

    /// Show the columns that show by default (the column menu's "Reset to default").
    pub(super) fn reset_column_visibility(
        &mut self,
        key: &ResourceTableKey,
        definitions: &[TableColumnDefinition],
    ) {
        for column in &mut self.layout_mut(key, definitions).columns {
            column.visible = definitions
                .iter()
                .find(|definition| definition.id == column.id)
                .is_none_or(|definition| !definition.default_hidden);
        }
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
                    visible: !definition.default_hidden,
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
mod tests;
