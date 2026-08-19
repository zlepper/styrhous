#!/usr/bin/env bash
set -euo pipefail

# cargo-packager accepts SVG only on Linux. Keep the cross-platform raster
# assets derived from the editable SVG source so every installer uses the same
# artwork. The @2x suffix supplies macOS's required Retina icon density.
magick -background none assets/icons/kubernetes-dev-ui.svg \
  -resize 512x512 -depth 8 PNG32:assets/icons/kubernetes-dev-ui.png
magick -background none assets/icons/kubernetes-dev-ui.svg \
  -resize 1024x1024 -depth 8 PNG32:assets/icons/kubernetes-dev-ui@2x.png
magick assets/icons/kubernetes-dev-ui.png \
  -define icon:auto-resize=256,128,64,48,32,16 assets/icons/kubernetes-dev-ui.ico
