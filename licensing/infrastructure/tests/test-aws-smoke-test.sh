#!/usr/bin/env bash
set -euo pipefail

infrastructure_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
temporary_root="$(mktemp -d)"
command_log="$temporary_root/commands.log"
alarm_state="$temporary_root/alarm-state"
bash_executable="$(command -v bash)"
system_path="$PATH"
jq_executable=$(command -v jq || find /nix/store -maxdepth 3 -type f -name jq -print -quit)
python_executable=$(command -v python3 || find /nix/store -maxdepth 3 -type f -name python3 -print -quit)
if [[ -z "$jq_executable" || -z "$python_executable" ]]; then
  echo "The smoke test requires jq and Python 3." >&2
  exit 1
fi

cleanup() {
  rm -rf -- "$temporary_root"
}
trap cleanup EXIT

cat >"$temporary_root/pulumi" <<'EOF'
#!/bin/sh
set -eu
printf 'pulumi|%s\n' "$*" >>"$AWS_SMOKE_TEST_LOG"
case "$*" in
  'stack output HostedOrigin') printf '%s\n' 'https://license.example.com' ;;
  'stack output MaintenanceFunctionName') printf '%s\n' 'licensing-maintenance-test' ;;
  *) exit 1 ;;
esac
EOF

cat >"$temporary_root/curl" <<'EOF'
#!/bin/sh
set -eu
printf 'curl|%s\n' "$*" >>"$AWS_SMOKE_TEST_LOG"
header_file=''
url=''
while [ "$#" -gt 0 ]; do
  case "$1" in
    --dump-header)
      header_file=$2
      shift 2
      ;;
    --output | --header | --data | --retry | --retry-delay)
      shift 2
      ;;
    --*) shift ;;
    *) url=$1; shift ;;
  esac
done
case "$url" in
  */health) printf '%s\n' '{"status":"healthy"}' ;;
  */desktop/v1/openapi.json)
    printf '%s\n' '{"openapi":"3.1.0","paths":{"/desktop/v1/device/authorize":{"post":{}},"/desktop/v1/keys":{"get":{}}}}'
    ;;
  */desktop/v1/keys)
    if [ "${AWS_SMOKE_TEST_BROKEN_DESKTOP:-0}" = 1 ]; then
      printf '%s\n' '{"keys":[]}'
    else
      printf '%s\n' '{"keys":[{"kid":"test"}]}'
    fi
    ;;
  */desktop/v1/device/authorize)
    printf '%s\n' '{"device_code":"device","user_code":"ABCD-EFGH","verification_uri":"https://license.example.com/desktop/v1/device/verify","verification_uri_complete":"https://license.example.com/desktop/v1/device/verify?user_code=ABCD-EFGH","expires_in":600,"interval":5}'
    ;;
  */auth/sign-in/github*)
    cat >"$header_file" <<'HEADERS'
HTTP/2 302
location: https://github.com/login/oauth/authorize?redirect_uri=https%3A%2F%2Flicense.example.com%2Fauth%2Fprovider-callback%2Fgithub
set-cookie: .AspNetCore.Correlation=opaque; Secure; HttpOnly; SameSite=None

HEADERS
    ;;
  */auth/sign-in/google*)
    cat >"$header_file" <<'HEADERS'
HTTP/2 302
location: https://accounts.google.com/o/oauth2/v2/auth?redirect_uri=https%3A%2F%2Flicense.example.com%2Fauth%2Fprovider-callback%2Fgoogle
set-cookie: .AspNetCore.Correlation=opaque; Secure; HttpOnly; SameSite=None

HEADERS
    ;;
  */auth/sign-in/microsoft*)
    cat >"$header_file" <<'HEADERS'
HTTP/2 302
location: https://login.microsoftonline.com/common/oauth2/v2.0/authorize?redirect_uri=https%3A%2F%2Flicense.example.com%2Fauth%2Fprovider-callback%2Fmicrosoft
set-cookie: .AspNetCore.Correlation=opaque; Secure; HttpOnly; SameSite=None

HEADERS
    ;;
  */api/webhooks/stripe) printf '%s\n' '{"reasonCode":"ignored"}' ;;
  https://license.example.com/) printf '%s\n' '<title>Account overview · Styrhous</title>' ;;
  *) exit 1 ;;
