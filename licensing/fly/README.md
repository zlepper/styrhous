# Fly deployment

This is the deliberately low-cost production layout: one public Fly application and one private,
unmanaged Fly Postgres development cluster in Amsterdam. There is no NAT gateway, AWS account,
message broker, Lambda, scheduler, or separate worker.

## Bootstrap

Choose globally unique application names before the first deploy. Update `app` in
[`../fly.toml`](../fly.toml) and use its matching private hostname in the connection string:

```text
<database-app-name>.flycast
```

Create the database with Fly's PostgreSQL launcher in `ams`, select the Development configuration,
use a single 256 MB shared CPU machine and a 1 GB volume, and do not allocate a public IP. Enable
the database's supported development scale-to-zero setting:

```text
FLY_SCALE_TO_ZERO=1h
```

The one-hour idle threshold is managed by Fly. The application must have no open PostgreSQL
connections for that hour; stopping its machine closes Rebus polling connections, which is why
`fly.toml` uses `auto_stop_machines = "stop"`, not suspend.

Create the public application in `ams`. Before configuring an issuer, OAuth callback, or Stripe
URL on a custom hostname, attach that hostname to the application and follow Fly's returned DNS
instructions until its certificate is active:

```bash
flyctl certs add licenses.example.com --app <application-app-name>
# Create the DNS records shown by Fly, then wait for a successful certificate check.
flyctl certs check licenses.example.com --app <application-app-name>
```

Set the secrets only after the public hostname resolves and its certificate is active, then deploy
from the repository root:

```bash
flyctl secrets set --app <application-app-name> \
  ConnectionStrings__Licensing='Host=<database-app-name>.flycast;Port=5432;Database=postgres;Username=...;Password=...' \
  Messaging__QueueName=styrhous-licensing \
  Messaging__ErrorQueueName=styrhous-licensing-error \
  InvitationEmail__FromAddress=noreply@example.com \
  InvitationEmail__AcceptanceUrl=https://licenses.example.com/invitations/accept \
  InvitationEmail__Smtp__Host=smtp.sendgrid.net \
  InvitationEmail__Smtp__Port=587 \
  InvitationEmail__Smtp__Username=apikey \
  InvitationEmail__Smtp__Password=... \
  DataProtection__Certificate=... \
  DataProtection__CertificatePassword=... \
  DesktopProtocol__Certificate=... \
  DesktopProtocol__CertificatePassword=... \
  DesktopProtocol__Issuer=https://licenses.example.com/ \
  Stripe__SecretKey=... \
  Stripe__WebhookSecret=... \
  Stripe__MonthlyPriceId=... \
  Stripe__AnnualPriceId=... \
  Stripe__CheckoutSuccessUrl=https://licenses.example.com/billing?checkout=success \
  Stripe__CheckoutCancelUrl=https://licenses.example.com/billing?checkout=cancelled \
  Stripe__CustomerPortalConfigurationId=... \
  Stripe__CustomerPortalReturnUrl=https://licenses.example.com/billing

flyctl deploy . --ha=false --remote-only --config licensing/fly.toml
```

`--ha=false` is required for the first deployment: it creates one public Machine rather than
Fly's default redundant pair, which keeps this initial deployment to one monolith consumer.

Set one complete OAuth provider pair as well, for example
`Authentication__GitHub__ClientId` and `Authentication__GitHub__ClientSecret`. Configure every
provider's callback URL on the public origin before enabling it.

The deploy command first runs `dotnet Styrhous.Licensing.dll migrate` as Fly's release command.
It then starts the web monolith. The GitHub Actions deployment uses the same configuration and a
single `FLY_API_TOKEN` repository/environment secret.

## Idle behaviour and recovery

Fly stops the application after it becomes idle and wakes it for the next public request. Flycast
allows the private Postgres service to wake when the application connects. No background retry is
promised while both machines are stopped: a pending durable outbox row is recovered on the next
application startup. This is an intentional cost/reliability trade-off for the first customers.

Daily Fly volume snapshots are the only selected recovery mechanism. A single-machine unmanaged
development database is not highly available and snapshots are not a replacement for a tested
off-platform backup strategy.
