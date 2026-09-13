#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${PULUMI_STACK:-}" ]]; then
  echo "PULUMI_STACK is required." >&2
  exit 1
fi

stack_name=${PULUMI_STACK##*/}
urn_prefix="urn:pulumi:${stack_name}::styrhous-licensing::"

# Reconcile the complete stack except compute that could execute application code.
# Existing runtimes retain their previous image and pinned secret version while the
# migration task and all of its dependencies move to the new desired state.
pulumi up --yes \
  --exclude "${urn_prefix}aws:lambda/function:Function::licensing-api" \
  --exclude "${urn_prefix}aws:ecs/taskDefinition:TaskDefinition::licensing-worker" \
  --exclude "${urn_prefix}aws:lambda/function:Function::licensing-maintenance" \
  --exclude-dependents
