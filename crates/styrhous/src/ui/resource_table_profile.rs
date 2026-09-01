//! CPU-only benchmark support for large resource tables.
//!
//! The profile renders the production workspace with deterministic cluster state.
//! Kubernetes I/O and GPU submission are deliberately outside the benchmark.

use super::namespace_selector::NamespaceSelectorSettings;
use super::state::{
    ClusterConnectionState, ClusterState, ResourceSearchState, ResourceWatchState, UiState,
};
use super::table_preferences::{
    PersistedResourceTablePreferences, ResourceTableKey, TableColumnDefinition,
};
use super::workspace;
use crate::api_resource::ApiResource;
use crate::minimal_resource::MinimalResource;
use crate::resource_detail::ResourceOwner;
use crate::resource_table::{AVAILABLE_COLUMN, CellValue, READY_COLUMN, UP_TO_DATE_COLUMN};
use crate::worker::{KubernetesResourceAdded, WorkerCommandBox, WorkerResult};
use components::SortDirection;
use std::collections::{BTreeMap, HashMap};
use time::OffsetDateTime;

const CLUSTER_KEY: i32 = 1;
const VIEWPORT_SIZE: egui::Vec2 = egui::vec2(1280.0, 900.0);
const SCROLL_DELTA_Y: f32 = -352.0;
const NAMESPACES: [&str; 4] = ["default", "payments", "platform", "staging"];

/// Reusable state for Criterion's large resource-table scenarios.
pub struct ResourceTableProfile {
    context: egui::Context,
    input: egui::RawInput,
    ui_state: UiState,
    table_preferences: PersistedResourceTablePreferences,
    namespace_selector_settings: NamespaceSelectorSettings,
    api_resource: ApiResource,
    elapsed_seconds: f64,
    next_scroll_delta_y: f32,
    search_variant: bool,
    sort_direction: SortDirection,
    update_variant: bool,
    #[cfg(test)]
    last_visible_resource_names: Vec<String>,
}

impl ResourceTableProfile {
    /// Creates a connected workspace containing the requested number of rows.
    pub fn with_resource_count(resource_count: usize) -> Result<Self, String> {
        if resource_count == 0 {
            return Err("resource-table benchmark needs at least one row".to_owned());
        }

        let context = egui::Context::default();
        super::configure_egui_context(&context);
        let api_resource = deployment_resource();
        let mut cluster = ClusterState::new(CLUSTER_KEY, "benchmark".to_owned());
        cluster.connection = ClusterConnectionState::Connected;
        cluster.namespaces_load = super::state::ClusterLoadState::Ready;
        cluster.api_resources_load = super::state::ClusterLoadState::Ready;
        cluster.selected_api_resource = Some(api_resource.clone());

        for namespace in NAMESPACES {
            cluster.selected_namespaces.insert(namespace.to_owned());
            cluster.resource_cache.insert(
                (api_resource.clone(), Some(namespace.to_owned())),
                ResourceWatchState {
                    is_synced: true,
                    ..Default::default()
                },
            );
        }

        for index in 0..resource_count {
            let namespace = NAMESPACES[index % NAMESPACES.len()];
            let resource = synthetic_resource(index, namespace);
            cluster
                .resource_cache
                .get_mut(&(api_resource.clone(), Some(namespace.to_owned())))
                .expect("benchmark namespace watch exists")
                .resources
                .insert(resource.uid.clone(), resource);
        }

        Ok(Self {
            context,
            input: raw_input(0.0),
            ui_state: UiState {
                clusters: HashMap::from([(CLUSTER_KEY, cluster)]),
                selected_cluster: Some(CLUSTER_KEY),
                ..Default::default()
            },
            table_preferences: PersistedResourceTablePreferences::default(),
            namespace_selector_settings: NamespaceSelectorSettings::default(),
            api_resource,
            elapsed_seconds: 0.0,
            next_scroll_delta_y: SCROLL_DELTA_Y,
            search_variant: false,
            sort_direction: SortDirection::Ascending,
            update_variant: false,
            #[cfg(test)]
            last_visible_resource_names: Vec::new(),
        })
    }

