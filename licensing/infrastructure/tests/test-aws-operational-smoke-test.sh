#!/usr/bin/env bash
set -euo pipefail

infrastructure_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
temporary_root="$(mktemp -d)"
command_log="$temporary_root/commands.log"
service_state="$temporary_root/service-state"
queue_state="$temporary_root/queue-state"
dead_letter_marker="$temporary_root/dead-letter-marker"
system_path="$PATH"
jq_executable=$(command -v jq || find /nix/store -maxdepth 3 -type f -name jq -print -quit)

cleanup() {
  rm -rf -- "$temporary_root"
}
trap cleanup EXIT

cat >"$temporary_root/pulumi" <<'EOF'
#!/bin/sh
set -eu
printf 'pulumi|%s\n' "$*" >>"$OPERATIONAL_SMOKE_TEST_LOG"
case "$*" in
  'stack output ClusterArn') printf '%s\n' 'arn:aws:ecs:eu-west-1:123:cluster/licensing' ;;
  'stack output WorkerServiceName') printf '%s\n' 'licensing-worker' ;;
  'stack output WorkQueueUrl') printf '%s\n' 'https://sqs.example/work' ;;
  'stack output DeadLetterQueueUrl') printf '%s\n' 'https://sqs.example/dead-letter' ;;
  'stack output MaintenanceFunctionName') printf '%s\n' 'licensing-maintenance' ;;
  'stack output DeadLetterAlarmName') printf '%s\n' 'licensing-dead-letter' ;;
  'stack output AlertTopicArn') printf '%s\n' 'arn:aws:sns:eu-west-1:123:licensing-alerts' ;;
  *) exit 1 ;;
esac
EOF

cat >"$temporary_root/python3" <<'EOF'
#!/bin/sh
set -eu
printf '%s\n' '01999999-0000-7000-8000-000000000001'
EOF

cat >"$temporary_root/aws" <<'EOF'
#!/bin/sh
set -eu
printf 'aws|%s\n' "$*" >>"$OPERATIONAL_SMOKE_TEST_LOG"
case "$1 $2" in
  'ecs describe-services')
    calls=0
    [ ! -f "$OPERATIONAL_SMOKE_SERVICE_STATE" ] || calls=$(cat "$OPERATIONAL_SMOKE_SERVICE_STATE")
    calls=$((calls + 1))
    printf '%s\n' "$calls" >"$OPERATIONAL_SMOKE_SERVICE_STATE"
    if [ "${OPERATIONAL_SMOKE_SCALE_IN_DURING_PROBE:-0}" = 1 ] && [ "$calls" -ge 3 ]; then
      printf '%s\n' '0 0 0'
    else
      case "$calls" in
        1) printf '%s\n' '0 0 0' ;;
        2 | 3) printf '%s\n' '1 1 0' ;;
        *) printf '%s\n' '0 0 0' ;;
      esac
    fi
    ;;
  'sqs get-queue-attributes')
    calls=0
    [ ! -f "$OPERATIONAL_SMOKE_QUEUE_STATE" ] || calls=$(cat "$OPERATIONAL_SMOKE_QUEUE_STATE")
    calls=$((calls + 1))
    printf '%s\n' "$calls" >"$OPERATIONAL_SMOKE_QUEUE_STATE"
    if [ "$calls" -le 2 ]; then printf '%s\n' '0 1'; else printf '%s\n' '0 0'; fi
    ;;
  'lambda invoke')
    for argument in "$@"; do output_file=$argument; done
    if printf '%s' "$*" | grep -Fq smokeTestStatusId; then
      body='{"outboxDrained":true,"smokeProbe":{"probeId":"01999999-0000-7000-8000-000000000001","workId":"01999999-0001-7000-8000-000000000001","isProcessing":false,"deliveredAt":"2026-09-04T10:02:00Z"}}'
    else
      body='{"outboxDrained":true,"smokeProbe":{"probeId":"01999999-0000-7000-8000-000000000001","workId":"01999999-0001-7000-8000-000000000001","isProcessing":false,"deliveredAt":null}}'
    fi
    jq -cn --arg body "$body" '{statusCode:200,body:$body}' >"$output_file"
    printf '%s\n' '{"StatusCode":200}'
    ;;
  'sqs send-message')
    previous=''
    for argument in "$@"; do
      if [ "$previous" = '--message-body' ]; then
        printf '%s\n' "$argument" >"$OPERATIONAL_SMOKE_DEAD_LETTER_MARKER"
      fi
      previous=$argument
    done
    printf '%s\n' '{"MessageId":"smoke-message"}'
    ;;
  'sqs receive-message')
    marker=$(cat "$OPERATIONAL_SMOKE_DEAD_LETTER_MARKER")
    jq -cn --arg marker "$marker" '{Messages:[{Body:$marker,ReceiptHandle:"smoke-receipt"}]}'
    ;;
  'sqs delete-message') ;;
  'sqs change-message-visibility') ;;
  'cloudwatch describe-alarms')
    if printf '%s' "$*" | grep -Fq 'join(`,`,AlarmActions)'; then
      if [ "${OPERATIONAL_SMOKE_NO_ALARM_ACTIONS:-0}" = 1 ]; then
        printf '%s\n' 'ALARM True'
      elif [ "${OPERATIONAL_SMOKE_WRONG_ALARM_ACTION:-0}" = 1 ]; then
        printf '%s\n' 'ALARM True arn:aws:sns:eu-west-1:123:wrong-topic'
      else
        printf '%s\n' 'ALARM True arn:aws:sns:eu-west-1:123:licensing-alerts'
      fi
    else
      printf '%s\n' 'OK'
    fi
    ;;
  'sns list-subscriptions-by-topic')
    if [ "${OPERATIONAL_SMOKE_NO_CONFIRMED_SUBSCRIPTION:-0}" = 1 ]; then
      printf '%s\n' '0'
    else
      printf '%s\n' '1'
    fi
    ;;
  *) exit 1 ;;
