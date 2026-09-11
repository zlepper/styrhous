#!/usr/bin/env bash
set -euo pipefail

infrastructure_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
temporary_root="$(mktemp -d)"
command_log="$temporary_root/pulumi-commands.log"
expected_log="$temporary_root/expected.log"
bash_executable="$(command -v bash)"

cleanup() {
  rm -rf -- "$temporary_root"
}
trap cleanup EXIT

printf '%s\n' \
  '#!/bin/sh' \
  'set -eu' \
  'printf "%s\n" "$*" >>"$CONFIGURE_STACK_TEST_LOG"' \
  'if [ "$*" = "config get hostedOrigin" ]; then' \
  '  if [ "${CONFIGURE_STACK_TEST_HOSTED_ORIGIN_READ_FAILURE:-}" = "true" ]; then' \
  '    printf "%s\n" "Pulumi backend is unavailable" >&2' \
  '    exit 24' \
  '  elif [ "${CONFIGURE_STACK_TEST_NO_HOSTED_ORIGIN:-}" = "true" ]; then' \
  '    printf "%s\n" "error: configuration key styrhous-licensing:hostedOrigin not found" >&2' \
  '    exit 1' \
  '  else' \
  '    printf "%s\n" "https://existing.example.com"' \
  '  fi' \
  'elif [ "$*" = "config get databasePassword --show-secrets" ]; then' \
  '  if [ "${CONFIGURE_STACK_TEST_DATABASE_READ_FAILURE:-}" = "true" ]; then' \
  '    printf "%s\n" "Pulumi backend is unavailable" >&2' \
  '    exit 23' \
  '  elif [ -n "${CONFIGURE_STACK_TEST_DATABASE_PASSWORD:-}" ]; then' \
  '    printf "%s\n" "$CONFIGURE_STACK_TEST_DATABASE_PASSWORD"' \
  '  else' \
  '    printf "%s\n" "error: configuration key styrhous-licensing:databasePassword not found" >&2' \
  '    exit 1' \
  '  fi' \
  'elif [ "$*" = "config get databaseRuntimePassword --show-secrets" ]; then' \
  '  if [ "${CONFIGURE_STACK_TEST_DATABASE_RUNTIME_READ_FAILURE:-}" = "true" ]; then' \
  '    printf "%s\n" "Pulumi backend is unavailable" >&2' \
  '    exit 25' \
  '  elif [ -n "${CONFIGURE_STACK_TEST_DATABASE_RUNTIME_PASSWORD:-}" ]; then' \
  '    printf "%s\n" "$CONFIGURE_STACK_TEST_DATABASE_RUNTIME_PASSWORD"' \
  '  else' \
  '    printf "%s\n" "error: configuration key styrhous-licensing:databaseRuntimePassword not found" >&2' \
  '    exit 1' \
  '  fi' \
  'fi' \
  >"$temporary_root/pulumi"
chmod +x "$temporary_root/pulumi"

base_environment=(
  "PATH=$temporary_root:/usr/bin:/bin"
  "CONFIGURE_STACK_TEST_LOG=$command_log"
  "PULUMI_STACK=organization/styrhous-licensing/production"
  "AWS_REGION=eu-west-1"
  "DATABASE_PASSWORD=database-password"
  "DATABASE_RUNTIME_PASSWORD=database-runtime-password"
  "DESKTOP_CERTIFICATE=desktop-certificate"
  "DESKTOP_CERTIFICATE_PASSWORD=desktop-password"
  "DATA_PROTECTION_CERTIFICATE=data-protection-certificate"
  "DATA_PROTECTION_CERTIFICATE_PASSWORD=data-protection-password"
  "STRIPE_SECRET_KEY=stripe-secret"
  "STRIPE_WEBHOOK_SECRET=webhook-secret"
  "STRIPE_MONTHLY_PRICE_ID=price-monthly"
  "STRIPE_ANNUAL_PRICE_ID=price-annual"
  "STRIPE_CUSTOMER_PORTAL_CONFIGURATION_ID=portal-configuration"
  "INVITATION_FROM_ADDRESS=licensing@example.com"
  "ALERT_EMAIL_ADDRESS=alerts@example.com"
  "MAXIMUM_WORKER_TASKS=12"
  "API_IMAGE_URI=registry.example/api:current"
  "WORKER_IMAGE_URI=registry.example/worker:current"
)

