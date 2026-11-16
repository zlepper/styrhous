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

keys=$(curl --fail --silent --show-error --retry 12 --retry-all-errors --retry-delay 10 \
  "$origin/desktop/v2/keys")
jq -e '.keys | type == "array" and length > 0
  and all(.[]; .kty == "OKP" and .crv == "Ed25519" and .alg == "EdDSA" and .use == "sig")' \
  <<<"$keys" >/dev/null

echo "Desktop licensing origin verification passed."
