#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
exec "$repo_root/scripts/review-oracle-diff.sh" \
    "$repo_root/crates/components/tests/oracles/inspector_details_showcase.png" \
    "$repo_root/crates/components/tests/snapshots/inspector_details/showcase.png" \
    "$repo_root/target/visual-diffs/inspector-details"
