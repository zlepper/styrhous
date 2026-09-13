#!/usr/bin/env bash
set -euo pipefail

origin=${STYRHOUS_HOSTED_LICENSE_ORIGIN:-}
if [[ "${DESKTOP_PACKAGE_IS_TAG_RELEASE:-false}" == "true" ]]; then
  : "${origin:?STYRHOUS_HOSTED_LICENSE_ORIGIN is required for tagged releases.}"
elif [[ -z "$origin" ]]; then
  origin=https://licensing.validation.invalid
fi

origin=${origin%/}
VERIFY_DESKTOP_ORIGIN_LIVE=false \
  HOSTED_LICENSE_ORIGIN="$origin" \
  bash "$(dirname "${BASH_SOURCE[0]}")/verify-desktop-origin.sh" >&2
printf '%s\n' "$origin"
