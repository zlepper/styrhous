#!/usr/bin/env bash
set -euo pipefail

repository_root="$(git rev-parse --show-toplevel)"
cd "$repository_root"

require_line() {
  local file="$1"
  local expected="$2"
  if ! grep --fixed-strings --line-regexp --quiet -- "$expected" "$file"; then
    echo "$file must contain: $expected" >&2
    exit 1
  fi
}

require_line licensing/fly.toml '  dockerfile = "licensing/backend/Dockerfile"'
require_line licensing/fly.toml '  release_command = "dotnet Styrhous.Licensing.dll migrate"'
require_line licensing/fly.toml '  auto_stop_machines = "stop"'
require_line licensing/fly.toml '  auto_start_machines = true'
require_line licensing/fly.toml '  min_machines_running = 0'
require_line licensing/fly.toml '    path = "/health"'
require_line .github/workflows/deploy-licensing.yml \
  '        run: flyctl deploy . --ha=false --remote-only --config licensing/fly.toml'
