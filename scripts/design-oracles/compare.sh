#!/usr/bin/env bash
# Compares egui snapshots against the Harbour design oracles with Kineprism.
#
# Usage: scripts/design-oracles/compare.sh [--manifest PATH] [ID ...]
#
# Boards come from docs/design-oracles/harbour/manifest.toml; without IDs every board with a
# snapshot is compared. Reports land in target/design-oracles/<ID>/ and the summary in
# target/design-oracles/scoreboard.md. Set KINEPRISM to override the Kineprism binary.
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if ! command -v node >/dev/null; then
  exec nix develop "$script_dir/../.." --command node "$script_dir/compare.mjs" "$@"
fi
exec node "$script_dir/compare.mjs" "$@"
