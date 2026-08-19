#!/usr/bin/env bash
set -euo pipefail

# Keep this in sync with package.metadata.packager.appimage.libs in Cargo.toml.
# The Debian package declares the corresponding runtime dependency separately.
sudo apt-get update
sudo apt-get install --yes libxkbcommon-x11-0
