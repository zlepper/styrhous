#!/usr/bin/env bash
set -euo pipefail

infrastructure_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
temporary_root="$(mktemp -d)"
command_log="$temporary_root/commands.log"
system_path="$PATH"

cleanup() {
  rm -rf -- "$temporary_root"
}
trap cleanup EXIT

cat >"$temporary_root/pulumi" <<'EOF'
#!/bin/sh
set -eu
printf 'pulumi|%s\n' "$*" >>"$MIGRATION_TEST_LOG"
case "$*" in
  'stack output ClusterArn') printf '%s\n' 'arn:aws:ecs:eu-west-1:123:cluster/licensing' ;;
  'stack output MigrationTaskDefinitionArn') printf '%s\n' 'arn:aws:ecs:eu-west-1:123:task-definition/migration:1' ;;
  'stack output ProvisioningTaskDefinitionArn') printf '%s\n' 'arn:aws:ecs:eu-west-1:123:task-definition/provisioning:1' ;;
  'stack output ApplicationSecurityGroupId') printf '%s\n' 'sg-123' ;;
  'stack output PrivateSubnetIds --json') printf '%s\n' '["subnet-a","subnet-b"]' ;;
  *) exit 1 ;;
esac
EOF

cat >"$temporary_root/jq" <<'EOF'
#!/bin/sh
set -eu
cat >/dev/null
printf '%s\n' 'subnet-a,subnet-b'
EOF

cat >"$temporary_root/aws" <<'EOF'
#!/bin/sh
set -eu
printf 'aws|%s\n' "$*" >>"$MIGRATION_TEST_LOG"
case "$1 $2" in
  'ecs run-task') printf '%s\n' 'arn:aws:ecs:eu-west-1:123:task/licensing/migration-123' ;;
  'ecs wait') ;;
  'ecs describe-tasks')
    case "$*" in
      *"tasks[0].lastStatus"*) printf '%s\n' "${MIGRATION_TEST_STATUS:-STOPPED}" ;;
      *) printf '%s\n' "${MIGRATION_TEST_EXIT_CODE:-0}" ;;
    esac
    ;;
  'ecs stop-task') ;;
  *) exit 1 ;;
esac
EOF

chmod +x "$temporary_root/pulumi" "$temporary_root/jq" \
  "$temporary_root/aws"

run_migration() {
  env -i \
    "PATH=$temporary_root:$system_path" \
    "MIGRATION_TEST_LOG=$command_log" \
    "MIGRATION_TEST_STATUS=${MIGRATION_TEST_STATUS:-STOPPED}" \
    "MIGRATION_TEST_EXIT_CODE=${MIGRATION_TEST_EXIT_CODE:-0}" \
    "MIGRATION_WAIT_SECONDS=${MIGRATION_WAIT_SECONDS:-1200}" \
    MIGRATION_POLL_SECONDS=1 \
    PULUMI_STACK=organization/styrhous-licensing/production \
    bash "$infrastructure_root/run-migration-task.sh" "${1:-migration}"
}

run_migration >"$temporary_root/success.out"
grep -Fqx 'Licensing database migration completed.' "$temporary_root/success.out"
if grep -Fq 'aws|ecs stop-task' "$command_log"; then
  echo "A successful migration must not be stopped during cleanup." >&2
  exit 1
fi

: >"$command_log"
export MIGRATION_TEST_STATUS=RUNNING
export MIGRATION_WAIT_SECONDS=0
set +e
run_migration >"$temporary_root/timeout.out" 2>"$temporary_root/timeout.err"
status=$?
set -e
unset MIGRATION_TEST_STATUS MIGRATION_WAIT_SECONDS
if [[ "$status" -ne 1 ]]; then
  echo "A timed-out migration must fail." >&2
  exit 1
fi
grep -Fq 'did not stop within 0 seconds' "$temporary_root/timeout.err"
grep -Fq \
  'aws|ecs stop-task --cluster arn:aws:ecs:eu-west-1:123:cluster/licensing --task arn:aws:ecs:eu-west-1:123:task/licensing/migration-123' \
  "$command_log"
grep -Fq \
  'aws|ecs wait tasks-stopped --cluster arn:aws:ecs:eu-west-1:123:cluster/licensing --tasks arn:aws:ecs:eu-west-1:123:task/licensing/migration-123' \
  "$command_log"

: >"$command_log"
export MIGRATION_TEST_EXIT_CODE=42
set +e
run_migration >"$temporary_root/failure.out" 2>"$temporary_root/failure.err"
status=$?
set -e
unset MIGRATION_TEST_EXIT_CODE
if [[ "$status" -ne 1 ]]; then
  echo "A failed migration container must fail deployment." >&2
  exit 1
fi
grep -Fq 'exited with code 42' "$temporary_root/failure.err"
grep -Fq 'aws|ecs stop-task' "$command_log"

run_migration provisioning >"$temporary_root/provisioning.out"
grep -Fqx 'Licensing database provisioning completed.' "$temporary_root/provisioning.out"
grep -Fq -- '--task-definition arn:aws:ecs:eu-west-1:123:task-definition/provisioning:1' "$command_log"
grep -Fq 'assignPublicIp=DISABLED' "$command_log"

echo "Migration and provisioning task runner tests passed."