esac
EOF

cat >"$temporary_root/aws" <<'EOF'
#!/bin/sh
set -eu
printf 'aws|%s\n' "$*" >>"$AWS_SMOKE_TEST_LOG"
case "$1 $2" in
  'lambda invoke')
    for argument in "$@"; do output_file=$argument; done
    if [ "${AWS_SMOKE_TEST_LAMBDA_FAILURE:-0}" = 1 ]; then
      printf '%s\n' '{"statusCode":500}' >"$output_file"
      printf '%s\n' '{"StatusCode":200,"FunctionError":"Unhandled"}'
    else
      printf '%s\n' '{"statusCode":200}' >"$output_file"
      printf '%s\n' '{"StatusCode":200}'
    fi
    ;;
  'resourcegroupstaggingapi get-resources')
    attempts=0
    if [ -f "$AWS_SMOKE_TEST_ALARM_STATE" ]; then
      attempts=$(cat "$AWS_SMOKE_TEST_ALARM_STATE")
    fi
    attempts=$((attempts + 1))
    printf '%s\n' "$attempts" >"$AWS_SMOKE_TEST_ALARM_STATE"
    if [ "${AWS_SMOKE_TEST_ALARMS_NEVER_READY:-0}" = 1 ]; then
      printf '%s\n' 16
    elif [ "$attempts" -eq 1 ]; then
      printf '%s\n' 16
    else
      printf '%s\n' 17
    fi
    ;;
  *) exit 1 ;;
esac
EOF

cat >"$temporary_root/sleep" <<'EOF'
#!/bin/sh
set -eu
printf 'sleep|%s\n' "$*" >>"$AWS_SMOKE_TEST_LOG"
EOF

chmod +x "$temporary_root/pulumi" "$temporary_root/curl" \
  "$temporary_root/aws" "$temporary_root/sleep"
ln -s "$jq_executable" "$temporary_root/jq"
ln -s "$python_executable" "$temporary_root/python3"

env -i \
  "PATH=$temporary_root:$system_path" \
  "AWS_SMOKE_TEST_LOG=$command_log" \
  "AWS_SMOKE_TEST_ALARM_STATE=$alarm_state" \
  PULUMI_STACK=organization/styrhous-licensing/production \
  STRIPE_WEBHOOK_SECRET=whsec-test \
  OAUTH_PROVIDER=github \
  GITHUB_RUN_ID=1234 \
  "$bash_executable" "$infrastructure_root/aws-smoke-test.sh" \
  >"$temporary_root/success.out"

grep -Fqx 'AWS licensing smoke tests passed.' "$temporary_root/success.out"
grep -Fqx 'sleep|10' "$command_log"
grep -Fq 'lambda invoke --function-name licensing-maintenance-test' "$command_log"
installation_id=$(sed -n 's/.*installation_id=\([^ ]*\).*/\1/p' "$command_log" | head -1)
SMOKE_INSTALLATION_ID="$installation_id" "$python_executable" -c \
  'import os, uuid; value = uuid.UUID(os.environ["SMOKE_INSTALLATION_ID"]); assert value.version == 7 and value.variant == uuid.RFC_4122'
if [[ "$(cat "$alarm_state")" -ne 2 ]]; then
  echo "The smoke test must retry until tagged alarms become visible." >&2
  exit 1
fi

for provider in google microsoft; do
  : >"$command_log"
  rm -f -- "$alarm_state"
  env -i \
    "PATH=$temporary_root:$system_path" \
    "AWS_SMOKE_TEST_LOG=$command_log" \
    "AWS_SMOKE_TEST_ALARM_STATE=$alarm_state" \
    PULUMI_STACK=organization/styrhous-licensing/production \
    STRIPE_WEBHOOK_SECRET=whsec-test \
    "OAUTH_PROVIDER=$provider" \
    GITHUB_RUN_ID=1234 \
    "$bash_executable" "$infrastructure_root/aws-smoke-test.sh" \
    >"$temporary_root/$provider-success.out"
  grep -Fqx 'AWS licensing smoke tests passed.' "$temporary_root/$provider-success.out"
  grep -Fq "auth/sign-in/$provider?returnUrl=/account" "$command_log"