env -i \
  "${base_environment[@]}" \
  GITHUB_CLIENT_ID=github-client \
  GITHUB_CLIENT_SECRET=github-secret \
  "$bash_executable" "$infrastructure_root/configure-stack.sh"

printf '%s\n' \
  "stack select --create organization/styrhous-licensing/production" \
  "config get databasePassword --show-secrets" \
  "config get databaseRuntimePassword --show-secrets" \
  "config get hostedOrigin" \
  "config set aws:region eu-west-1" \
  "config set hostedOrigin https://existing.example.com" \
  "config set apiImageUri registry.example/api:current" \
  "config set workerImageUri registry.example/worker:current" \
  "config set stripeMonthlyPriceId price-monthly" \
  "config set stripeAnnualPriceId price-annual" \
  "config set stripeCustomerPortalConfigurationId portal-configuration" \
  "config set invitationFromAddress licensing@example.com" \
  "config set alertEmailAddress alerts@example.com" \
  "config set maximumWorkerTasks 12" \
  "config set --secret databasePassword database-password" \
  "config set --secret databaseRuntimePassword database-runtime-password" \
  "config set --secret desktopCertificate desktop-certificate" \
  "config set --secret desktopCertificatePassword desktop-password" \
  "config set --secret desktopPreviousCertificates []" \
  "config set --secret dataProtectionCertificate data-protection-certificate" \
  "config set --secret dataProtectionCertificatePassword data-protection-password" \
  "config set --secret dataProtectionPreviousCertificates []" \
  "config set --secret stripeSecretKey stripe-secret" \
  "config set --secret stripeWebhookSecret webhook-secret" \
  "config set githubClientId github-client" \
  "config set --secret githubClientSecret github-secret" \
  "config rm googleClientId" \
  "config rm googleClientSecret" \
  "config rm microsoftClientId" \
  "config rm microsoftClientSecret" \
  "config rm domainName" \
  "config rm hostedZoneId" \
  "config rm certificateArn" \
  >"$expected_log"
diff -u "$expected_log" "$command_log"

for provider in GOOGLE MICROSOFT; do
  : >"$command_log"
  env -i \
    "${base_environment[@]}" \
    "${provider}_CLIENT_ID=${provider,,}-client" \
    "${provider}_CLIENT_SECRET=${provider,,}-secret" \
    "$bash_executable" "$infrastructure_root/configure-stack.sh"
  provider_key=${provider,,}
  if ! grep -Fqx "config set ${provider_key}ClientId ${provider_key}-client" "$command_log" \
    || ! grep -Fqx "config set --secret ${provider_key}ClientSecret ${provider_key}-secret" \
      "$command_log"; then
    echo "$provider must be independently usable as the only authentication provider." >&2
    exit 1
  fi
  for absent_provider in github google microsoft; do
    if [[ "$absent_provider" == "$provider_key" ]]; then
      continue
    fi
    if ! grep -Fqx "config rm ${absent_provider}ClientId" "$command_log" \
      || ! grep -Fqx "config rm ${absent_provider}ClientSecret" "$command_log"; then
      echo "Provider-only configuration must remove stale $absent_provider credentials." >&2
      exit 1
    fi
  done
done

: >"$command_log"
set +e
env -i \
  "${base_environment[@]}" \
  CONFIGURE_STACK_TEST_DATABASE_READ_FAILURE=true \
  GITHUB_CLIENT_ID=github-client \
  GITHUB_CLIENT_SECRET=github-secret \
  "$bash_executable" "$infrastructure_root/configure-stack.sh" \
  >"$temporary_root/database-read.out" \
  2>"$temporary_root/database-read.err"