esac
EOF

cat >"$temporary_root/sleep" <<'EOF'
#!/bin/sh
set -eu
printf 'sleep|%s\n' "$*" >>"$OPERATIONAL_SMOKE_TEST_LOG"
EOF

chmod +x "$temporary_root/pulumi" "$temporary_root/python3" \
  "$temporary_root/aws" "$temporary_root/sleep"
ln -s "$jq_executable" "$temporary_root/jq"

run_smoke() {
  env -i \
    "PATH=$temporary_root:$system_path" \
    "OPERATIONAL_SMOKE_TEST_LOG=$command_log" \
    "OPERATIONAL_SMOKE_SERVICE_STATE=$service_state" \
    "OPERATIONAL_SMOKE_QUEUE_STATE=$queue_state" \
    "OPERATIONAL_SMOKE_DEAD_LETTER_MARKER=$dead_letter_marker" \
    "OPERATIONAL_SMOKE_NO_ALARM_ACTIONS=${OPERATIONAL_SMOKE_NO_ALARM_ACTIONS:-0}" \
    "OPERATIONAL_SMOKE_WRONG_ALARM_ACTION=${OPERATIONAL_SMOKE_WRONG_ALARM_ACTION:-0}" \
    "OPERATIONAL_SMOKE_NO_CONFIRMED_SUBSCRIPTION=${OPERATIONAL_SMOKE_NO_CONFIRMED_SUBSCRIPTION:-0}" \
    "OPERATIONAL_SMOKE_SCALE_IN_DURING_PROBE=${OPERATIONAL_SMOKE_SCALE_IN_DURING_PROBE:-0}" \
    PULUMI_STACK=organization/styrhous-licensing/production \
    OPERATIONAL_SMOKE_POLL_SECONDS=1 \
    "OPERATIONAL_SMOKE_INFLIGHT_OBSERVATIONS=${OPERATIONAL_SMOKE_INFLIGHT_OBSERVATIONS:-1}" \
    bash "$infrastructure_root/aws-operational-smoke-test.sh"
}

