#!/usr/bin/env bash

set -euo pipefail

if [ "$#" -ne 6 ]; then
  echo "usage: $0 <version> <platform> <arch> <format> <bundle> <output>" >&2
  exit 2
fi

version=$1
platform=$2
arch=$3
format=$4
bundle=$5
output=$6
signature="${bundle}.sig"

if [ ! -f "$bundle" ]; then
  echo "update bundle not found: $bundle" >&2
  exit 1
fi
if [ ! -f "$signature" ]; then
  echo "update signature not found: $signature" >&2
  exit 1
fi

bundle_name=$(basename "$bundle")
jq -n \
  --arg version "$version" \
  --arg url "https://github.com/zlepper/styrhous/releases/download/v${version}/${bundle_name}" \
  --rawfile signature "$signature" \
  --arg format "$format" \
  --arg date "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  '{version: $version, url: $url, signature: $signature, format: $format, pub_date: $date}' \
  >"$output"
