# Report: UI Component Integration & Test Expansion

## Summary

Refactored the main application to use Tailwind-styled components, then expanded tests to reveal and fix panel layout issues.

## Changes Made

### 1. Dependencies (`crates/app/Cargo.toml`)
- Added `components = { path = "../components" }`
- Added `egui_extras = "0.33"` for image loaders

### 2. Component Enhancements (`crates/components/`)

**icons.rs** - Added icon factory functions:
- `home_icon()`, `users_icon()`, `folder_icon()`, `calendar_icon()`, `document_icon()`, `chart_bar_icon()`

**combobox.rs** - Added `selected_text()` feature:
- New field and builder method to display summary text when closed
- When closed: shows selected_text as a label
- When open: shows filter text input for searching

### 3. Main Application (`crates/app/src/ui.rs`)

**Cluster Selection Panel**:
- Uses `NarrowSidebar` with `avatar_item()`
- Shows first letter of cluster name as avatar initial
- 72px width with white background Frame

**API Resource Tree** (moved to top-level):
- Uses `WideSidebar` with `expandable()` sections
- Declared before CentralPanel for correct egui layout
- Added `min_width(200.0)` to ensure text visibility
- Added white background Frame
- Uses deferred click handling (immutable borrow for display, mutation after)

**Namespace Selector**:
- Uses `TailwindCombobox` with fuzzy filtering
- Multi-select via click toggle

**Central Panel**:
- Added white background Frame

### 4. Test Expansion (`crates/app/src/ui.rs`)

Added `select_cluster()` test helper method and expanded `test_ui_flow`:
- `01_empty_state` - Initial empty state
- `02_clusters_loaded` - Clusters in sidebar
- `03_cluster_selected_empty` - Cluster selected, central panel visible
- `04_namespaces_loaded` - With namespace data
- `05_api_resources_loaded` - Full UI with API tree

## Panel Layout Fix

The API selector was originally nested inside CentralPanel, causing:
- Dark background on central area
- Clipped text in API tree
- Incorrect panel ordering

Fixed by:
1. Moving `api-selector` SidePanel before CentralPanel
2. Using immutable borrow for API tree display, deferred mutation
3. Adding white background Frames to all panels
4. Setting `min_width(200.0)` on api-selector

## Files Modified

1. `crates/app/Cargo.toml`
2. `crates/components/src/icons.rs`
3. `crates/components/src/combobox.rs`
4. `crates/app/src/ui.rs`
5. `crates/app/tests/snapshots/*.png` (5 snapshots)
6. `crates/components/tests/snapshots/*.png`

## Notes

- egui SidePanels must be declared before CentralPanel
- `ApiResourceGroupState.open` field is unused (WideSidebar manages state internally)
- Empty API group names displayed as "core"

---

# Report: Resource Table Feature Implementation

## Summary

Implemented a resource table feature that displays Kubernetes resources when clicking an API type in the explorer tree. Uses kube-rs watcher for real-time updates, following the existing namespace watcher pattern.

## User Requirements
- **Scope**: Watch resources in selected namespace(s) only
- **Columns**: Name, Namespace (when multiple namespaces selected), Status/Phase, Ready, Age
- **Watcher behavior**: Keep all watchers running (cache for faster switching back)

## Changes Made

### 1. New File: `crates/app/src/minimal_resource.rs`

Lightweight resource representation with:
- `uid`, `name`, `namespace`, `creation_timestamp`, `phase`, `ready_status`
- `age()` method using `time` crate for duration formatting
- `display_status()` and `display_ready()` helper methods

### 2. Dependencies (`crates/app/Cargo.toml`)
- Added `time = { version = "0.3", features = ["parsing"] }`

### 3. Worker Changes (`crates/app/src/worker.rs`)

**New Command:**
- `StartResourceWatch { cluster_key, api_resource, namespace }`

**New Results:**
- `KubernetesResourceAdded { cluster_key, api_resource, namespace, resource }`
- `KubernetesResourceDeleted { cluster_key, api_resource, namespace, resource_uid }`
- `KubernetesResourcesReplaced { cluster_key, api_resource, namespace, resources }`
- `KubernetesResourceWatchStarted { cluster_key, api_resource, namespace }`

**Shared State:**
- Added `SharedWorkerState` with `RwLock<HashMap<i32, kube::Client>>`
- Stores clients when clusters connect for later watcher creation

### 4. Resource Watcher (`crates/app/src/cluster_connection_manager.rs`)

**New Structs/Functions:**
- `KubernetesResourceWatcher` - Generic watcher using `DynamicObject`
- `start_resource_watcher()` - Public API for starting watchers
- `extract_minimal_resource()` - Converts `DynamicObject` to `MinimalResource`
- `extract_status()` - Type-specific status extraction (Pod, Deployment, Service, Job, etc.)