    /// Renders one production workspace frame and returns an observable result.
    pub fn run_frame(&mut self) -> usize {
        self.elapsed_seconds += 1.0 / 60.0;
        self.input.time = Some(self.elapsed_seconds);
        let mut commands = Vec::<WorkerCommandBox>::new();
        let mut shell_requests = Vec::new();
        let mut output = self.context.run_ui(self.input.clone(), |ui| {
            workspace::show(
                ui,
                &mut self.ui_state,
                &mut commands,
                &mut shell_requests,
                &[],
                &mut self.table_preferences,
                &self.namespace_selector_settings,
            );
        });
        #[cfg(test)]
        {
            self.last_visible_resource_names.clear();
            for clipped_shape in &output.shapes {
                collect_visible_resource_names(
                    &clipped_shape.shape,
                    &mut self.last_visible_resource_names,
                );
            }
        }
        output.textures_delta.clear();
        self.input.events.clear();
        output.shapes.len()
    }

    /// Scrolls the virtualized table by one viewport-sized step.
    pub fn scroll_frame(&mut self) -> usize {
        self.input.events = vec![
            egui::Event::PointerMoved(egui::pos2(640.0, 500.0)),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, self.next_scroll_delta_y),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            },
        ];
        self.run_frame();
        self.next_scroll_delta_y = -self.next_scroll_delta_y;
        self.run_frame()
    }

    /// Alternates fuzzy queries so every invocation measures filter invalidation.
    pub fn search_frame(&mut self) -> usize {
        self.search_variant = !self.search_variant;
        let query = if self.search_variant {
            "service-1"
        } else {
            "service-2"
        };
        self.ui_state
            .clusters
            .get_mut(&CLUSTER_KEY)
            .expect("benchmark cluster exists")
            .resource_searches
            .insert(
                self.api_resource.clone(),
                ResourceSearchState {
                    query: query.to_owned(),
                    regex_mode: false,
                },
            );
        self.run_frame()
    }

    /// Alternates name-sort direction so every invocation measures reordering.
    pub fn sort_frame(&mut self) -> usize {
        self.sort_direction = match self.sort_direction {
            SortDirection::Ascending => SortDirection::Descending,
            SortDirection::Descending => SortDirection::Ascending,
        };
        self.table_preferences.set_sort(
            &ResourceTableKey::workspace(&self.api_resource),
            &[TableColumnDefinition::sortable("name", "Name", 160.0)],
            "name",
            self.sort_direction,
        );
        self.run_frame()
    }

    /// Applies one watch update and renders the resulting table frame.
    pub fn update_frame(&mut self) -> usize {
        self.update_variant = !self.update_variant;
        let mut resource = synthetic_resource(0, NAMESPACES[0]);
        resource.cells.insert(
            AVAILABLE_COLUMN.to_owned(),
            CellValue::Number(i64::from(self.update_variant)),
        );
        KubernetesResourceAdded {
            cluster_key: CLUSTER_KEY,
            api_resource: self.api_resource.clone(),
            namespace: Some(NAMESPACES[0].to_owned()),
            resource,
        }
        .apply(&mut self.ui_state, &mut Vec::new());
        self.run_frame()
    }

    #[cfg(test)]
    fn cache_generation(&self) -> u64 {
        self.ui_state.clusters[&CLUSTER_KEY]
            .resource_table_cache
            .generation()
    }

    #[cfg(test)]
    fn prepared_resource_counts(&self) -> (usize, usize) {
        let prepared = self.ui_state.clusters[&CLUSTER_KEY]
            .resource_table_cache
            .prepared();
        (prepared.resource_count, prepared.visible_resource_count)
    }

    #[cfg(test)]
    fn first_visible_resource_name(&self) -> Option<&str> {
        self.last_visible_resource_names.first().map(String::as_str)
    }
}

