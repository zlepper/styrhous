#!/usr/bin/env bash
set -euo pipefail

infrastructure_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [[ $# -gt 1 || ( $# -eq 1 && "$1" != --if-present ) ]]; then
  echo "Usage: publish-portal.sh [--if-present]" >&2
  exit 1
fi
outputs=$(pulumi -C "$infrastructure_root" stack output --json)
bucket=$(node -e 'const value = JSON.parse(require("node:fs").readFileSync(0, "utf8")).PortalBucketName; if (value != null && typeof value !== "string") process.exit(1); process.stdout.write(value ?? "");' <<<"$outputs")
distribution=$(node -e 'const value = JSON.parse(require("node:fs").readFileSync(0, "utf8")).PortalDistributionId; if (value != null && typeof value !== "string") process.exit(1); process.stdout.write(value ?? "");' <<<"$outputs")
if [[ "${1:-}" == --if-present && -z "$bucket" && -z "$distribution" ]]; then
  # A new stack has no existing documents to protect during the policy update.
  exit 0
fi
if [[ -z "$bucket" || -z "$distribution" ]]; then
  echo "Portal bucket and distribution outputs are required." >&2
  exit 1
fi
if [[ "${1:-}" == --if-present ]]; then
  current_key=$(aws s3api list-objects-v2 --bucket "$bucket" --prefix index.html \
    --query "Contents[?Key == 'index.html'].Key | [0]" --output text)
  if [[ -z "$current_key" || "$current_key" == None ]]; then
    # Infrastructure may exist after an interrupted first deployment.
    exit 0
  fi
  [[ "$current_key" == index.html ]] || exit 1
  current_document=$(aws s3 cp "s3://${bucket}/index.html" -)
  if [[ "$current_document" == *'http-equiv="content-security-policy"'* ]]; then
    # Once the CSP migration is complete, never publish a new UI before its API.
    exit 0
  fi
fi
aws s3 sync "$infrastructure_root/../portal/build" "s3://${bucket}" \
  --exclude index.html --cache-control 'public,max-age=31536000,immutable'
aws s3 cp "$infrastructure_root/../portal/build/index.html" "s3://${bucket}/index.html" \
  --cache-control 'no-cache'
invalidation=$(aws cloudfront create-invalidation \
  --distribution-id "$distribution" --paths '/*' \
  --query 'Invalidation.Id' --output text)
aws cloudfront wait invalidation-completed \
  --distribution-id "$distribution" --id "$invalidation"
