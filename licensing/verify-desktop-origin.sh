#!/usr/bin/env bash
set -euo pipefail

origin=${HOSTED_LICENSE_ORIGIN:-}
case "$origin" in
  https://*) ;;
  *) echo "HOSTED_LICENSE_ORIGIN must be an HTTPS origin." >&2; exit 1 ;;
esac

origin=${origin%/}
authority=${origin#https://}
if [[ -z "$authority" \
  || "$authority" == */* \
  || "$authority" == *\?* \
  || "$authority" == *\#* \
  || "$authority" == *@* ]]; then
  echo "HOSTED_LICENSE_ORIGIN must not contain credentials, a path, query, or fragment." >&2
  exit 1
fi

if [[ "${VERIFY_DESKTOP_ORIGIN_LIVE:-true}" == "false" ]]; then
  echo "Desktop licensing origin syntax is valid."
  exit 0
fi

temporary_root=$(mktemp -d)
cleanup() {
  rm -rf -- "$temporary_root"
}
trap cleanup EXIT

curl --fail --retry 12 --retry-all-errors --retry-delay 10 \
  "$origin/desktop/v1/openapi.json" >"$temporary_root/openapi.json"
jq -e '
  .openapi == "3.1.0"
  and (.paths["/desktop/v1/device/authorize"].post != null)
  and (.paths["/desktop/v1/keys"].get != null)
' "$temporary_root/openapi.json" >/dev/null

curl --fail --retry 12 --retry-all-errors --retry-delay 10 \
  "$origin/desktop/v1/keys" >"$temporary_root/keys.json"
jq -e '.keys | type == "array" and length > 0' \
  "$temporary_root/keys.json" >/dev/null

echo "Desktop licensing origin verification passed."
