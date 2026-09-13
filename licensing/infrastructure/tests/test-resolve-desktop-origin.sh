#!/usr/bin/env bash
set -euo pipefail

infrastructure_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
resolver="$infrastructure_root/resolve-desktop-origin.sh"

branch_default=$(STYRHOUS_HOSTED_LICENSE_ORIGIN= DESKTOP_PACKAGE_IS_TAG_RELEASE=false bash "$resolver")
test "$branch_default" = https://licensing.validation.invalid

branch_configured=$(STYRHOUS_HOSTED_LICENSE_ORIGIN=https://validation.styrhous.example/ \
  DESKTOP_PACKAGE_IS_TAG_RELEASE=false \
  bash "$resolver")
test "$branch_configured" = https://validation.styrhous.example

if tag_without_origin=$(STYRHOUS_HOSTED_LICENSE_ORIGIN= DESKTOP_PACKAGE_IS_TAG_RELEASE=true bash "$resolver" 2>&1); then
  echo "tagged releases must reject a missing hosted licensing origin" >&2
  exit 1
fi
case "$tag_without_origin" in
  *'STYRHOUS_HOSTED_LICENSE_ORIGIN is required for tagged releases.'*) ;;
  *)
    echo "tagged releases must explain a missing hosted licensing origin" >&2
    exit 1
    ;;
esac

tag_configured=$(STYRHOUS_HOSTED_LICENSE_ORIGIN=https://licenses.styrhous.example/ \
  DESKTOP_PACKAGE_IS_TAG_RELEASE=true \
  bash "$resolver")
test "$tag_configured" = https://licenses.styrhous.example
