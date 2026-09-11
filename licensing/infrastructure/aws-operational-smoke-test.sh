#!/usr/bin/env bash
set -euo pipefail

: "${PULUMI_STACK:?PULUMI_STACK is required}"

poll_seconds=${OPERATIONAL_SMOKE_POLL_SECONDS:-10}
inflight_observations=${OPERATIONAL_SMOKE_INFLIGHT_OBSERVATIONS:-65}
if [[ ! "$poll_seconds" =~ ^[1-9][0-9]*$ \
  || ! "$inflight_observations" =~ ^[1-9][0-9]*$ ]]; then
  echo "Operational smoke poll settings must be positive integers." >&2
  exit 1
fi

temporary_root=$(mktemp -d)
dead_letter_marker=''
dead_letter_cleaned=false

cleanup_dead_letter_probe() {
  if [[ -z "$dead_letter_marker" || "$dead_letter_cleaned" == true ]]; then
    return
  fi
  for _ in {1..6}; do
    messages=$(aws sqs receive-message \
      --queue-url "$dead_letter_queue" \
      --max-number-of-messages 10 \
      --visibility-timeout 10 \
      --wait-time-seconds 1 \
      --output json 2>/dev/null || true)
    if [[ -z "$messages" ]]; then
      messages='{}'
    fi
    while IFS= read -r encoded; do
      [[ -n "$encoded" ]] || continue
      message=$(printf '%s' "$encoded" | base64 --decode)
      receipt=$(printf '%s' "$message" | jq -r '.ReceiptHandle')
      if [[ $(printf '%s' "$message" | jq -r '.Body') == "$dead_letter_marker" ]]; then
        aws sqs delete-message \
          --queue-url "$dead_letter_queue" \
          --receipt-handle "$receipt" >/dev/null || true
        dead_letter_cleaned=true
      else
        aws sqs change-message-visibility \
          --queue-url "$dead_letter_queue" \
          --receipt-handle "$receipt" \
          --visibility-timeout 0 >/dev/null || true
      fi
    done < <(printf '%s' "$messages" | jq -r '.Messages[]? | @base64')
    [[ "$dead_letter_cleaned" == true ]] && return
  done
}

cleanup() {
  local status=$?
  trap - EXIT INT TERM
  set +e
  cleanup_dead_letter_probe
  rm -rf -- "$temporary_root"
  exit "$status"
}
trap cleanup EXIT INT TERM

cluster=$(pulumi stack output ClusterArn)
worker_service=$(pulumi stack output WorkerServiceName)
work_queue=$(pulumi stack output WorkQueueUrl)
dead_letter_queue=$(pulumi stack output DeadLetterQueueUrl)
maintenance_function=$(pulumi stack output MaintenanceFunctionName)
dead_letter_alarm=$(pulumi stack output DeadLetterAlarmName)
alert_topic=$(pulumi stack output AlertTopicArn)

read_service_counts() {
  aws ecs describe-services \
    --cluster "$cluster" \
    --services "$worker_service" \
    --query 'services[0].[desiredCount,runningCount,pendingCount]' \
    --output text
}

read_queue_counts() {
  aws sqs get-queue-attributes \
    --queue-url "$work_queue" \
    --attribute-names \
      ApproximateNumberOfMessages \
      ApproximateNumberOfMessagesNotVisible \
    --query 'Attributes.[ApproximateNumberOfMessages,ApproximateNumberOfMessagesNotVisible]' \
    --output text
}

invoke_maintenance() {
  local payload=$1
  local name=$2
  local response_file="$temporary_root/${name}.json"
  local metadata
  metadata=$(aws lambda invoke \
    --function-name "$maintenance_function" \
    --cli-binary-format raw-in-base64-out \
    --payload "$payload" \
    "$response_file")
  if [[ $(printf '%s' "$metadata" | jq -r '.FunctionError // empty') != "" ]]; then
    echo "The maintenance Lambda returned a function error during $name." >&2
    return 1
  fi
  jq -e '.statusCode == 200' "$response_file" >/dev/null
  jq -r '.body' "$response_file"
}

worker_stopped=false
for _ in {1..120}; do
  read -r desired running pending <<<"$(read_service_counts)"
  if [[ "$desired" == 0 && "$running" == 0 && "$pending" == 0 ]]; then
    worker_stopped=true
    break
  fi
  sleep "$poll_seconds"
done
if [[ "$worker_stopped" != true ]]; then
  echo "The licensing worker did not return to zero before the smoke probe." >&2
  exit 1
fi

probe_id=$(python3 -c \
  'import secrets, time, uuid; milliseconds = int(time.time() * 1000); value = (milliseconds << 80) | (7 << 76) | (secrets.randbits(12) << 64) | (2 << 62) | secrets.randbits(62); print(uuid.UUID(int=value))')