done

set +e
env -i \
  "PATH=$temporary_root:$system_path" \
  "AWS_SMOKE_TEST_LOG=$command_log" \
  "AWS_SMOKE_TEST_ALARM_STATE=$alarm_state" \
  PULUMI_STACK=organization/styrhous-licensing/production \
  STRIPE_WEBHOOK_SECRET=whsec-test \
  "$bash_executable" "$infrastructure_root/aws-smoke-test.sh" \
  >"$temporary_root/missing.out" \
  2>"$temporary_root/missing.err"
status=$?
set -e
if [[ "$status" -ne 1 ]] || ! grep -Fq \
  'Required smoke-test value is missing: OAUTH_PROVIDER' \
  "$temporary_root/missing.err"; then
  echo "Missing smoke-test configuration must fail with a useful error." >&2
  exit 1
fi

: >"$command_log"
rm -f -- "$alarm_state"
set +e
env -i \
  "PATH=$temporary_root:$system_path" \
  "AWS_SMOKE_TEST_LOG=$command_log" \
  "AWS_SMOKE_TEST_ALARM_STATE=$alarm_state" \
  AWS_SMOKE_TEST_LAMBDA_FAILURE=1 \
  PULUMI_STACK=organization/styrhous-licensing/production \
  STRIPE_WEBHOOK_SECRET=whsec-test \
  OAUTH_PROVIDER=github \
  DESKTOP_SMOKE_INSTALLATION_ID=01999999-0000-7000-8000-000000000001 \
  "$bash_executable" "$infrastructure_root/aws-smoke-test.sh" \
  >"$temporary_root/lambda-failure.out" \
  2>"$temporary_root/lambda-failure.err"
status=$?
set -e
if [[ "$status" -ne 1 ]] || ! grep -Fq \
  'The maintenance Lambda returned a function error.' \
  "$temporary_root/lambda-failure.err"; then
  echo "A maintenance Lambda function error must fail the smoke test." >&2
  exit 1
fi

: >"$command_log"
rm -f -- "$alarm_state"
set +e
env -i \
  "PATH=$temporary_root:$system_path" \
  "AWS_SMOKE_TEST_LOG=$command_log" \
  "AWS_SMOKE_TEST_ALARM_STATE=$alarm_state" \
  AWS_SMOKE_TEST_ALARMS_NEVER_READY=1 \
  PULUMI_STACK=organization/styrhous-licensing/production \
  STRIPE_WEBHOOK_SECRET=whsec-test \
  OAUTH_PROVIDER=github \
  DESKTOP_SMOKE_INSTALLATION_ID=01999999-0000-7000-8000-000000000001 \
  "$bash_executable" "$infrastructure_root/aws-smoke-test.sh" \
  >"$temporary_root/alarm-failure.out" \
  2>"$temporary_root/alarm-failure.err"
status=$?
set -e
if [[ "$status" -ne 1 ]] || ! grep -Fq \
  'Expected at least 17 tagged licensing alarms, found 16.' \
  "$temporary_root/alarm-failure.err"; then
  echo "An incomplete alarm deployment must fail after bounded retries." >&2
  exit 1
fi
if [[ "$(cat "$alarm_state")" -ne 12 ]]; then
  echo "The alarm smoke test must stop after twelve attempts." >&2
  exit 1
fi

: >"$command_log"
rm -f -- "$alarm_state"
set +e
env -i \
  "PATH=$temporary_root:$system_path" \
  "AWS_SMOKE_TEST_LOG=$command_log" \
  "AWS_SMOKE_TEST_ALARM_STATE=$alarm_state" \
  AWS_SMOKE_TEST_BROKEN_DESKTOP=1 \
  PULUMI_STACK=organization/styrhous-licensing/production \
  STRIPE_WEBHOOK_SECRET=whsec-test \
  OAUTH_PROVIDER=github \
  "$bash_executable" "$infrastructure_root/aws-smoke-test.sh" \
  >"$temporary_root/desktop-failure.out" \
  2>"$temporary_root/desktop-failure.err"
status=$?
set -e
if [[ "$status" -eq 0 ]]; then
  echo "A desktop contract regression must fail the smoke test." >&2
  exit 1
fi

echo "AWS smoke script tests passed."
