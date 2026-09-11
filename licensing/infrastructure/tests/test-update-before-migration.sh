#!/usr/bin/env bash
set -euo pipefail

infrastructure_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
temporary_root="$(mktemp -d)"
command_log="$temporary_root/pulumi-arguments.log"
expected_log="$temporary_root/expected.log"
error_log="$temporary_root/error.log"

cleanup() {
  rm -rf -- "$temporary_root"
}
trap cleanup EXIT

printf '%s\n' \
  '#!/bin/sh' \
  'set -eu' \
  'printf "%s\n" "$@" >"$PRE_MIGRATION_TEST_LOG"' \
  >"$temporary_root/pulumi"
chmod +x "$temporary_root/pulumi"

export PATH="$temporary_root:$PATH"
export PRE_MIGRATION_TEST_LOG="$command_log"
export PULUMI_STACK="organization/styrhous-licensing/production"

bash "$infrastructure_root/update-before-migration.sh"

urn_prefix="urn:pulumi:production::styrhous-licensing::"
printf '%s\n' \
  up \
  --yes \
  --exclude \
  "${urn_prefix}aws:lambda/function:Function::licensing-api" \
  --exclude \
  "${urn_prefix}aws:ecs/taskDefinition:TaskDefinition::licensing-worker" \
  --exclude \
  "${urn_prefix}aws:lambda/function:Function::licensing-maintenance" \
  --exclude-dependents \
  >"$expected_log"
diff -u "$expected_log" "$command_log"

: >"$command_log"
unset PULUMI_STACK
set +e
bash "$infrastructure_root/update-before-migration.sh" 2>"$error_log"
status=$?
set -e
if [[ "$status" -ne 1 ]]; then
  echo "Expected a missing PULUMI_STACK to return status 1, got $status." >&2
  exit 1
fi
if [[ -s "$command_log" ]]; then
  echo "Pulumi must not run without an explicit stack." >&2
  exit 1
fi
printf '%s\n' 'PULUMI_STACK is required.' >"$expected_log"
diff -u "$expected_log" "$error_log"

echo "Pre-migration update tests passed."