status=$?
set -e
if [[ "$status" -ne 23 ]]; then
  echo "An operational Pulumi read failure must retain its failing status." >&2
  exit 1
fi
printf '%s\n' \
  "stack select --create organization/styrhous-licensing/production" \
  "config get databasePassword --show-secrets" \
  >"$expected_log"
diff -u "$expected_log" "$command_log"
if ! grep -Fq "Unable to read the existing database password from Pulumi" \
  "$temporary_root/database-read.err"; then
  echo "An operational Pulumi read failure must stop before configuration writes." >&2
  exit 1
fi

: >"$command_log"
set +e
env -i \
  "${base_environment[@]}" \
  CONFIGURE_STACK_TEST_DATABASE_PASSWORD=database-password \
  CONFIGURE_STACK_TEST_DATABASE_RUNTIME_READ_FAILURE=true \
  GITHUB_CLIENT_ID=github-client \
  GITHUB_CLIENT_SECRET=github-secret \
  "$bash_executable" "$infrastructure_root/configure-stack.sh" \
  >"$temporary_root/runtime-database-read.out" \
  2>"$temporary_root/runtime-database-read.err"
status=$?
set -e
if [[ "$status" -ne 25 ]]; then
  echo "A runtime-password Pulumi read failure must retain its failing status." >&2
  exit 1
fi
printf '%s\n' \
  "stack select --create organization/styrhous-licensing/production" \
  "config get databasePassword --show-secrets" \
  "config get databaseRuntimePassword --show-secrets" \
  >"$expected_log"
diff -u "$expected_log" "$command_log"
if ! grep -Fq "Unable to read the existing database runtime password from Pulumi" \
  "$temporary_root/runtime-database-read.err"; then
  echo "A runtime-password read failure must stop before configuration writes." >&2
  exit 1
fi

: >"$command_log"
env -i \
  "${base_environment[@]}" \
  CONFIGURE_STACK_TEST_NO_HOSTED_ORIGIN=true \
  GITHUB_CLIENT_ID=github-client \
  GITHUB_CLIENT_SECRET=github-secret \
  "$bash_executable" "$infrastructure_root/configure-stack.sh"
if ! grep -Fqx "config set hostedOrigin https://bootstrap.invalid/" "$command_log"; then
  echo "A new generated-domain stack must start from the non-routable bootstrap origin." >&2
  exit 1
fi

: >"$command_log"
set +e
env -i \
  "${base_environment[@]}" \
  CONFIGURE_STACK_TEST_HOSTED_ORIGIN_READ_FAILURE=true \
  GITHUB_CLIENT_ID=github-client \
  GITHUB_CLIENT_SECRET=github-secret \
  "$bash_executable" "$infrastructure_root/configure-stack.sh" \
  >"$temporary_root/hosted-origin-read.out" \
  2>"$temporary_root/hosted-origin-read.err"
status=$?
set -e
if [[ "$status" -ne 24 ]]; then
  echo "An operational hosted-origin read failure must retain its failing status." >&2
  exit 1
fi
if ! grep -Fq "Unable to read the existing hosted origin from Pulumi" \
  "$temporary_root/hosted-origin-read.err"; then
  echo "An operational hosted-origin read failure must stop before configuration writes." >&2
  exit 1
fi
if grep -Fq "config set hostedOrigin" "$command_log"; then
  echo "A hosted-origin read failure must not overwrite the stack origin." >&2
  exit 1
fi

: >"$command_log"
set +e
env -i \
  "${base_environment[@]}" \
  CONFIGURE_STACK_TEST_DATABASE_PASSWORD=previous-database-password \
  GITHUB_CLIENT_ID=github-client \
  GITHUB_CLIENT_SECRET=github-secret \
  "$bash_executable" "$infrastructure_root/configure-stack.sh" \
  >"$temporary_root/database-rotation.out" \
  2>"$temporary_root/database-rotation.err"
