#!/usr/bin/env bash

set -euo pipefail

temporary_directory=$(mktemp -d)
trap 'rm -rf "$temporary_directory"' EXIT

bundle="$temporary_directory/styrhous.AppImage"
manifest="$temporary_directory/update.json"
printf 'fixture update payload' >"$bundle"
printf 'fixture updater signature\n' >"$bundle.sig"

bash scripts/generate-update-manifest.sh \
  '0.0.1-alpha.1' \
  linux \
  x86_64 \
  appimage \
  "$bundle" \
  "$manifest"

jq -e '
  .version == "0.0.1-alpha.1"
  and .url == "https://github.com/zlepper/styrhous/releases/download/v0.0.1-alpha.1/styrhous.AppImage"
  and .signature == "fixture updater signature\n"
  and .format == "appimage"
  and (.pub_date | type == "string" and length > 0)
' "$manifest" >/dev/null
