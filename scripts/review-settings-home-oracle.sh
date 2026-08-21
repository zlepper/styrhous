#!/usr/bin/env bash
# Compare the Settings home blade against its approved visual oracle.
#
# Usage:
#   ./scripts/review-settings-home-oracle.sh [snapshot.png] [output-directory]

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
snapshot=${1:-"$repo_root/crates/styrhous/tests/snapshots/settings/settings_home_navigates_to_cluster_discovery_and_shows_candidates/settings_home.png"}
output_dir=${2:-"$repo_root/target/visual-diffs/settings-home-blade"}
oracle="$repo_root/docs/design/oracles/settings-home-blade-oracle.png"
blade_snapshot="$output_dir/settings-home-blade-snapshot.png"

if ! command -v magick >/dev/null; then
    echo "ImageMagick is required. Run: nix-shell -p imagemagick --run ./scripts/review-settings-home-oracle.sh" >&2
    exit 127
fi

mkdir -p "$output_dir"
magick "$snapshot" -crop '744x1008+784+8' +repage "$blade_snapshot"

"$repo_root/scripts/review-oracle-diff.sh" "$oracle" "$blade_snapshot" "$output_dir"