status=$?
set -e
if [[ "$status" -ne 1 ]]; then
  echo "An ordinary deployment must reject database-password rotation." >&2
  exit 1
fi
printf '%s\n' \
  "stack select --create organization/styrhous-licensing/production" \
  "config get databasePassword --show-secrets" \
  >"$expected_log"
diff -u "$expected_log" "$command_log"
if ! grep -Fq "coordinated database-credential rotation" \
  "$temporary_root/database-rotation.err"; then
  echo "The rejected database rotation must explain the safe procedure." >&2
  exit 1
fi

: >"$command_log"
set +e
env -i \
  "${base_environment[@]}" \
  CONFIGURE_STACK_TEST_DATABASE_PASSWORD=database-password \
  CONFIGURE_STACK_TEST_DATABASE_RUNTIME_PASSWORD=previous-runtime-password \
  GITHUB_CLIENT_ID=github-client \
  GITHUB_CLIENT_SECRET=github-secret \
  "$bash_executable" "$infrastructure_root/configure-stack.sh" \
  >"$temporary_root/runtime-rotation.out" \
  2>"$temporary_root/runtime-rotation.err"
status=$?
set -e
if [[ "$status" -ne 1 ]]; then
  echo "An ordinary deployment must reject database runtime-password rotation." >&2
  exit 1
fi
printf '%s\n' \
  "stack select --create organization/styrhous-licensing/production" \
  "config get databasePassword --show-secrets" \
  "config get databaseRuntimePassword --show-secrets" \
  >"$expected_log"
diff -u "$expected_log" "$command_log"
if ! grep -Fq "coordinated database-credential rotation" \
  "$temporary_root/runtime-rotation.err"; then
  echo "The rejected runtime rotation must explain the safe procedure." >&2
  exit 1
fi

required_variables=(
  PULUMI_STACK
  AWS_REGION
  DATABASE_PASSWORD
  DATABASE_RUNTIME_PASSWORD
  DESKTOP_CERTIFICATE
  DESKTOP_CERTIFICATE_PASSWORD
  DATA_PROTECTION_CERTIFICATE
  DATA_PROTECTION_CERTIFICATE_PASSWORD
  STRIPE_SECRET_KEY
  STRIPE_WEBHOOK_SECRET
  STRIPE_MONTHLY_PRICE_ID
  STRIPE_ANNUAL_PRICE_ID
  STRIPE_CUSTOMER_PORTAL_CONFIGURATION_ID
  INVITATION_FROM_ADDRESS
  ALERT_EMAIL_ADDRESS
  API_IMAGE_URI
  WORKER_IMAGE_URI
)
for missing_variable in "${required_variables[@]}"; do
  missing_environment=()
  for assignment in "${base_environment[@]}"; do
    if [[ "${assignment%%=*}" != "$missing_variable" ]]; then
      missing_environment+=("$assignment")
    fi
  done
  : >"$command_log"
  set +e
  env -i \
    "${missing_environment[@]}" \
    GITHUB_CLIENT_ID=github-client \
    GITHUB_CLIENT_SECRET=github-secret \
    "$bash_executable" "$infrastructure_root/configure-stack.sh" \
    >"$temporary_root/missing.out" \
    2>"$temporary_root/missing.err"
  status=$?
  set -e
  if [[ "$status" -ne 1 || -s "$command_log" ]]; then
    echo "Missing $missing_variable must fail before Pulumi runs." >&2
    exit 1
  fi
  if ! grep -Fq "Required deployment value is missing: $missing_variable" \
    "$temporary_root/missing.err"; then
    echo "Missing $missing_variable must identify the required value." >&2
    exit 1
  fi
done

