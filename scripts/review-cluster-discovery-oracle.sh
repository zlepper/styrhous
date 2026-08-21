#!/usr/bin/env bash
# Compare the cluster-discovery blade against its light design oracle.
#
# Usage:
#   ./scripts/review-cluster-discovery-oracle.sh [snapshot.png] [output-directory]

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
snapshot=${1:-"$repo_root/crates/styrhous/tests/snapshots/settings/settings_home_navigates_to_cluster_discovery_and_shows_candidates/cluster_discovery.png"}
output_dir=${2:-"$repo_root/target/visual-diffs/cluster-discovery-blade"}
oracle="$repo_root/docs/design/oracles/cluster-discovery-blade-oracle.png"
blade_snapshot="$output_dir/cluster-discovery-blade-snapshot.png"

mkdir -p "$output_dir"
magick "$snapshot" -crop '744x1008+784+8' +repage "$blade_snapshot"

"$repo_root/scripts/review-oracle-diff.sh" "$oracle" "$blade_snapshot" "$output_dir"
