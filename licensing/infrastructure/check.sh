#!/usr/bin/env bash
set -euo pipefail

infrastructure_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$infrastructure_root"

dotnet restore Styrhous.Licensing.Infrastructure.slnx --locked-mode
dotnet format Styrhous.Licensing.Infrastructure.slnx --verify-no-changes --no-restore
dotnet build Styrhous.Licensing.Infrastructure.slnx \
  --configuration Release \
  --no-restore \
  --warnaserror
dotnet test Styrhous.Licensing.Infrastructure.slnx \
  --configuration Release \
  --no-build \
  --no-restore
bash -n \
  configure-stack.sh \
  pulumi-config.sh \
  resolve-image-uris.sh \
  run-migration-task.sh \
  update-before-migration.sh \
  publish-portal.sh \
  verify-desktop-origin.sh \
  aws-smoke-test.sh \
  aws-operational-smoke-test.sh
bash tests/test-configure-stack.sh
bash tests/test-resolve-image-uris.sh
bash tests/test-update-before-migration.sh
bash tests/test-publish-portal.sh
bash tests/test-run-migration-task.sh
bash tests/test-aws-smoke-test.sh
bash tests/test-aws-operational-smoke-test.sh
