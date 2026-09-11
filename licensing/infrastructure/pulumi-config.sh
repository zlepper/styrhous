#!/usr/bin/env bash

read_optional_pulumi_config() {
  local key=$1
  local description=$2
  shift 2
  local configured
  if configured=$(pulumi config get "$key" "$@" 2>&1); then
    printf '%s' "$configured"
    return 0
  else
    local status=$?
  fi

  if [[ "$configured" == *"configuration key"* \
    && "$configured" == *"not found"* ]]; then
    return 4
  fi
  printf 'Unable to read %s from Pulumi.\n%s\n' \
    "$description" \
    "$configured" >&2
  return "$status"
}
