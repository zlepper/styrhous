#!/usr/bin/env bash
set -euo pipefail

# Register this checkout's desktop metadata for local `cargo run` sessions on
# Wayland. Run again after moving the checkout so the symlinks point at its new
# location.

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "This setup script only applies to Linux desktop environments." >&2
  exit 1
fi

script_directory=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
repository_root=$(cd -- "${script_directory}/.." && pwd -P)
data_home=${XDG_DATA_HOME:-"${HOME}/.local/share"}

if [[ "${data_home}" != /* ]]; then
  echo "XDG_DATA_HOME must be an absolute path: ${data_home}" >&2
  exit 1
fi

link_into_data_home() {
  local source=$1
  local destination=$2

  mkdir -p "$(dirname -- "${destination}")"
  if [[ -e "${destination}" && ! -L "${destination}" ]]; then
    echo "Refusing to replace existing non-symlink: ${destination}" >&2
    exit 1
  fi

  ln -sfn "${source}" "${destination}"
}

link_into_data_home \
  "${repository_root}/assets/styrhous-dev.desktop" \
  "${data_home}/applications/styrhous-dev.desktop"
link_into_data_home \
  "${repository_root}/assets/icons/kubernetes-dev-ui.png" \
  "${data_home}/icons/hicolor/512x512/apps/styrhous-dev.png"
link_into_data_home \
  "${repository_root}/assets/icons/kubernetes-dev-ui@2x.png" \
  "${data_home}/icons/hicolor/512x512@2/apps/styrhous-dev.png"

if command -v kbuildsycoca6 >/dev/null; then
  kbuildsycoca6 --noincremental >/dev/null
fi

echo "Registered the local Styrhous development icon in ${data_home}."
