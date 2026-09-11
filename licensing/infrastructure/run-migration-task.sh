#!/usr/bin/env bash
set -euo pipefail

: "${PULUMI_STACK:?PULUMI_STACK is required}"

migration_wait_seconds=${MIGRATION_WAIT_SECONDS:-1200}
migration_poll_seconds=${MIGRATION_POLL_SECONDS:-10}
if [[ ! "$migration_wait_seconds" =~ ^[0-9]+$ \
  || ! "$migration_poll_seconds" =~ ^[1-9][0-9]*$ ]]; then
  echo "MIGRATION_WAIT_SECONDS must be a non-negative integer and MIGRATION_POLL_SECONDS must be a positive integer." >&2
  exit 1
fi

task_kind=${1:-migration}
case "$task_kind" in
  migration) task_output=MigrationTaskDefinitionArn ;;
  provisioning) task_output=ProvisioningTaskDefinitionArn ;;
  *) echo "Expected migration or provisioning task." >&2; exit 1 ;;
esac

cluster=$(pulumi stack output ClusterArn)
task_definition=$(pulumi stack output "$task_output")
security_group=$(pulumi stack output ApplicationSecurityGroupId)
subnets=$(pulumi stack output PrivateSubnetIds --json | jq -r 'join(",")')
task=''
migration_succeeded=false

cleanup() {
  local status=$?
  trap - EXIT INT TERM
  if [[ -n "$task" && "$task" != "None" && "$migration_succeeded" != true ]]; then
    echo "Stopping incomplete ${task_kind} task $task." >&2
    aws ecs stop-task \
      --cluster "$cluster" \
      --task "$task" \
      --reason "Licensing deployment ${task_kind} did not complete successfully." \
      >/dev/null || true
    aws ecs wait tasks-stopped --cluster "$cluster" --tasks "$task" || true
  fi
  exit "$status"
}
trap cleanup EXIT INT TERM

task=$(aws ecs run-task \
  --cluster "$cluster" \
  --task-definition "$task_definition" \
  --launch-type FARGATE \
  --network-configuration "awsvpcConfiguration={subnets=[${subnets}],securityGroups=[${security_group}],assignPublicIp=DISABLED}" \
  --query 'tasks[0].taskArn' \
  --output text)
if [[ -z "$task" || "$task" == "None" ]]; then
  echo "ECS did not start the licensing ${task_kind} task." >&2
  exit 1
fi

started_at=$(date +%s)
while true; do
  last_status=$(aws ecs describe-tasks \
    --cluster "$cluster" \
    --tasks "$task" \
    --query 'tasks[0].lastStatus' \
    --output text)
  if [[ "$last_status" == "STOPPED" ]]; then
    break
  fi
  if [[ -z "$last_status" || "$last_status" == "None" ]]; then
    echo "ECS stopped reporting the licensing ${task_kind} task before it completed." >&2
    exit 1
  fi
  if (( $(date +%s) - started_at >= migration_wait_seconds )); then
    echo "The licensing ${task_kind} task did not stop within ${migration_wait_seconds} seconds." >&2
    exit 1
  fi
  sleep "$migration_poll_seconds"
done

exit_code=$(aws ecs describe-tasks \
  --cluster "$cluster" \
  --tasks "$task" \
  --query 'tasks[0].containers[0].exitCode' \
  --output text)
if [[ "$exit_code" != "0" ]]; then
  echo "The licensing ${task_kind} task exited with code $exit_code." >&2
  exit 1
fi

migration_succeeded=true
echo "Licensing database ${task_kind} completed."