**Key Design:**
- Uses `kube::discovery::pinned_kind()` to convert `ApiResource` to kube's types
- Handles namespaced vs cluster-scoped resources via `caps.scope`
- Follows exact pattern of `KubernetesNamespaceWatcher`

### 5. UI Changes (`crates/app/src/ui.rs`)

**New Types:**
- `ResourceWatchKey = (ApiResource, String)`
- `ResourceWatchState { resources, is_synced }`

**ClusterState Extensions:**
- `resource_cache: HashMap<ResourceWatchKey, ResourceWatchState>`
- `active_watchers: HashSet<ResourceWatchKey>`

**Event Handlers:**
- Handle all new `WorkerResult` variants in `UiState::update()`

**Watcher Triggering:**
- On API resource selection: start watchers for all selected namespaces
- On namespace selection: start watcher if API resource is selected

**Table Rendering:**
- Uses `TailwindTable` component
- Columns: Name, Namespace (conditional), Status, Ready, Age
- Resources sorted alphabetically by name

### 6. ApiResource (`crates/app/src/api_resource.rs`)
- Added `Hash` derive for use as HashMap key

## Data Flow

```
User clicks "pods" in sidebar
    ↓
UI sends StartResourceWatch for each selected namespace
    ↓
Worker looks up kube::Client from SharedWorkerState
    ↓
spawn KubernetesResourceWatcher task
    ↓
Watcher uses pinned_kind() to discover API resource
    ↓
Creates Api::<DynamicObject> and starts watcher stream
    ↓
Events flow: Init → InitApply → InitDone → Apply/Delete
    ↓
UI receives results, updates resource_cache
    ↓
TailwindTable renders resources
```

## Testing

- Unit tests for `MinimalResource::age()` formatting
- UI snapshot tests updated (snapshots regenerated)
- All tests pass

## Files Modified

| File | Description |
|------|-------------|
| `crates/app/src/minimal_resource.rs` | **NEW** - MinimalResource type |
| `crates/app/src/main.rs` | Added `mod minimal_resource` |
| `crates/app/src/worker.rs` | Command/Result variants, SharedWorkerState |
| `crates/app/src/cluster_connection_manager.rs` | KubernetesResourceWatcher implementation |
| `crates/app/src/ui.rs` | ClusterState extensions, table rendering |
| `crates/app/src/api_resource.rs` | Added Hash derive |
| `crates/app/Cargo.toml` | Added time dependency |
| `crates/app/tests/snapshots/*.png` | Updated snapshots |

## Notes

- Watchers are kept running when switching API resources (caching requirement)
- Status extraction supports Pod, Deployment, ReplicaSet, StatefulSet, Service, Job
- Generic fallback for unknown resource types
- Worker uses `tokio::sync::RwLock` for async-safe shared state

---

# Report: Integration Test Implementation

## Summary

Added an integration test that validates the resource watcher feature against a real Kind cluster. Also improved accessibility support in the combobox component.

## Changes Made

### 1. Accessibility Improvements (`crates/components/src/combobox.rs`)

**Combobox Items**:
- Added `widget_info()` to `ComboboxUi::item()` for screen reader and kittest support
- Items now expose `WidgetType::Button` role with their label text

**Combobox Input**:
- Added `widget_info()` to combobox input field
- Input exposes `WidgetType::ComboBox` role with the combobox label

### 2. Test Helpers (`crates/app/src/ui.rs`)

Added `#[cfg(test)]` helper methods:

```rust
select_cluster(cluster_key: i32)
  - Sets selected cluster
  - Triggers ConnectToCluster if disconnected

select_namespace(cluster_key: i32, namespace: &str)
  - Adds namespace to selected_namespaces

select_api_resource(cluster_key: i32, api_resource: ApiResource)
  - Sets selected_api_resource
  - Sends StartResourceWatch for all selected namespaces
```

### 3. Integration Test (`crates/app/src/ui.rs`)

**Test**: `test_resource_watcher_integration`
- Nextest creates or reuses the default Kind cluster before running the test.
- Uses real `Worker` (not mock)

**Test Flow**:
1. Wait for clusters to load from kubeconfig
2. Find "kind-kind" cluster and select it
3. Wait for namespaces and API resources to load
4. Select "kube-system" namespace
5. Select "pods" API resource (triggers watcher)
6. Wait for resources to sync
7. Assert resource count > 0
8. Assert coredns pod exists
9. Take snapshot

