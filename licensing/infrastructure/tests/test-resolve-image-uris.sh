#!/usr/bin/env bash
set -euo pipefail

infrastructure_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
temporary_root="$(mktemp -d)"
command_log="$temporary_root/commands.log"
github_environment="$temporary_root/github.env"
system_path="$PATH"

cleanup() {
  rm -rf -- "$temporary_root"
}
trap cleanup EXIT

cat >"$temporary_root/pulumi" <<'EOF'
#!/bin/sh
set -eu
printf 'pulumi|%s\n' "$*" >>"$IMAGE_URI_TEST_LOG"
case "$*" in
  'stack select --create organization/styrhous-licensing/test') exit 0 ;;
  'config get apiImageUri') key=api ;;
  'config get workerImageUri') key=worker ;;
  *) exit 1 ;;
esac
if [ -n "${key:-}" ] \
  && [ "$key" = "${IMAGE_URI_TEST_OPERATIONAL_FAILURE:-}" ]; then
  printf '%s\n' 'Pulumi backend is unavailable' >&2
  exit 23
fi
if [ "${IMAGE_URI_TEST_MISSING:-false}" = true ]; then
  printf 'error: configuration key styrhous-licensing:%sImageUri not found\n' "$key" >&2
  exit 1
fi
printf 'registry.example/%s@sha256:current\n' "$key"
EOF
chmod +x "$temporary_root/pulumi"

run_resolver() {
  env -i \
    "PATH=$temporary_root:$system_path" \
    "IMAGE_URI_TEST_LOG=$command_log" \
    "IMAGE_URI_TEST_MISSING=${IMAGE_URI_TEST_MISSING:-false}" \
    "IMAGE_URI_TEST_OPERATIONAL_FAILURE=${IMAGE_URI_TEST_OPERATIONAL_FAILURE:-}" \
    "ALLOW_IMAGE_BOOTSTRAP=${ALLOW_IMAGE_BOOTSTRAP:-false}" \
    "INITIAL_IMAGE_URI=${INITIAL_IMAGE_URI:-}" \
    PULUMI_STACK=organization/styrhous-licensing/test \
    "GITHUB_ENV=$github_environment" \
    bash "$infrastructure_root/resolve-image-uris.sh"
}

run_resolver
printf '%s\n' \
  'API_IMAGE_URI=registry.example/api@sha256:current' \
  'WORKER_IMAGE_URI=registry.example/worker@sha256:current' \
  >"$temporary_root/expected.env"
diff -u "$temporary_root/expected.env" "$github_environment"

: >"$github_environment"
export IMAGE_URI_TEST_MISSING=true
export ALLOW_IMAGE_BOOTSTRAP=true
export INITIAL_IMAGE_URI=registry.example/bootstrap:initial
run_resolver
printf '%s\n' \
  'API_IMAGE_URI=registry.example/bootstrap:initial' \
  'WORKER_IMAGE_URI=registry.example/bootstrap:initial' \
  >"$temporary_root/expected.env"
diff -u "$temporary_root/expected.env" "$github_environment"

: >"$github_environment"
export ALLOW_IMAGE_BOOTSTRAP=false
set +e
run_resolver >"$temporary_root/missing.out" 2>"$temporary_root/missing.err"
status=$?
set -e
if [[ "$status" -ne 1 || -s "$github_environment" ]]; then
  echo "A non-bootstrap preview must reject missing image configuration." >&2
  exit 1
fi
grep -Fq 'apiImageUri is not configured' "$temporary_root/missing.err"

: >"$github_environment"
export IMAGE_URI_TEST_MISSING=false
export IMAGE_URI_TEST_OPERATIONAL_FAILURE=worker
set +e
run_resolver >"$temporary_root/failure.out" 2>"$temporary_root/failure.err"
status=$?
set -e
if [[ "$status" -ne 23 || -s "$github_environment" ]]; then
  echo "An operational image-config read failure must fail without partial output." >&2
  exit 1
fi
grep -Fq 'Unable to read existing workerImageUri from Pulumi' \
  "$temporary_root/failure.err"

echo "Image URI resolver tests passed."
