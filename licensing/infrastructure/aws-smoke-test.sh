#!/usr/bin/env bash
set -euo pipefail

required=(PULUMI_STACK STRIPE_WEBHOOK_SECRET OAUTH_PROVIDER)
for variable in "${required[@]}"; do
  if [[ -z "${!variable:-}" ]]; then
    echo "Required smoke-test value is missing: $variable" >&2
    exit 1
  fi
done

case "$OAUTH_PROVIDER" in
  github | google | microsoft) ;;
  *) echo "OAUTH_PROVIDER must be github, google, or microsoft." >&2; exit 1 ;;
esac

temporary_root=$(mktemp -d)
cleanup() {
  rm -rf -- "$temporary_root"
}
trap cleanup EXIT

origin=$(pulumi stack output HostedOrigin)
maintenance_function=$(pulumi stack output MaintenanceFunctionName)
stack_name=${PULUMI_STACK##*/}

HOSTED_LICENSE_ORIGIN="$origin" \
  bash "$(dirname "${BASH_SOURCE[0]}")/verify-desktop-origin.sh"

curl --fail --retry 12 --retry-all-errors --retry-delay 10 "$origin/health" \
  >"$temporary_root/health.json"
jq -e '.status == "healthy"' "$temporary_root/health.json" >/dev/null
curl --fail --retry 12 --retry-all-errors --retry-delay 10 "$origin/" \
  >"$temporary_root/index.html"
grep -Fq 'Account overview · Styrhous' "$temporary_root/index.html"

curl --silent --show-error \
  --dump-header "$temporary_root/oauth.headers" \
  --output /dev/null \
  "$origin/auth/sign-in/$OAUTH_PROVIDER?returnUrl=/account"
grep -Eq '^HTTP/[^ ]+ 302' "$temporary_root/oauth.headers"
callback="$origin/auth/provider-callback/$OAUTH_PROVIDER"
encoded_callback=$(jq -nr --arg callback "$callback" '$callback | @uri')
grep -Fi 'location:' "$temporary_root/oauth.headers" | grep -Fq "$encoded_callback"
grep -Fiq 'set-cookie:' "$temporary_root/oauth.headers"

timestamp=$(date +%s)
event_id="evt_styrhous_smoke_${GITHUB_RUN_ID:-manual}_$timestamp"
payload=$(printf \
  '{"id":"%s","object":"event","created":%s,"type":"styrhous.smoke","data":{"object":{}}}' \
  "$event_id" \
  "$timestamp")
signature=$(
  STRIPE_SMOKE_PAYLOAD="$payload" \
  STRIPE_SMOKE_TIMESTAMP="$timestamp" \
  python3 -c 'import hashlib, hmac, os; print(hmac.new(os.environ["STRIPE_WEBHOOK_SECRET"].encode(), (os.environ["STRIPE_SMOKE_TIMESTAMP"] + "." + os.environ["STRIPE_SMOKE_PAYLOAD"]).encode(), hashlib.sha256).hexdigest())'
)
curl --fail-with-body \
  --header 'Content-Type: application/json' \
  --header "Stripe-Signature: t=$timestamp,v1=$signature" \
  --data "$payload" \
  "$origin/api/webhooks/stripe" \
  >"$temporary_root/webhook.json"
jq -e '.reasonCode == "ignored"' "$temporary_root/webhook.json" >/dev/null

invocation=$(aws lambda invoke \
  --function-name "$maintenance_function" \
  --cli-binary-format raw-in-base64-out \
  --payload '{}' \
  "$temporary_root/maintenance.json")
if [[ $(printf '%s' "$invocation" | jq -r '.FunctionError // empty') != "" ]]; then
  echo "The maintenance Lambda returned a function error." >&2
  exit 1
fi
jq -e '.statusCode == 200' "$temporary_root/maintenance.json" >/dev/null

alarm_count=0
for attempt in {1..12}; do
  alarm_count=$(aws resourcegroupstaggingapi get-resources \
    --resource-type-filters cloudwatch:alarm \
    --tag-filters \
      Key=application,Values=styrhous-licensing \
      "Key=environment,Values=$stack_name" \
    --query 'length(ResourceTagMappingList)' \
    --output text)
  if [[ "$alarm_count" =~ ^[0-9]+$ && "$alarm_count" -ge 17 ]]; then
    break
  fi
  if [[ "$attempt" -lt 12 ]]; then
    sleep 10
  fi
done
if [[ ! "$alarm_count" =~ ^[0-9]+$ || "$alarm_count" -lt 17 ]]; then
  echo "Expected at least 17 tagged licensing alarms, found $alarm_count." >&2
  exit 1
fi

echo "AWS licensing smoke tests passed."