#[cfg(test)]
fn collect_visible_resource_names(shape: &egui::Shape, names: &mut Vec<String>) {
    match shape {
        egui::Shape::Vec(shapes) => {
            for shape in shapes {
                collect_visible_resource_names(shape, names);
            }
        }
        egui::Shape::Text(text) => {
            let value = text.galley.text();
            if value.strip_prefix("service-").is_some_and(|suffix| {
                suffix.len() == 5 && suffix.bytes().all(|byte| byte.is_ascii_digit())
            }) {
                names.push(value.to_owned());
            }
        }
        _ => {}
    }
}

fn deployment_resource() -> ApiResource {
    ApiResource {
        group: "apps".to_owned(),
        version: "v1".to_owned(),
        kind: "Deployment".to_owned(),
        name: "deployments".to_owned(),
        namespaced: true,
    }
}

fn synthetic_resource(index: usize, namespace: &str) -> MinimalResource {
    let name = format!("service-{index:05}");
    MinimalResource {
        uid: format!("uid-{index:05}"),
        name: name.clone(),
        namespace: Some(namespace.to_owned()),
        creation_timestamp: Some(
            OffsetDateTime::UNIX_EPOCH + time::Duration::seconds(index as i64),
        ),
        controller_owner: (!index.is_multiple_of(3)).then(|| ResourceOwner {
            api_version: "apps/v1".to_owned(),
            kind: "ReplicaSet".to_owned(),
            name: format!("{name}-7d9c"),
            uid: format!("owner-{index:05}"),
            controller: true,
        }),
        labels: BTreeMap::from([
            ("app.kubernetes.io/name".to_owned(), name),
            (
                "app.kubernetes.io/team".to_owned(),
                format!("team-{}", index % 12),
            ),
        ]),
        annotations: BTreeMap::from([(
            "styrhous.dev/benchmark".to_owned(),
            format!("row-{index}"),
        )]),
        cells: BTreeMap::from([
            (
                READY_COLUMN.to_owned(),
                CellValue::Text(format!("{}/3", index % 4)),
            ),
            (
                UP_TO_DATE_COLUMN.to_owned(),
                CellValue::Number((index % 4) as i64),
            ),
            (
                AVAILABLE_COLUMN.to_owned(),
                CellValue::Number((index % 3) as i64),
            ),
        ]),
        log_containers: Vec::new(),
    }
}

fn raw_input(time: f64) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, VIEWPORT_SIZE)),
        time: Some(time),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_renders_and_exercises_each_interaction() {
        let mut profile = ResourceTableProfile::with_resource_count(1_000)
            .expect("resource-table profile initializes");

        assert!(profile.run_frame() > 0);
        let initial_generation = profile.cache_generation();
        assert!(initial_generation > 0);

        assert!(profile.scroll_frame() > 0);
        assert_eq!(profile.cache_generation(), initial_generation);

        assert!(profile.search_frame() > 0);
        let search_generation = profile.cache_generation();
        assert!(search_generation > initial_generation);

        assert!(profile.sort_frame() > 0);
        let sort_generation = profile.cache_generation();
        assert!(sort_generation > search_generation);

        assert!(profile.update_frame() > 0);
        assert!(profile.cache_generation() > sort_generation);
    }

    #[test]
    fn ten_thousand_resource_profile_keeps_rendering_virtualized() {
        let mut profile = ResourceTableProfile::with_resource_count(10_000)
            .expect("large resource-table profile initializes");

        let first_frame_shapes = profile.run_frame();
        assert_eq!(profile.prepared_resource_counts(), (10_000, 10_000));
        let first_visible_resource = profile
            .first_visible_resource_name()
            .expect("the first viewport has visible resources")
            .to_owned();
        assert!(
            first_frame_shapes < 1_000,
            "virtualized table should not paint one shape set per resource"
        );

        let scroll_frame_shapes = profile.scroll_frame();
        assert!(scroll_frame_shapes < 1_000);
        assert_ne!(
            profile.first_visible_resource_name(),
            Some(first_visible_resource.as_str()),
            "scrolling should move a different resource into the first visible row"
        );

        assert!(profile.search_frame() > 0);
        let (total_count, visible_count) = profile.prepared_resource_counts();
        assert_eq!(total_count, 10_000);
        assert!(visible_count > 0);
        assert!(visible_count < total_count);
    }
}