seed_result=$(invoke_maintenance "{\"smokeTestId\":\"$probe_id\"}" seed)
jq -e --arg probe "$probe_id" '
  .outboxDrained == true
  and .smokeProbe.probeId == $probe
  and (.smokeProbe.workId | type == "string")
' <<<"$seed_result" >/dev/null

processing_observed=false
for _ in {1..120}; do
  read -r visible in_flight <<<"$(read_queue_counts)"
  read -r desired running pending <<<"$(read_service_counts)"
  if (( in_flight > 0 && desired > 0 && running > 0 )); then
    processing_observed=true
    break
  fi
  sleep "$poll_seconds"
done
if [[ "$processing_observed" != true ]]; then
  echo "The licensing worker did not scale from zero and hold the smoke probe in flight." >&2
  exit 1
fi

for ((observation = 1; observation <= inflight_observations; observation++)); do
  read -r visible in_flight <<<"$(read_queue_counts)"
  read -r desired running pending <<<"$(read_service_counts)"
  if (( in_flight < 1 || desired < 1 || running < 1 )); then
    echo "The licensing worker scaled in while the smoke probe remained in flight." >&2
    exit 1
  fi
  if (( observation < inflight_observations )); then
    sleep "$poll_seconds"
  fi
done

probe_delivered=false
for _ in {1..120}; do
  status_result=$(invoke_maintenance \
    "{\"smokeTestStatusId\":\"$probe_id\"}" \
    status)
  if jq -e --arg probe "$probe_id" '
      .smokeProbe.probeId == $probe
      and (.smokeProbe.deliveredAt | type == "string")
    ' <<<"$status_result" >/dev/null; then
    probe_delivered=true
    break
  fi
  sleep "$poll_seconds"
done
if [[ "$probe_delivered" != true ]]; then
  echo "The worker did not complete the SES-backed infrastructure smoke probe." >&2
  exit 1
fi

worker_returned_to_zero=false
for _ in {1..180}; do
  read -r visible in_flight <<<"$(read_queue_counts)"
  read -r desired running pending <<<"$(read_service_counts)"
  if [[ "$visible" == 0 && "$in_flight" == 0 \
    && "$desired" == 0 && "$running" == 0 && "$pending" == 0 ]]; then
    worker_returned_to_zero=true
    break
  fi
  sleep "$poll_seconds"
done
if [[ "$worker_returned_to_zero" != true ]]; then
  echo "The licensing worker did not return to zero after sustained idle time." >&2
  exit 1
fi

dead_letter_marker="styrhous-licensing-smoke-${GITHUB_RUN_ID:-manual}-$(date +%s)"
confirmed_subscriptions=$(aws sns list-subscriptions-by-topic \
  --topic-arn "$alert_topic" \
  --query 'length(Subscriptions[?Protocol == `email` && SubscriptionArn != `PendingConfirmation`])' \
  --output text)
if [[ ! "$confirmed_subscriptions" =~ ^[1-9][0-9]*$ ]]; then
  echo "The licensing alert topic has no confirmed email subscription." >&2
  exit 1
fi
aws sqs send-message \
  --queue-url "$dead_letter_queue" \
  --message-body "$dead_letter_marker" >/dev/null

alarm_observed=false
for _ in {1..90}; do
  read -r alarm_state actions_enabled alarm_actions <<<"$(aws cloudwatch describe-alarms \
    --alarm-names "$dead_letter_alarm" \
    --query 'MetricAlarms[0].[StateValue,ActionsEnabled,join(`,`,AlarmActions)]' \
    --output text)"
  if [[ "$alarm_state" == ALARM \
    && "$actions_enabled" == True \
    && ",$alarm_actions," == *",$alert_topic,"* ]]; then
    alarm_observed=true
    break
  fi
  sleep "$poll_seconds"
done
if [[ "$alarm_observed" != true ]]; then
  echo "The dead-letter queue alarm did not enter ALARM with the exported notification topic enabled." >&2
  exit 1
fi

cleanup_dead_letter_probe
if [[ "$dead_letter_cleaned" != true ]]; then
  echo "The deployment smoke message could not be removed from the dead-letter queue." >&2
  exit 1
fi

alarm_recovered=false
for _ in {1..90}; do
  alarm_state=$(aws cloudwatch describe-alarms \
    --alarm-names "$dead_letter_alarm" \
    --query 'MetricAlarms[0].StateValue' \
    --output text)
  if [[ "$alarm_state" == OK ]]; then
    alarm_recovered=true
    break
  fi
  sleep "$poll_seconds"
done
if [[ "$alarm_recovered" != true ]]; then
  echo "The dead-letter queue alarm did not recover after smoke-probe cleanup." >&2
  exit 1
fi

echo "AWS licensing operational smoke tests passed."