**Wait Helper**:
```rust
fn wait_for<T>(harness, condition, max_ms) -> Option<T>
```
Polls condition every 50ms until met or timeout.

## Files Modified

| File | Changes |
|------|---------|
| `crates/components/src/combobox.rs` | Added accessibility to items and input |
| `crates/app/src/ui.rs` | Added test helpers and integration test |
| `crates/app/tests/snapshots/integration_resource_table.png` | New snapshot |

## Running the Test

```bash
# Run integration test
cargo nextest run -p kubernetes-dev-ui test_resource_watcher_integration

# Update snapshots
UPDATE_SNAPSHOTS=1 cargo nextest run -p kubernetes-dev-ui
```

## Notes

- Integration test now uses accessibility-based UI interactions via kittest
- `click_accesskit()` is used for off-screen elements (handles scrolling/clipping issues)
- Integration snapshot shows real pods from kube-system (coredns, etcd, kube-apiserver, etc.)

---

# Report: Accessibility Click Fix & Core API Version Bug Fix

## Summary

Fixed integration test to use kittest accessibility clicking and fixed a bug where core API resources had empty version strings.

## Issues Found & Fixed

### 1. Off-Screen Element Clicking
**Problem**: The "pods" button in the API resource sidebar was at y=612, but the screen height was only 600px. Regular `click()` failed silently because the element was outside the visible area.

**Solution**: Use `click_accesskit()` which triggers the AccessKit click action directly, bypassing coordinate-based clicking. Also added a `ScrollArea` to the API resource sidebar panel.

### 2. Core API Resources Missing Version
**Problem**: Core API resources (pods, services, etc.) were being stored with `version: ""` instead of `version: "v1"`. This caused the resource watcher to fail discovery.

**Root Cause**: In `get_core_api_resources()`, the version was extracted from `resource.version` which is None/empty in the Kubernetes API response for core resources. The version should come from the API path parameter.

**Fix**: Changed `cluster_connection_manager.rs` to use the version from the loop variable instead of `resource.version`:
```rust
// Before (buggy):
version: resource.version.clone().unwrap_or("".to_string()),

// After (fixed):
for version in &core_api_versions.versions {
    // ...
    version: version.clone(),  // Use loop variable
}
```

## Files Modified

| File | Changes |
|------|---------|
| `crates/app/src/ui.rs` | Use `click_accesskit()` for pods click, add ScrollArea to sidebar |
| `crates/app/src/cluster_connection_manager.rs` | Fix core API version extraction |
| `crates/components/src/combobox.rs` | Add accessibility click test |
| `crates/components/src/sidebar.rs` | Add child_item accessibility click test |
| `crates/app/tests/snapshots/*.png` | Updated snapshots |

## Test Improvements

- Added `test_combobox_accessibility_click` - verifies kittest clicking works with combobox items
- Added `test_sidebar_child_item_click` - verifies kittest clicking works with sidebar child items
- Integration test now uses accessibility clicking throughout:
  - `harness.get_by_label("kind-kind").click()` for cluster selection
  - `harness.get_by_role_and_label(Role::ComboBox, "Namespace").click()` for combobox
  - `harness.get_by_label("kube-system").click()` for namespace selection
  - `harness.get_by_label("core").click()` for expanding API group
  - `harness.get_by_label("pods").click_accesskit()` for API resource (off-screen)

---

# Report: Sidebar ScrollArea Enhancement

## Summary

Added built-in scrolling support to WideSidebar and NarrowSidebar components.

## Changes

### `crates/components/src/sidebar.rs`
- Modified `render_sidebar()` to wrap content in `ScrollArea::vertical()`
- `auto_shrink(false)` ensures the scroll area fills available height
- Both `WideSidebar` and `NarrowSidebar` now automatically scroll when content overflows

### `crates/app/src/ui.rs`
- Removed manual `ScrollArea` wrapper around API resource sidebar (no longer needed)

## Benefit

- Sidebars now automatically handle overflow with scrolling
- Users of the sidebar components don't need to manually add ScrollArea
- The application now properly scrolls the API resource tree when it exceeds the panel height

---

# Report: Resource Actions Implementation (Edit YAML, Delete)

## Summary

Added Edit YAML and Delete actions to the Kubernetes resource table. Users can edit resource YAML in a bottom panel and delete resources with two-click confirmation.

## Features Implemented

### 1. Actions Column
- Added pencil (edit) and trash (delete) icon buttons to each row
- Icons from Heroicons (outline style, 24x24)
- Hover tooltips for each action

### 2. YAML Editor Panel
- Resizable bottom panel (default 300px, min 100px)
- Monospace code editor for YAML
- "Modified" indicator when changes are detected
- Save button (disabled when no changes)
- Close button

