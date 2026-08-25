#!/usr/bin/env bash
set -euo pipefail

repository_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
temporary_directory=$(mktemp -d)
trap 'rm -rf -- "${temporary_directory}"' EXIT

desktop_entry="${repository_root}/assets/styrhous-dev.desktop"
grep -Fxq 'Type=Application' "${desktop_entry}"
grep -Fxq 'Icon=styrhous-dev' "${desktop_entry}"
grep -Fxq 'NoDisplay=true' "${desktop_entry}"

data_home="${temporary_directory}/data"
packaged_entry="${data_home}/applications/styrhous.desktop"
mkdir -p "$(dirname -- "${packaged_entry}")"
touch "${packaged_entry}"
XDG_DATA_HOME="${data_home}" XDG_CACHE_HOME="${temporary_directory}/cache" \
  bash "${repository_root}/scripts/setup-local-development-icon.sh"

assert_symlink_target() {
  local link=$1
  local expected_target=$2

  test -L "${link}"
  test "$(readlink -- "${link}")" = "${expected_target}"
}

assert_symlink_target \
  "${data_home}/applications/styrhous-dev.desktop" \
  "${repository_root}/assets/styrhous-dev.desktop"
assert_symlink_target \
  "${data_home}/icons/hicolor/512x512/apps/styrhous-dev.png" \
  "${repository_root}/assets/icons/kubernetes-dev-ui.png"
assert_symlink_target \
  "${data_home}/icons/hicolor/512x512@2/apps/styrhous-dev.png" \
  "${repository_root}/assets/icons/kubernetes-dev-ui@2x.png"
test -f "${packaged_entry}"
test ! -L "${packaged_entry}"

XDG_DATA_HOME="${data_home}" XDG_CACHE_HOME="${temporary_directory}/cache" \
  bash "${repository_root}/scripts/setup-local-development-icon.sh"

collision_data_home="${temporary_directory}/collision-data"
collision_entry="${collision_data_home}/applications/styrhous-dev.desktop"
mkdir -p "$(dirname -- "${collision_entry}")"
touch "${collision_entry}"

if XDG_DATA_HOME="${collision_data_home}" bash "${repository_root}/scripts/setup-local-development-icon.sh"; then
  echo "The setup script overwrote a regular desktop entry." >&2
  exit 1
fi

test -f "${collision_entry}"
test ! -L "${collision_entry}"
