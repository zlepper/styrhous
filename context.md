# Project Context

## Current State

The Kubernetes Development UI application has been refactored to use the Tailwind-styled components from `crates/components`.

## Recent Changes (December 2024)

### UI Component Integration

The main application (`crates/app/src/ui.rs`) now uses:

1. **NarrowSidebar** - Cluster selection with avatar initials
2. **TailwindCombobox** - Namespace selector with fuzzy filtering and multi-select
3. **WideSidebar** - API resource tree with expandable groups

### Component Enhancements

- Added `selected_text()` builder method to `TailwindCombobox` for displaying summary when closed
- Exported icon factory functions from `components::icons` module

### UI Tests Expanded

The test suite now covers the full UI flow:
- `01_empty_state` - Initial state before clusters load
- `02_clusters_loaded` - Clusters displayed in NarrowSidebar
- `03_cluster_selected_empty` - Selected cluster shows central panel
- `04_namespaces_loaded` - Namespaces loaded in combobox
- `05_api_resources_loaded` - API resource tree fully rendered

### Panel Layout Fix

Fixed panel ordering and styling:
- API selector panel moved to top-level (before CentralPanel)
- Added `min_width(200.0)` to ensure text visibility
- Added white background Frame to all panels
- Used deferred click handling pattern for API resource selection

## Resource Table Feature (December 2024)

### New Files
- `crates/app/src/minimal_resource.rs` - Lightweight resource representation for UI

### Key Changes
1. **Resource Watcher** (`cluster_connection_manager.rs`)
   - `KubernetesResourceWatcher` - Generic watcher using `DynamicObject`
   - `start_resource_watcher()` - Public API for starting watchers
   - Status extraction for Pod, Deployment, Service, Job, etc.

2. **Worker Commands/Results** (`worker.rs`)
   - `StartResourceWatch` command with cluster_key, api_resource, namespace
   - `KubernetesResourceAdded/Deleted/Replaced/WatchStarted` results
   - Shared state in `WorkerRuntime` for storing kube clients

3. **UI State** (`ui.rs`)
   - `resource_cache: HashMap<(ApiResource, String), ResourceWatchState>`
   - `active_watchers: HashSet<(ApiResource, String)>`
   - Resource table rendering with `TailwindTable`

### Data Flow
1. User selects API resource type (e.g., "pods") in sidebar
2. UI sends `StartResourceWatch` for each selected namespace
3. Worker spawns `KubernetesResourceWatcher` using cached kube client
4. Watcher streams events: Init → InitApply → InitDone → Apply/Delete
5. UI updates `resource_cache` and renders table

### Watcher Caching
- Watchers stay running when switching API resources
- Cache key: `(ApiResource, namespace)` tuple
- Fast switching back to previously viewed resources

## Integration Test (December 2024)

### New Test
- `test_resource_watcher_integration` - Integration test against real Kind cluster
- Marked `#[ignore]` - requires running Kind cluster
- Run with: `cargo test -p kubernetes-dev-ui -- --ignored`

### Test Flow (Accessibility-Driven)
1. Wait for clusters to load from kubeconfig
2. Click "kind-kind" cluster via `harness.get_by_label("kind-kind").click()`
3. Wait for namespaces and API resources to load
4. Click namespace combobox via `get_by_role_and_label(Role::ComboBox, "Namespace")`
5. Click "kube-system" namespace via `get_by_label("kube-system")`
6. Click "core" group to expand via `get_by_label("core")`
7. Click "pods" via `get_by_label("pods").click_accesskit()` (off-screen element)
8. Wait for resources to sync
9. Assert coredns pod exists
10. Take snapshot for visual verification

### Accessibility Clicking Notes
- Use `click()` for on-screen elements (uses pointer coordinates)
- Use `click_accesskit()` for off-screen elements (triggers AccessKit action directly)
- Sidebar components have built-in `ScrollArea` for content overflow

### Accessibility Enhancements
- Added `widget_info()` to `ComboboxUi::item()` for accessibility
- Added `widget_info()` to combobox input field
- All sidebar items have accessibility labels via `add_button_accessibility()`

## Bug Fixes (December 2024)

### Core API Resource Version
- Fixed `get_core_api_resources()` to use the correct version from the API path
- Before: `version: resource.version.clone().unwrap_or("")` → resulted in empty version
- After: `version: version.clone()` → correctly captures "v1" for core resources
- This fix was required for the resource watcher to properly discover pods, services, etc.

## Resource Actions Feature (December 2024)

