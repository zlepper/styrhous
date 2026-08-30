#!/usr/bin/env bash
# Generate a visual diff, then ask a fresh Codex CLI session
# to critique the three images. The session is read-only and writes only its
# final review to target/visual-diffs.
#
# Usage:
#   ./scripts/review-oracle-diff.sh <oracle.png> <snapshot.png> [output-directory]

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)

if [[ "$#" -lt 2 ]] || [[ "$#" -gt 3 ]]; then
    echo "usage: $0 <oracle.png> <snapshot.png> [output-directory]" >&2
    exit 2
fi

oracle=$1
snapshot=$2
output_dir=${3:-"$repo_root/target/visual-diffs"}
oracle_name=$(basename "$oracle" .png)
amplified_difference="$output_dir/${oracle_name}-difference-autolevel.png"
report="$output_dir/${oracle_name}-codex-visual-review.md"

"$repo_root/scripts/compare-oracle-snapshot.sh" "$oracle" "$snapshot" "$output_dir"

if ! command -v codex >/dev/null; then
    echo "Codex CLI is required for the visual review." >&2
    exit 127
fi

codex --ask-for-approval never exec \
    --ephemeral \
    --sandbox read-only \
    --cd "$repo_root" \
    --image "$oracle" \
    --image "$snapshot" \
    --image "$amplified_difference" \
    --output-last-message "$report" \
    "You are reviewing a pixel-conscious UI implementation. Compare the approved oracle, the current deterministic snapshot, and the amplified ImageMagick difference map. Do not modify files or run commands. Return a concise, prioritized list of remaining visual mismatches. For each, name the screen region, describe direction and approximate size/color discrepancy, and suggest the most likely UI primitive to adjust. Ignore imperceptible antialiasing unless it affects a repeated text style." \
    </dev/null

echo "Codex review: $report"
