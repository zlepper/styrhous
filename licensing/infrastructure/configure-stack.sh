#!/usr/bin/env bash
set -euo pipefail

infrastructure_root=${BASH_SOURCE[0]%/*}
[[ "$infrastructure_root" != "${BASH_SOURCE[0]}" ]] || infrastructure_root=.
source "$infrastructure_root/pulumi-config.sh"

required=(
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
for variable in "${required[@]}"; do
  if [[ -z "${!variable:-}" ]]; then
    echo "Required deployment value is missing: $variable" >&2
    exit 1
  fi
done

for provider in GITHUB GOOGLE MICROSOFT; do
  client_id_variable="${provider}_CLIENT_ID"
  client_secret_variable="${provider}_CLIENT_SECRET"
  if [[ (-n "${!client_id_variable:-}" && -z "${!client_secret_variable:-}") \
    || (-z "${!client_id_variable:-}" && -n "${!client_secret_variable:-}") ]]; then
    echo "$provider requires both its client ID and client secret." >&2
    exit 1
  fi
done

if [[ (-z "${GITHUB_CLIENT_ID:-}" || -z "${GITHUB_CLIENT_SECRET:-}") \
  && (-z "${GOOGLE_CLIENT_ID:-}" || -z "${GOOGLE_CLIENT_SECRET:-}") \
  && (-z "${MICROSOFT_CLIENT_ID:-}" || -z "${MICROSOFT_CLIENT_SECRET:-}") ]]; then
  echo "At least one complete external authentication provider is required." >&2
  exit 1
fi

if [[ -n "${MAXIMUM_WORKER_TASKS:-}" \
  && (! "$MAXIMUM_WORKER_TASKS" =~ ^[0-9]+$ \
    || "$MAXIMUM_WORKER_TASKS" -lt 1 \
    || "$MAXIMUM_WORKER_TASKS" -gt 32) ]]; then
  echo "MAXIMUM_WORKER_TASKS must be between 1 and 32." >&2
  exit 1
fi

if [[ -n "${DOMAIN_NAME:-}" ]]; then
  for variable in HOSTED_ORIGIN HOSTED_ZONE_ID CERTIFICATE_ARN; do
    if [[ -z "${!variable:-}" ]]; then
      echo "Custom-domain deployment value is missing: $variable" >&2
      exit 1
    fi
  done
  expected_origin="https://${DOMAIN_NAME}/"
  if [[ "${HOSTED_ORIGIN%/}/" != "$expected_origin" ]]; then
    echo "HOSTED_ORIGIN must match the configured HTTPS domain." >&2
    exit 1
  fi
fi

pulumi stack select --create "$PULUMI_STACK"

if configured_database_password=$(read_optional_pulumi_config \
  databasePassword \
  "the existing database password" \
  --show-secrets); then
  :
else
  read_status=$?
  if [[ "$read_status" -eq 4 ]]; then
    configured_database_password=""
  else
    exit "$read_status"
  fi
fi
if [[ -n "$configured_database_password" \
  && "$configured_database_password" != "$DATABASE_PASSWORD" ]]; then
  echo "DATABASE_PASSWORD cannot be changed by the application deployment; use the coordinated database-credential rotation procedure." >&2
  exit 1
fi

if configured_database_runtime_password=$(read_optional_pulumi_config \
  databaseRuntimePassword \
  "the existing database runtime password" \
  --show-secrets); then
  :
else
  read_status=$?
  if [[ "$read_status" -eq 4 ]]; then
    configured_database_runtime_password=""
  else
    exit "$read_status"
  fi
fi
if [[ -n "$configured_database_runtime_password" \
  && "$configured_database_runtime_password" != "$DATABASE_RUNTIME_PASSWORD" ]]; then
  echo "DATABASE_RUNTIME_PASSWORD cannot be changed by the application deployment; use the coordinated database-credential rotation procedure." >&2
  exit 1
fi

if [[ -z "${DOMAIN_NAME:-}" ]]; then
  if [[ -z "${HOSTED_ORIGIN:-}" ]]; then
    if configured_hosted_origin=$(read_optional_pulumi_config \
      hostedOrigin \
      "the existing hosted origin"); then
      HOSTED_ORIGIN=$configured_hosted_origin
    else
      read_status=$?
      if [[ "$read_status" -eq 4 ]]; then
        HOSTED_ORIGIN=""
      else
        exit "$read_status"
      fi
    fi
  fi
  HOSTED_ORIGIN=${HOSTED_ORIGIN:-https://bootstrap.invalid/}
fi

pulumi config set aws:region "$AWS_REGION"
pulumi config set hostedOrigin "$HOSTED_ORIGIN"
pulumi config set apiImageUri "$API_IMAGE_URI"
pulumi config set workerImageUri "$WORKER_IMAGE_URI"
pulumi config set stripeMonthlyPriceId "$STRIPE_MONTHLY_PRICE_ID"
pulumi config set stripeAnnualPriceId "$STRIPE_ANNUAL_PRICE_ID"
pulumi config set stripeCustomerPortalConfigurationId \
  "$STRIPE_CUSTOMER_PORTAL_CONFIGURATION_ID"
pulumi config set invitationFromAddress "$INVITATION_FROM_ADDRESS"
pulumi config set alertEmailAddress "$ALERT_EMAIL_ADDRESS"
if [[ -n "${MAXIMUM_WORKER_TASKS:-}" ]]; then
  pulumi config set maximumWorkerTasks "$MAXIMUM_WORKER_TASKS"
else
  pulumi config rm maximumWorkerTasks || true
fi
if [[ -z "$configured_database_password" ]]; then
  pulumi config set --secret databasePassword "$DATABASE_PASSWORD"
fi
if [[ -z "$configured_database_runtime_password" ]]; then
  pulumi config set --secret databaseRuntimePassword "$DATABASE_RUNTIME_PASSWORD"
fi
pulumi config set --secret desktopCertificate "$DESKTOP_CERTIFICATE"
pulumi config set --secret desktopCertificatePassword "$DESKTOP_CERTIFICATE_PASSWORD"
pulumi config set --secret desktopPreviousCertificates \
  "${DESKTOP_PREVIOUS_CERTIFICATES_JSON:-[]}"
pulumi config set --secret dataProtectionCertificate "$DATA_PROTECTION_CERTIFICATE"
pulumi config set --secret dataProtectionCertificatePassword \
  "$DATA_PROTECTION_CERTIFICATE_PASSWORD"
pulumi config set --secret dataProtectionPreviousCertificates \
  "${DATA_PROTECTION_PREVIOUS_CERTIFICATES_JSON:-[]}"
pulumi config set --secret stripeSecretKey "$STRIPE_SECRET_KEY"
pulumi config set --secret stripeWebhookSecret "$STRIPE_WEBHOOK_SECRET"

configure_provider() {
  local provider=$1
  local client_id=$2
  local client_secret=$3
  local id_key="${provider}ClientId"
  local secret_key="${provider}ClientSecret"
  if [[ -n "$client_id" && -n "$client_secret" ]]; then
    pulumi config set "$id_key" "$client_id"
    pulumi config set --secret "$secret_key" "$client_secret"
  else
    pulumi config rm "$id_key" || true
    pulumi config rm "$secret_key" || true
  fi
}

configure_provider github "${GITHUB_CLIENT_ID:-}" "${GITHUB_CLIENT_SECRET:-}"
configure_provider google "${GOOGLE_CLIENT_ID:-}" "${GOOGLE_CLIENT_SECRET:-}"
configure_provider microsoft "${MICROSOFT_CLIENT_ID:-}" "${MICROSOFT_CLIENT_SECRET:-}"

if [[ -n "${DOMAIN_NAME:-}" ]]; then
  pulumi config set domainName "$DOMAIN_NAME"
  pulumi config set hostedZoneId "$HOSTED_ZONE_ID"
  pulumi config set certificateArn "$CERTIFICATE_ARN"
else
  pulumi config rm domainName || true
  pulumi config rm hostedZoneId || true
  pulumi config rm certificateArn || true
fi