### Overview
Added Edit YAML and Delete actions to the resource table:
- **Actions column** - Native egui text buttons for each row ("Edit {name}", "Delete {name}")
- **Bottom panel** - Resizable YAML editor
- **Two-click delete** - First click marks for deletion (red text, 3s timeout), second click confirms

### Action Buttons
- Uses native `ui.button()` for proper accessibility with egui_kittest
- Button labels include resource name: "Edit coredns-xxx", "Delete coredns-xxx"
- Delete button changes to "Confirm delete {name}" when pending
- Icon-based buttons were replaced with text buttons for better accessibility in tests

### Icons Available
- `trash.svg` and `pencil.svg` exist in `crates/components/src/icons/`
- Icon functions: `trash(ui, size, color)` and `pencil(ui, size, color)` for non-interactive icons

### Worker Commands/Results
- `GetResourceYaml` - Fetch full resource YAML
- `DeleteResource` - Delete a resource
- `ApplyResourceYaml` - Apply edited YAML via server-side apply
- `ResourceYamlFetched`, `ResourceDeleteCompleted`, `ResourceApplyCompleted` results

### UI State
- `YamlPanelState` - Tracks resource being edited, original/edited YAML, panel height
- `PendingDelete` - Tracks resource marked for deletion with timestamp
- `ResourceAction` - Enum for deferred action handling

### Kubernetes API Functions
- `create_dynamic_api()` - Helper to create namespaced/cluster-scoped API
- `get_resource_yaml()` - Fetch resource, strip server-managed fields, serialize to YAML
- `delete_resource()` - Delete via kube::Api::delete()
- `apply_resource_yaml()` - Apply via server-side apply with force (kube::api::Patch::Apply)

### Server-Managed Field Stripping
Both `get_resource_yaml()` and `apply_resource_yaml()` strip:
- `metadata.managedFields` - Causes "managedFields must be nil" error on apply
- `metadata.resourceVersion` - Immutable, causes conflicts
- `metadata.uid` - Immutable
- `metadata.creationTimestamp` - Immutable

This provides a cleaner YAML editing experience and avoids apply conflicts.

### Bottom Panel Behavior
- Rendered before CentralPanel to reserve space
- Resizable with min height 100px
- Shows "Modified" indicator when YAML is changed
- Save button disabled when no changes
- Close discards unsaved changes (TODO: confirmation dialog)

## Architecture Notes

- Components are in `crates/components`, app is in `crates/app`
- Image loaders must be installed via `egui_extras::install_image_loaders()` for SVG icons
- The combobox shows `selected_text` when closed, switches to filter input when open
- egui SidePanels must be declared before CentralPanel for correct layout
- egui BottomPanels must also be declared before CentralPanel
- Worker maintains shared state (`SharedWorkerState`) for kube clients
- Resource watchers use `DynamicObject` for generic type handling
- Core API resources use `group: "core"` for display but convert to `""` for kube API calls
- Sidebar components (`WideSidebar`, `NarrowSidebar`) have built-in vertical scrolling
- Use deferred action pattern to avoid borrow conflicts during UI rendering

## Resource Actions Integration Test (December 2024)

### New Test
- `test_resource_actions_integration` - Tests Edit YAML and Delete against real Kind cluster
- Marked `#[ignore]` - requires running Kind cluster
- Run with: `cargo test -p kubernetes-dev-ui test_resource_actions_integration -- --ignored`

### Test Flow (Accessibility-Driven UI Clicks)
1. Cleanup: Delete any leftover `test-cm-*` ConfigMaps from previous runs
2. Create test ConfigMap via kube-rs with unique timestamped name
3. Navigate UI: select Kind cluster, default namespace, configmaps
4. Click "Edit {name}" button via `get_by_label().click_accesskit()`
5. Wait for YAML panel to open
6. Modify YAML in state (text editing via accessibility is complex)
7. Click "Save YAML" button via `get_by_label("Save YAML").click()`
8. Verify change persisted via kube-rs get
9. Click "Delete {name}" button (first click marks for deletion)
10. Click "Confirm delete {name}" button (second click confirms)
11. Wait for resource to be removed from cache via watcher
12. Verify deletion via kube-rs get

### Key Testing Patterns
- Use `click_accesskit()` for buttons that may be off-screen in scrollable areas
- Run extra frames after state changes to ensure UI is re-rendered
- Native egui buttons (`ui.button()`) are required for kittest accessibility detection
- `Button::image_and_text()` inside table cells doesn't properly expose accessibility
- No snapshots for integration tests - real cluster data (Age column, etc.) constantly changes
- Test uses deterministic ConfigMap name `test-cm-integration` for reproducibility
