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

installation_id=${DESKTOP_SMOKE_INSTALLATION_ID:-}
if [[ -z "$installation_id" ]]; then
  installation_id=$(python3 -c \
    'import secrets, time, uuid; milliseconds = int(time.time() * 1000); value = (milliseconds << 80) | (7 << 76) | (secrets.randbits(12) << 64) | (2 << 62) | secrets.randbits(62); print(uuid.UUID(int=value))')
fi
curl --fail-with-body \
  --data-urlencode 'client_id=styrhous-desktop' \
  --data-urlencode 'scope=styrhous.desktop offline_access' \
  --data-urlencode "installation_id=$installation_id" \
  --data-urlencode 'display_name=Deployment smoke test' \
  --data-urlencode 'platform=linux' \
  --data-urlencode 'architecture=x86_64' \
  --data-urlencode 'styrhous_version=0.1.0' \
  "$origin/desktop/v1/device/authorize" >"$temporary_root/authorization.json"
jq -e --arg origin "$origin" '
  (.device_code | type == "string" and length > 0)
  and (.user_code | type == "string" and length > 0)
  and .verification_uri == ($origin + "/desktop/v1/device/verify")
  and (.verification_uri_complete | startswith($origin + "/desktop/v1/device/verify?user_code="))
  and .expires_in == 600
  and .interval == 5
' "$temporary_root/authorization.json" >/dev/null

echo "Desktop licensing origin verification passed."
