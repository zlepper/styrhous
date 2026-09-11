#!/usr/bin/env bash
set -euo pipefail

infrastructure_root=${BASH_SOURCE[0]%/*}
[[ "$infrastructure_root" != "${BASH_SOURCE[0]}" ]] || infrastructure_root=.
source "$infrastructure_root/pulumi-config.sh"

: "${PULUMI_STACK:?PULUMI_STACK is required}"
: "${GITHUB_ENV:?GITHUB_ENV is required}"

pulumi stack select --create "$PULUMI_STACK"

resolve_image_uri() {
  local key=$1
  local value
  if value=$(read_optional_pulumi_config "$key" "existing $key"); then
    :
  else
    local status=$?
    if [[ "$status" -ne 4 ]]; then
      return "$status"
    fi
    if [[ "${ALLOW_IMAGE_BOOTSTRAP:-false}" != true \
      || -z "${INITIAL_IMAGE_URI:-}" ]]; then
      echo "$key is not configured; bootstrap the stack through the protected deployment workflow." >&2
      return 1
    fi
    value=$INITIAL_IMAGE_URI
  fi

  if [[ -z "$value" || "$value" == *$'\n'* || "$value" == *$'\r'* ]]; then
    echo "$key must be a non-empty single-line container image URI." >&2
    return 1
  fi
  printf '%s' "$value"
}

api_image_uri=$(resolve_image_uri apiImageUri)
worker_image_uri=$(resolve_image_uri workerImageUri)
printf 'API_IMAGE_URI=%s\n' "$api_image_uri" >>"$GITHUB_ENV"
printf 'WORKER_IMAGE_URI=%s\n' "$worker_image_uri" >>"$GITHUB_ENV"