### 3. Delete Confirmation
- Two-click pattern for safety
- First click: trash icon turns red, tooltip shows "Click again to confirm"
- Second click within 3 seconds: actually deletes
- Timeout resets to normal state after 3 seconds

## Files Modified

| File | Changes |
|------|---------|
| `crates/components/src/icons/trash.svg` | **NEW** - Trash icon from Heroicons |
| `crates/components/src/icons/pencil.svg` | **NEW** - Pencil icon from Heroicons |
| `crates/components/src/icons.rs` | Added `trash()` and `pencil()` functions |
| `crates/app/Cargo.toml` | Added `serde_yaml` dependency |
| `crates/app/src/worker.rs` | Added Get/Delete/Apply commands and results |
| `crates/app/src/cluster_connection_manager.rs` | Added `create_dynamic_api()`, `get_resource_yaml()`, `delete_resource()`, `apply_resource_yaml()` |
| `crates/app/src/ui.rs` | Added `YamlPanelState`, `PendingDelete`, `ResourceAction`; actions column; bottom panel |

## Technical Details

### Worker Commands
```rust
GetResourceYaml { cluster_key, api_resource, namespace, resource_name }
DeleteResource { cluster_key, api_resource, namespace, resource_name }
ApplyResourceYaml { cluster_key, api_resource, namespace, resource_name, yaml }
```

### Server-Side Apply
YAML editing uses Kubernetes server-side apply (`kube::api::Patch::Apply`) rather than replace, which is more robust for partial updates.

### Deferred Action Pattern
Actions are collected during table rendering and executed afterward to avoid borrow conflicts with mutable cluster state.

## Future Improvements

1. Add confirmation dialog when closing panel with unsaved changes
2. Add context menu (right-click) as alternative to action buttons
3. Add YAML syntax highlighting
4. Add error handling/display for failed operations

---

# Report: Resource Actions Integration Test

## Summary

Added an integration test for the Edit YAML and Delete features. The test validates the complete data flow against a real Kind cluster.

## Bug Fixes

### Server-Managed Field Stripping
**Problem**: Server-side apply failed with "metadata.managedFields must be nil" error.

**Solution**: Strip server-managed fields from YAML before apply:
- `managedFields` - Kubernetes manages these internally
- `resourceVersion` - Immutable, causes conflicts
- `uid` - Immutable
- `creationTimestamp` - Immutable

Now stripping these fields in **both** directions:
1. `get_resource_yaml()` - Cleaner YAML for the editor
2. `apply_resource_yaml()` - Avoids apply errors

### Force Apply for Field Ownership
**Problem**: Apply failed with "conflict with 'unknown'" when taking ownership of fields.

**Solution**: Added `.force()` to PatchParams:
```rust
let patch_params = kube::api::PatchParams::apply("kubernetes-dev-ui").force();
```

## Test Implementation

### Test: `test_resource_actions_integration`

**Location**: `crates/app/src/ui.rs`

**Prerequisites**: `kind` and `kubectl` available in the environment. Nextest creates or reuses the default Kind cluster.

**Run Command**:
```bash
cargo nextest run -p kubernetes-dev-ui test_resource_actions_integration
```

### Test Flow
1. Create test ConfigMap with unique name (timestamp-based)
2. Navigate UI: Kind cluster → default namespace → configmaps
3. Trigger Edit YAML via worker command
4. Modify YAML: "original-value" → "edited-value"
5. Trigger Save via worker command
6. Verify change via kube-rs get
7. Trigger Delete (set pending_delete, send DeleteResource)
8. Verify deletion via kube-rs get

### Snapshots Generated
| Snapshot | Description |
|----------|-------------|
| `resource_actions_01_before_edit.png` | ConfigMap visible in table |
| `resource_actions_02_yaml_panel_open.png` | YAML panel open, clean YAML |
| `resource_actions_03_yaml_modified.png` | YAML with edited value |
| `resource_actions_04_after_save.png` | Panel closed after save |
| `resource_actions_05_pending_delete.png` | Trash icon red (pending) |
| `resource_actions_06_after_delete.png` | ConfigMap removed |

## Files Modified

| File | Changes |
|------|---------|
| `crates/app/src/cluster_connection_manager.rs` | Strip managed fields in get/apply |
| `crates/app/src/ui.rs` | Integration test implementation |
| `crates/app/tests/snapshots/*.png` | 6 new snapshots |

## Notes

- Test uses programmatic worker commands (not UI button clicks) for reliability
- ConfigMap is deleted as part of the test, so no cleanup needed
- The test validates both the UI state transitions and the actual Kubernetes API effects
