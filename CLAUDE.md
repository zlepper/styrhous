# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Test Commands

```bash
# Build (entire workspace)
cargo build

# Run the application
cargo run -p kubernetes-dev-ui

# Run all tests
cargo test -p kubernetes-dev-ui

# Run tests with snapshot updates (for UI tests)
UPDATE_SNAPSHOTS=1 cargo test -p kubernetes-dev-ui

# Run a single test
cargo test -p kubernetes-dev-ui test_ui_flow

# Run ignored tests (requires real cluster connection)
cargo test -p kubernetes-dev-ui -- --ignored
```

## Development Environment

Uses Nix flakes for development dependencies. Enter the dev shell with:
```bash
nix develop
```

Local Kubernetes testing uses `kind` (included in flake.nix).

## Architecture

This is a Kubernetes development UI built with egui/eframe. The application uses a multi-threaded architecture separating UI from Kubernetes operations.

### Core Components

**UI Layer (`crates/app/src/ui.rs`)**
- `MyEguiApp<W: WorkerTrait>` - Main application struct, generic over worker for testability
- `UiState` - Holds all UI state including clusters, namespaces, and selections
- `ClusterState` - Per-cluster state (connection, namespaces, API resources)
- Receives updates from worker via `WorkerResult` enum and sends commands via `WorkerCommand`

**Worker Layer (`crates/app/src/worker.rs`)**
- `Worker` - Production implementation running Kubernetes operations on a background thread
- `WorkerTrait` - Abstraction enabling mock injection for tests
- `MockWorker` - Test double with `VecDeque<WorkerResult>` for injecting responses
- Uses `mpsc` channels for UI-worker communication
- Spawns a tokio runtime for async Kubernetes operations

**Kubernetes Integration (`crates/app/src/cluster_connection_manager.rs`)**
- `ClusterConnection` - Manages active cluster connections with background watchers
- `KubernetesNamespaceWatcher` - Watches namespace changes via kube-rs watcher API
- `KubernetesApiInspector` - Discovers available API resources in a cluster
- Reads kubeconfig contexts to populate cluster list

### Data Flow

1. UI calls `worker.start()` which spawns background thread with tokio runtime
2. Worker sends initial `LoadClusters` command to itself
3. Worker reads kubeconfig and sends `KubernetesClustersUpdated` result
4. On cluster selection, UI sends `ConnectToCluster` command
5. Worker creates `ClusterConnection` which spawns namespace watcher and API inspector
6. Real-time namespace updates flow back via `KubernetesNamespacesAdded/Deleted/Replaced`

### Testing

Uses `egui_kittest` for snapshot testing. Tests inject `MockWorker` to control worker responses:

```rust
let mut harness = Harness::new_eframe(|_cc| MyEguiApp::<MockWorker>::default());
harness.state_mut().worker.results.push_back(WorkerResult::...);
harness.run();
harness.snapshot("name");
```

Snapshots are stored in `crates/app/tests/snapshots/`.

### Helper Types

- `SortedName` - Case-insensitive sortable string wrapper for BTreeMap keys
- `MinimalNamespace` - Lightweight namespace representation with optional display name from `tesseract.dev/display-name` annotation
- `ApiResource` - Kubernetes API resource descriptor (group, version, kind, name)
