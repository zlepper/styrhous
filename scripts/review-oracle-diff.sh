#!/usr/bin/env bash
# Generate the resource-table visual diff, then ask a fresh Codex CLI session
# to critique the three images. The session is read-only and writes only its
# final review to target/visual-diffs.
#
# Usage:
#   nix-shell -p imagemagick --run ./scripts/review-oracle-diff.sh

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
oracle=${1:-"$repo_root/crates/app/integration_resource_table_oracle.png"}
snapshot=${2:-"$repo_root/crates/app/tests/snapshots/oracle_resource_table_injected.png"}
output_dir=${3:-"$repo_root/target/visual-diffs"}
oracle_name=$(basename "$oracle" .png)
amplified_difference="$output_dir/${oracle_name}-difference-autolevel.png"
report="$output_dir/codex-visual-review.md"

"$repo_root/scripts/compare-oracle-snapshot.sh" "$oracle" "$snapshot" "$output_dir"

if ! command -v codex >/dev/null; then
    echo "Codex CLI is required for the visual review." >&2
    exit 127
fi

codex --ask-for-approval never exec \
    --ephemeral \
    --sandbox read-only \
    --cd "$repo_root" \
    --model gpt-5.6-luna \
    --image "$oracle" \
    --image "$snapshot" \
    --image "$amplified_difference" \
    --output-last-message "$report" \
    "You are reviewing a pixel-conscious UI implementation. Compare the approved oracle, the current deterministic snapshot, and the amplified ImageMagick difference map. Do not modify files or run commands. Return a concise, prioritized list of remaining visual mismatches. For each, name the screen region, describe direction and approximate size/color discrepancy, and suggest the most likely UI primitive to adjust. Ignore imperceptible antialiasing unless it affects a repeated text style." \
    </dev/null

echo "Codex review: $report"
