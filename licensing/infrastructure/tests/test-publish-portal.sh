#!/usr/bin/env bash
set -euo pipefail
infrastructure_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
command_log=$(mktemp)
trap 'rm -f -- "$command_log"' EXIT
export PORTAL_TEST_LOG="$command_log"
export PORTAL_TEST_OUTPUTS='{"PortalBucketName":"portal-bucket","PortalDistributionId":"distribution"}'
export PORTAL_TEST_DOCUMENT='<html></html>'
export PORTAL_TEST_INDEX=index.html
export PORTAL_TEST_FAILURE=''
pulumi() {
  [[ "$PORTAL_TEST_FAILURE" != pulumi ]] || return 1
  printf '%s' "$PORTAL_TEST_OUTPUTS"
}
aws() {
  printf '%s\n' "$*" >>"$PORTAL_TEST_LOG"
  [[ "$*" != *"$PORTAL_TEST_FAILURE"* || -z "$PORTAL_TEST_FAILURE" ]] || return 1
  if [[ "$1 $2" == 's3api list-objects-v2' ]]; then
    printf '%s' "$PORTAL_TEST_INDEX"
  elif [[ "$1 $2" == 's3 cp' && "$4" == - ]]; then
    printf '%s' "$PORTAL_TEST_DOCUMENT"
  elif [[ "$1 $2" == 'cloudfront create-invalidation' ]]; then
    printf '%s' invalidation
  fi
}
export -f pulumi aws

bash "$infrastructure_root/publish-portal.sh" --if-present
mapfile -t commands <"$command_log"
[[ ${#commands[@]} == 6 ]]
[[ "${commands[0]}" == 's3api list-objects-v2 '* ]]
[[ "${commands[1]}" == 's3 cp s3://portal-bucket/index.html -' ]]
[[ "${commands[2]}" == 's3 sync '*' --exclude index.html --cache-control public,max-age=31536000,immutable' ]]
[[ "${commands[3]}" == 's3 cp '*'/index.html s3://portal-bucket/index.html --cache-control no-cache' ]]
[[ "${commands[4]}" == 'cloudfront create-invalidation '* ]]
[[ "${commands[5]}" == 'cloudfront wait invalidation-completed --distribution-id distribution --id invalidation' ]]

# Later deployments must not expose the new UI against an old API.
: >"$command_log"
export PORTAL_TEST_DOCUMENT='<meta http-equiv="content-security-policy" content="script-src ...">'
bash "$infrastructure_root/publish-portal.sh" --if-present
[[ $(wc -l <"$command_log") == 2 ]]
: >"$command_log"
export PORTAL_TEST_INDEX=None
bash "$infrastructure_root/publish-portal.sh" --if-present
[[ $(wc -l <"$command_log") == 1 ]]
export PORTAL_TEST_INDEX=index.html
: >"$command_log"
bash "$infrastructure_root/publish-portal.sh"
[[ $(wc -l <"$command_log") == 4 ]]

: >"$command_log"
export PORTAL_TEST_OUTPUTS='{}'
bash "$infrastructure_root/publish-portal.sh" --if-present
[[ ! -s "$command_log" ]]
if bash "$infrastructure_root/publish-portal.sh" 2>/dev/null; then exit 1; fi
export PORTAL_TEST_OUTPUTS='{"PortalBucketName":"portal-bucket"}'
if bash "$infrastructure_root/publish-portal.sh" --if-present 2>/dev/null; then exit 1; fi
[[ ! -s "$command_log" ]]

export PORTAL_TEST_OUTPUTS='{"PortalBucketName":"portal-bucket","PortalDistributionId":"distribution"}'
export PORTAL_TEST_DOCUMENT='<html></html>'
for failure in pulumi 's3api list-objects-v2' 's3 cp s3://portal-bucket/index.html -' 's3 sync' '/index.html s3://' 'cloudfront create-invalidation' 'cloudfront wait'; do
  : >"$command_log"
  export PORTAL_TEST_FAILURE="$failure"
  if bash "$infrastructure_root/publish-portal.sh" --if-present; then
    echo "Publication must fail at $failure." >&2
    exit 1
  fi
  if [[ "$failure" == pulumi ]]; then
    [[ ! -s "$command_log" ]]
  else
    [[ $(tail -1 "$command_log") == *"$failure"* ]]
  fi
done
echo "Portal publication tests passed."