: >"$command_log"
set +e
env -i \
  "${base_environment[@]}" \
  GITHUB_CLIENT_ID=github-client \
  GITHUB_CLIENT_SECRET=github-secret \
  DOMAIN_NAME=license.example.com \
  HOSTED_ORIGIN=https://other.example.com \
  HOSTED_ZONE_ID=hosted-zone \
  CERTIFICATE_ARN=arn:aws:acm:::certificate/test \
  "$bash_executable" "$infrastructure_root/configure-stack.sh" \
  >"$temporary_root/mismatch.out" \
  2>"$temporary_root/mismatch.err"
status=$?
set -e
if [[ "$status" -ne 1 || -s "$command_log" ]]; then
  echo "A custom-domain mismatch must fail before mutating Pulumi configuration." >&2
  exit 1
fi
if ! grep -Fq "HOSTED_ORIGIN must match" "$temporary_root/mismatch.err"; then
  echo "The custom-domain mismatch must explain the invalid origin." >&2
  exit 1
fi

: >"$command_log"
env -i \
  "${base_environment[@]}" \
  GITHUB_CLIENT_ID=github-client \
  GITHUB_CLIENT_SECRET=github-secret \
  DOMAIN_NAME=license.example.com \
  HOSTED_ORIGIN=https://license.example.com/ \
  HOSTED_ZONE_ID=hosted-zone \
  CERTIFICATE_ARN=arn:aws:acm:::certificate/test \
  "$bash_executable" "$infrastructure_root/configure-stack.sh"
for expected_command in \
  "config set hostedOrigin https://license.example.com/" \
  "config set domainName license.example.com" \
  "config set hostedZoneId hosted-zone" \
  "config set certificateArn arn:aws:acm:::certificate/test"; do
  if ! grep -Fqx "$expected_command" "$command_log"; then
    echo "Custom-domain configuration is missing: $expected_command" >&2
    exit 1
  fi
done

: >"$command_log"
set +e
env -i \
  "${base_environment[@]}" \
  "$bash_executable" "$infrastructure_root/configure-stack.sh" \
  >"$temporary_root/provider.out" \
  2>"$temporary_root/provider.err"
status=$?
set -e
if [[ "$status" -ne 1 || -s "$command_log" ]]; then
  echo "Missing authentication providers must fail before Pulumi runs." >&2
  exit 1
fi
if ! grep -Fq "At least one complete external authentication provider" \
  "$temporary_root/provider.err"; then
  echo "The missing-provider failure must explain the requirement." >&2
  exit 1
fi

: >"$command_log"
set +e
env -i \
  "${base_environment[@]}" \
  GITHUB_CLIENT_ID=github-client \
  GITHUB_CLIENT_SECRET=github-secret \
  GOOGLE_CLIENT_ID=partial-google-client \
  "$bash_executable" "$infrastructure_root/configure-stack.sh" \
  >"$temporary_root/partial-provider.out" \
  2>"$temporary_root/partial-provider.err"
status=$?
set -e
if [[ "$status" -ne 1 || -s "$command_log" ]]; then
  echo "A partial provider must fail before Pulumi runs." >&2
  exit 1
fi
if ! grep -Fq "GOOGLE requires both its client ID and client secret" \
  "$temporary_root/partial-provider.err"; then
  echo "The partial-provider failure must identify the incomplete provider." >&2
  exit 1
fi

: >"$command_log"
set +e
env -i \
  "${base_environment[@]}" \
  GITHUB_CLIENT_ID=github-client \
  GITHUB_CLIENT_SECRET=github-secret \
  MAXIMUM_WORKER_TASKS=0 \
  "$bash_executable" "$infrastructure_root/configure-stack.sh" \
  >"$temporary_root/capacity.out" \
  2>"$temporary_root/capacity.err"
status=$?
set -e
if [[ "$status" -ne 1 || -s "$command_log" ]]; then
  echo "Invalid worker capacity must fail before Pulumi runs." >&2
  exit 1
fi
if ! grep -Fq "MAXIMUM_WORKER_TASKS must be between 1 and 32" \
  "$temporary_root/capacity.err"; then
  echo "The invalid-capacity failure must explain the supported range." >&2
  exit 1
fi

echo "Stack configuration tests passed."