run_smoke >"$temporary_root/success.out"
grep -Fqx 'AWS licensing operational smoke tests passed.' "$temporary_root/success.out"
grep -Fq 'lambda invoke --function-name licensing-maintenance' "$command_log"
grep -Fq 'sqs delete-message --queue-url https://sqs.example/dead-letter --receipt-handle smoke-receipt' "$command_log"
grep -Fq 'cloudwatch describe-alarms --alarm-names licensing-dead-letter' "$command_log"

: >"$command_log"
rm -f -- "$service_state" "$queue_state" "$dead_letter_marker"
export OPERATIONAL_SMOKE_NO_CONFIRMED_SUBSCRIPTION=1
set +e
run_smoke >"$temporary_root/no-subscription.out" 2>"$temporary_root/no-subscription.err"
status=$?
set -e
unset OPERATIONAL_SMOKE_NO_CONFIRMED_SUBSCRIPTION
if [[ "$status" -ne 1 ]] || ! grep -Fq \
  'alert topic has no confirmed email subscription' \
  "$temporary_root/no-subscription.err"; then
  echo "An unconfirmed alert subscription must fail the smoke test." >&2
  exit 1
fi
if grep -Fq 'sqs send-message' "$command_log"; then
  echo "The smoke test must not trigger an undeliverable alarm notification." >&2
  exit 1
fi

: >"$command_log"
rm -f -- "$service_state" "$queue_state" "$dead_letter_marker"
export OPERATIONAL_SMOKE_NO_ALARM_ACTIONS=1
set +e
run_smoke >"$temporary_root/no-actions.out" 2>"$temporary_root/no-actions.err"
status=$?
set -e
unset OPERATIONAL_SMOKE_NO_ALARM_ACTIONS
if [[ "$status" -ne 1 ]] || ! grep -Fq \
  'alarm did not enter ALARM with the exported notification topic enabled' \
  "$temporary_root/no-actions.err"; then
  echo "A dead-letter alarm without notification actions must fail the smoke test." >&2
  exit 1
fi
grep -Fq 'sqs delete-message --queue-url https://sqs.example/dead-letter --receipt-handle smoke-receipt' "$command_log"

: >"$command_log"
rm -f -- "$service_state" "$queue_state" "$dead_letter_marker"
export OPERATIONAL_SMOKE_WRONG_ALARM_ACTION=1
set +e
run_smoke >"$temporary_root/wrong-action.out" 2>"$temporary_root/wrong-action.err"
status=$?
set -e
unset OPERATIONAL_SMOKE_WRONG_ALARM_ACTION
if [[ "$status" -ne 1 ]] || ! grep -Fq \
  'alarm did not enter ALARM with the exported notification topic enabled' \
  "$temporary_root/wrong-action.err"; then
  echo "A dead-letter alarm targeting another topic must fail the smoke test." >&2
  exit 1
fi
grep -Fq 'sqs delete-message --queue-url https://sqs.example/dead-letter --receipt-handle smoke-receipt' "$command_log"

: >"$command_log"
rm -f -- "$service_state" "$queue_state" "$dead_letter_marker"
export OPERATIONAL_SMOKE_SCALE_IN_DURING_PROBE=1
export OPERATIONAL_SMOKE_INFLIGHT_OBSERVATIONS=2
set +e
run_smoke >"$temporary_root/early-scale-in.out" 2>"$temporary_root/early-scale-in.err"
status=$?
set -e
unset OPERATIONAL_SMOKE_SCALE_IN_DURING_PROBE OPERATIONAL_SMOKE_INFLIGHT_OBSERVATIONS
if [[ "$status" -ne 1 ]] || ! grep -Fq \
  'licensing worker scaled in while the smoke probe remained in flight' \
  "$temporary_root/early-scale-in.err"; then
  echo "Scaling in while the probe is in flight must fail the smoke test." >&2
  exit 1
fi

echo "AWS operational smoke script tests passed."
